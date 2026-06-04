use crate::prompt::{CliError, PromptRequest, RunResult};
use harness::{Capabilities, Harness};
use async_trait::async_trait;
use std::process::Stdio;
use std::time::Instant;
use tokio::process::Command;

/// Wraps `claude -p PROMPT --output-format json` (Claude Code's
/// non-interactive print mode).
pub struct ClaudeCode {
    bin: String,
    max_budget_usd: Option<f64>,
}

impl Default for ClaudeCode {
    fn default() -> Self {
        Self::new()
    }
}

impl ClaudeCode {
    pub fn new() -> Self {
        Self {
            bin: "claude".into(),
            max_budget_usd: None,
        }
    }

    pub fn bin(mut self, path: impl Into<String>) -> Self {
        self.bin = path.into();
        self
    }

    pub fn max_budget_usd(mut self, usd: f64) -> Self {
        self.max_budget_usd = Some(usd);
        self
    }
}

#[async_trait]
impl Harness<PromptRequest> for ClaudeCode {
    type Response = RunResult;
    type Error = CliError;

    fn name(&self) -> &str {
        "claude-code"
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            supports_max_turns: true,
            supports_model_override: true,
            supports_json_output: true,
            reports_tokens: true,
            reports_cost: true,
            supports_workdir: true,
        }
    }

    async fn run(&self, req: PromptRequest) -> Result<RunResult, CliError> {
        let mut cmd = Command::new(&self.bin);
        cmd.arg("-p")
            .arg(&req.prompt)
            .arg("--output-format")
            .arg("json");

        if let Some(n) = req.max_turns {
            cmd.arg("--max-turns").arg(n.to_string());
        }
        if let Some(m) = &req.model {
            cmd.arg("--model").arg(m);
        }
        if let Some(b) = self.max_budget_usd {
            cmd.arg("--max-budget-usd").arg(b.to_string());
        }
        if let Some(dir) = &req.workdir {
            cmd.current_dir(dir);
        }
        for (k, v) in &req.env {
            cmd.env(k, v);
        }

        cmd.stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let start = Instant::now();
        let child_fut = async {
            let child = cmd.spawn().map_err(CliError::Spawn)?;
            child.wait_with_output().await.map_err(CliError::Io)
        };

        let output = match req.timeout {
            Some(d) => match tokio::time::timeout(d, child_fut).await {
                Ok(r) => r?,
                Err(_) => return Err(CliError::Timeout(d)),
            },
            None => child_fut.await?,
        };

        let wall = start.elapsed();
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        let exit_code = output.status.code().unwrap_or(-1);

        let (messages, tokens_in, tokens_out, cost_usd) = parse_claude_json(&stdout);

        if exit_code != 0 {
            return Err(CliError::NonZeroExit {
                code: exit_code,
                stdout,
                stderr,
            });
        }

        Ok(RunResult {
            stdout,
            stderr,
            exit_code,
            messages,
            tokens_in,
            tokens_out,
            cost_usd,
            wall,
        })
    }
}

fn parse_claude_json(s: &str) -> (u32, u64, u64, f64) {
    let v: serde_json::Value = match serde_json::from_str(s.trim()) {
        Ok(v) => v,
        Err(_) => return (0, 0, 0, 0.0),
    };

    let messages = v
        .get("num_turns")
        .and_then(|n| n.as_u64())
        .unwrap_or(0) as u32;
    let tokens_in = v
        .pointer("/usage/input_tokens")
        .and_then(|n| n.as_u64())
        .unwrap_or(0);
    let tokens_out = v
        .pointer("/usage/output_tokens")
        .and_then(|n| n.as_u64())
        .unwrap_or(0);
    let cost_usd = v
        .get("total_cost_usd")
        .and_then(|n| n.as_f64())
        .unwrap_or(0.0);

    (messages, tokens_in, tokens_out, cost_usd)
}
