//! Generic shell-command verifier.
//!
//! Wraps an arbitrary shell command as a `Verifier`. Exit code 0 means
//! pass; non-zero means fail. The command is run from `env.workdir()`
//! with the consumer-provided environment variables merged into the
//! subprocess env — the runner uses this to surface task-specific
//! context (e.g. `AGENT_FIX_RULE_ID` / `AGENT_FIX_FLOW_STEP` for
//! ui-watchdog-shape issues) without the verifier needing to know
//! about it.
//!
//! Structured failure detail is not captured (the shell only gives us
//! exit code + stderr tail) — for richer feedback, write a domain
//! `Verifier` that emits a `VerifyOutcome::structured`.

use crate::environment::Environment;
use crate::task::Task;
use crate::verifier::{Verifier, VerifierError, VerifyOutcome};
use async_trait::async_trait;
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;

/// Run a shell command from `env.workdir()` to verify the task is
/// solved. `cmd` is the literal command string, executed via
/// `sh -c <cmd>`. Inherits env from the runner process AND from
/// whatever the Workflow passes through.
pub struct ShellVerifier {
    cmd: String,
    timeout: Option<Duration>,
    /// Extra env applied on every verify. Combined with the per-run
    /// env the Workflow passes via `WorkflowRequest::env`.
    extra_env: Vec<(String, String)>,
}

impl ShellVerifier {
    pub fn new(cmd: impl Into<String>) -> Self {
        Self {
            cmd: cmd.into(),
            timeout: None,
            extra_env: Vec::new(),
        }
    }

    pub fn timeout(mut self, d: Duration) -> Self {
        self.timeout = Some(d);
        self
    }

    pub fn env(mut self, k: impl Into<String>, v: impl Into<String>) -> Self {
        self.extra_env.push((k.into(), v.into()));
        self
    }
}

#[async_trait]
impl Verifier for ShellVerifier {
    async fn verify(
        &self,
        env: &dyn Environment,
        _task: &dyn Task,
    ) -> Result<VerifyOutcome, VerifierError> {
        let mut cmd = Command::new("sh");
        cmd.arg("-c")
            .arg(&self.cmd)
            .current_dir(env.workdir())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        for (k, v) in &self.extra_env {
            cmd.env(k, v);
        }

        let child_fut = async {
            let child = cmd.spawn().map_err(VerifierError::Io)?;
            child.wait_with_output().await.map_err(VerifierError::Io)
        };

        let output = match self.timeout {
            Some(d) => match tokio::time::timeout(d, child_fut).await {
                Ok(r) => r?,
                Err(_) => return Err(VerifierError::Timeout),
            },
            None => child_fut.await?,
        };

        let passed = output.status.success();
        let stderr_tail = if !passed {
            let s = String::from_utf8_lossy(&output.stderr);
            let tail: String = s
                .chars()
                .rev()
                .take(800)
                .collect::<String>()
                .chars()
                .rev()
                .collect();
            Some(tail)
        } else {
            None
        };

        Ok(VerifyOutcome {
            passed,
            stderr_tail,
            structured: None,
        })
    }
}
