//! Smoke test — run the same trivial prompt through Goose and Claude Code,
//! print the normalized RunResult fields side-by-side.
//!
//! Requires `goose` and/or `claude` on PATH. Skips an adapter when its
//! binary is missing rather than failing the example.
//!
//! Usage:
//!     cargo run --example smoke
//!
//! Env knobs:
//!     SMOKE_PROMPT       — override the prompt (default: a tiny request)
//!     SMOKE_GOOSE_MODEL  — passes through as `--model`
//!     SMOKE_CLAUDE_MODEL — passes through as `--model`

use harness_cli::{ClaudeCode, CliError, Goose, PromptRequest, RunResult};
use harness::Harness;
use std::time::Duration;

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let prompt = std::env::var("SMOKE_PROMPT")
        .unwrap_or_else(|_| "Reply with just the word READY.".into());

    let mut req = PromptRequest::new(&prompt)
        .max_turns(1)
        .timeout(Duration::from_secs(60));
    if let Ok(cwd) = std::env::current_dir() {
        req = req.workdir(cwd);
    }

    if which("goose") {
        let mut goose_req = req.clone();
        if let Ok(m) = std::env::var("SMOKE_GOOSE_MODEL") {
            goose_req = goose_req.model(m);
        }
        run_one(&Goose::new(), goose_req).await;
    } else {
        eprintln!("[skip] goose not on PATH");
    }

    if which("claude") {
        let mut claude_req = req.clone();
        if let Ok(m) = std::env::var("SMOKE_CLAUDE_MODEL") {
            claude_req = claude_req.model(m);
        }
        run_one(&ClaudeCode::new(), claude_req).await;
    } else {
        eprintln!("[skip] claude not on PATH");
    }

    Ok(())
}

async fn run_one<H>(h: &H, req: PromptRequest)
where
    H: Harness<PromptRequest, Response = RunResult, Error = CliError>,
{
    let caps = h.capabilities();
    println!(
        "\n== {} == (max_turns={} json={} tokens={} cost={})",
        h.name(),
        caps.supports_max_turns,
        caps.supports_json_output,
        caps.reports_tokens,
        caps.reports_cost,
    );
    match h.run(req).await {
        Ok(r) => {
            println!(
                "ok  exit={} messages={} tokens={}→{} cost=${:.4} wall={:?}",
                r.exit_code, r.messages, r.tokens_in, r.tokens_out, r.cost_usd, r.wall
            );
            let preview: String = r.stdout.chars().take(500).collect();
            if !preview.is_empty() {
                println!("---stdout---\n{}\n", preview);
            }
        }
        Err(e) => {
            eprintln!("err {}: {}", h.name(), e);
        }
    }
}

fn which(bin: &str) -> bool {
    let path = match std::env::var_os("PATH") {
        Some(p) => p,
        None => return false,
    };
    for entry in std::env::split_paths(&path) {
        if entry.join(bin).is_file() {
            return true;
        }
    }
    false
}
