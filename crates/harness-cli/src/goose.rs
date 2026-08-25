use crate::prompt::{CliError, PromptRequest, RunResult};
use async_trait::async_trait;
use harness::{Capabilities, Harness};
use std::process::Stdio;
use std::time::Instant;
use tokio::process::Command;

/// Wraps the `goose` CLI in `run` subcommand:
/// `goose run -i FILE --output-format json --no-session [...]`.
pub struct Goose {
    bin: String,
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
impl Harness<PromptRequest> for Goose {
    type Response = RunResult;
    type Error = CliError;

    fn name(&self) -> &str {
        "goose"
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            supports_max_turns: true,
            supports_model_override: true,
            supports_json_output: true,
            reports_tokens: true,
            reports_cost: false,
            supports_workdir: true,
        }
    }

    async fn run(&self, req: PromptRequest) -> Result<RunResult, CliError> {
        let mut tmp = tempfile::NamedTempFile::new().map_err(CliError::Io)?;
        std::io::Write::write_all(&mut tmp, req.prompt.as_bytes()).map_err(CliError::Io)?;
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

        let (messages, tokens_in, tokens_out) = parse_goose_json(&stdout);

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
            cost_usd: 0.0,
            wall,
        })
    }
}

fn parse_goose_json(s: &str) -> (u32, u64, u64) {
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

    // Goose's --output-format json schema varies by provider. Observed
    // shapes:
    //   ollama-direct:    /metadata/{input,output,total}_tokens
    //   openai-compat (LiteLLM): /metadata/total_tokens (often null;
    //                            no per-direction split)
    //   anthropic-direct: /usage/{input,output}_tokens
    //   misc extensions:  /total_token_usage/{input,output}_tokens
    // Fall through every candidate; 0 if the provider didn't report.
    let tokens_in = v
        .pointer("/total_token_usage/input_tokens")
        .or_else(|| v.pointer("/usage/input_tokens"))
        .or_else(|| v.pointer("/metadata/input_tokens"))
        .and_then(|n| n.as_u64())
        .unwrap_or(0);
    let tokens_out = v
        .pointer("/total_token_usage/output_tokens")
        .or_else(|| v.pointer("/usage/output_tokens"))
        .or_else(|| v.pointer("/metadata/output_tokens"))
        .and_then(|n| n.as_u64())
        .unwrap_or(0);

    (messages, tokens_in, tokens_out)
}

#[cfg(test)]
mod tests {
    use super::parse_goose_json;

    #[test]
    fn parses_ollama_direct_shape() {
        let s = r#"{"messages":[{"role":"user"},{"role":"assistant"}],"metadata":{"input_tokens":5197,"output_tokens":18,"total_tokens":5215,"status":"completed"}}"#;
        let (msgs, ti, to) = parse_goose_json(s);
        assert_eq!(msgs, 2);
        assert_eq!(ti, 5197);
        assert_eq!(to, 18);
    }

    #[test]
    fn parses_openai_compat_shape() {
        // LiteLLM-routed goose only fills total_tokens, no breakdown.
        let s = r#"{"messages":[{"role":"user"},{"role":"assistant"}],"metadata":{"total_tokens":null,"status":"completed"}}"#;
        let (msgs, ti, to) = parse_goose_json(s);
        assert_eq!(msgs, 2);
        assert_eq!(ti, 0);
        assert_eq!(to, 0);
    }

    #[test]
    fn parses_anthropic_direct_shape() {
        let s = r#"{"messages":[],"usage":{"input_tokens":100,"output_tokens":50}}"#;
        let (_msgs, ti, to) = parse_goose_json(s);
        assert_eq!(ti, 100);
        assert_eq!(to, 50);
    }

    #[test]
    fn handles_banner_before_json() {
        let s = "\u{1b}[33mwarn: thing\u{1b}[0m\n{\"messages\":[],\"usage\":{\"input_tokens\":7,\"output_tokens\":3}}";
        let (_msgs, ti, to) = parse_goose_json(s);
        assert_eq!(ti, 7);
        assert_eq!(to, 3);
    }

    #[test]
    fn returns_zeros_for_garbage() {
        let (msgs, ti, to) = parse_goose_json("not json");
        assert_eq!((msgs, ti, to), (0, 0, 0));
    }
}
