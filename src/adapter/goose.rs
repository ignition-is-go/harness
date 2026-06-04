use crate::{Harness, HarnessError, HarnessRequest, RunResult};
use async_trait::async_trait;
use std::process::Stdio;
use std::time::Instant;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

/// Wraps the `goose` CLI in `run` subcommand:
/// `goose run -i FILE --output-format json --no-session [...]`.
pub struct Goose {
    /// Path to the `goose` binary. Defaults to `"goose"` (PATH lookup).
    bin: String,
    /// Optional provider override (`--provider`). Useful when the same
    /// model name maps to multiple providers in goose config.
    provider: Option<String>,
}

impl Default for Goose {
    fn default() -> Self {
        Self::new()
    }
}

impl Goose {
    pub fn new() -> Self {
        Self {
            bin: "goose".into(),
            provider: None,
        }
    }

    pub fn bin(mut self, path: impl Into<String>) -> Self {
        self.bin = path.into();
        self
    }

    pub fn provider(mut self, p: impl Into<String>) -> Self {
        self.provider = Some(p.into());
        self
    }
}

#[async_trait]
impl Harness for Goose {
    fn name(&self) -> &str {
        "goose"
    }

    async fn run(&self, req: HarnessRequest) -> Result<RunResult, HarnessError> {
        // goose -i FILE reads the instruction file; stdin "-" also works
        // but writing to a tempfile gives nicer process-tree introspection
        // when something hangs.
        let mut tmp = tempfile::NamedTempFile::new().map_err(HarnessError::Io)?;
        std::io::Write::write_all(&mut tmp, req.prompt.as_bytes()).map_err(HarnessError::Io)?;
        let prompt_path = tmp.path().to_owned();

        let mut cmd = Command::new(&self.bin);
        cmd.arg("run")
            .arg("-i")
            .arg(&prompt_path)
            .arg("--output-format")
            .arg("json")
            .arg("--no-session")
            .arg("--quiet");

        if let Some(n) = req.max_turns {
            cmd.arg("--max-turns").arg(n.to_string());
        }
        if let Some(m) = &req.model {
            cmd.arg("--model").arg(m);
        }
        if let Some(p) = &self.provider {
            cmd.arg("--provider").arg(p);
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
            let child = cmd.spawn().map_err(HarnessError::Spawn)?;
            child
                .wait_with_output()
                .await
                .map_err(HarnessError::Io)
        };

        let output = match req.timeout {
            Some(d) => match tokio::time::timeout(d, child_fut).await {
                Ok(r) => r?,
                Err(_) => return Err(HarnessError::Timeout(d)),
            },
            None => child_fut.await?,
        };

        let wall = start.elapsed();
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        let exit_code = output.status.code().unwrap_or(-1);

        // goose --output-format json emits a single JSON object with at
        // least a `messages` array and `total_token_usage` block. Both
        // shape and field names are subject to change across goose
        // versions, so parse best-effort and leave 0 on miss.
        let (messages, tokens_in, tokens_out) = parse_goose_json(&stdout);

        if exit_code != 0 {
            return Err(HarnessError::NonZeroExit {
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
            wall,
        })
    }
}

fn parse_goose_json(s: &str) -> (u32, u64, u64) {
    // Some goose configs prepend non-JSON banner lines before the JSON
    // object even with --quiet (depending on extensions). Locate the
    // last `{...}` block and try that.
    let trimmed = s.trim();
    let start = trimmed.find('{').unwrap_or(0);
    let candidate = &trimmed[start..];

    let v: serde_json::Value = match serde_json::from_str(candidate) {
        Ok(v) => v,
        Err(_) => return (0, 0, 0),
    };

    let messages = v
        .get("messages")
        .and_then(|m| m.as_array())
        .map(|a| a.len() as u32)
        .unwrap_or(0);

    let tokens_in = v
        .pointer("/total_token_usage/input_tokens")
        .or_else(|| v.pointer("/usage/input_tokens"))
        .and_then(|n| n.as_u64())
        .unwrap_or(0);
    let tokens_out = v
        .pointer("/total_token_usage/output_tokens")
        .or_else(|| v.pointer("/usage/output_tokens"))
        .and_then(|n| n.as_u64())
        .unwrap_or(0);

    (messages, tokens_in, tokens_out)
}

// Silence unused warning for the AsyncWriteExt import on platforms where
// only synchronous tempfile writes are exercised by tests.
#[allow(dead_code)]
fn _ensure_async_write_imported() {
    fn _f<W: AsyncWriteExt + Unpin>(_w: W) {}
}
