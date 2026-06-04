//! Adapter-compliance suite.
//!
//! Every CLI-family `Harness` impl in this crate is exercised through
//! the same assertions so a new adapter can't silently violate the
//! contract.
//!
//! Two layers:
//!
//!   - **Unit (always run)**: cheap, no subprocess. Verifies metadata —
//!     `name()` is non-empty / stable, `capabilities()` is internally
//!     consistent (no `reports_tokens` without `supports_json_output`).
//!     These run in `cargo test` with no environment setup.
//!
//!   - **Real-CLI (gated)**: actually invoke the wrapped CLI with a
//!     trivial prompt. Skipped unless `HARNESS_TEST_REAL_CLI=1` is set
//!     AND the binary is on PATH. Designed for local smoke and ad-hoc
//!     CI lanes that pre-install the CLIs; never assumed in default CI.

use harness_cli::{ClaudeCode, CliError, Goose, PromptRequest, RunResult};
use harness::{Capabilities, Harness};
use std::time::Duration;

/// All CLI-family adapters share `Harness<PromptRequest, Response = RunResult, Error = CliError>`,
/// so the suite is generic over that one shape.
type CliAdapter = Box<
    dyn Harness<PromptRequest, Response = RunResult, Error = CliError>,
>;

fn all_adapters() -> Vec<CliAdapter> {
    vec![Box::new(Goose::new()), Box::new(ClaudeCode::new())]
}

#[test]
fn every_adapter_has_a_non_empty_name() {
    for a in all_adapters() {
        assert!(!a.name().is_empty(), "adapter name must be non-empty");
    }
}

#[test]
fn adapter_names_are_unique() {
    let adapters = all_adapters();
    let mut seen: Vec<String> = Vec::new();
    for a in &adapters {
        let n = a.name().to_owned();
        assert!(
            !seen.contains(&n),
            "duplicate adapter name {n:?}; names must be unique for routing"
        );
        seen.push(n);
    }
}

#[test]
fn capabilities_are_internally_consistent() {
    for a in all_adapters() {
        let c: Capabilities = a.capabilities();
        assert!(
            c.is_consistent(),
            "{}: capabilities are inconsistent ({:?})",
            a.name(),
            c
        );
    }
}

#[test]
fn goose_known_capabilities() {
    let c = Goose::new().capabilities();
    assert!(c.supports_json_output);
    assert!(c.supports_max_turns);
    assert!(c.supports_model_override);
    assert!(c.supports_workdir);
    // Goose does not expose a cost figure; consumers must compute.
    assert!(!c.reports_cost);
}

#[test]
fn claude_code_known_capabilities() {
    let c = ClaudeCode::new().capabilities();
    assert!(c.supports_json_output);
    assert!(c.supports_max_turns);
    assert!(c.supports_model_override);
    assert!(c.supports_workdir);
    assert!(c.reports_tokens);
    assert!(c.reports_cost);
}

// ---------------------------------------------------------------------
// Real-CLI lane. Gated; opts in via env var; tolerates missing binary.
// ---------------------------------------------------------------------

fn real_cli_enabled() -> bool {
    std::env::var("HARNESS_TEST_REAL_CLI")
        .map(|v| v == "1")
        .unwrap_or(false)
}

fn which(bin: &str) -> bool {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d.join(bin).is_file()))
        .unwrap_or(false)
}

async fn smoke_one<H>(h: &H)
where
    H: Harness<PromptRequest, Response = RunResult, Error = CliError>,
{
    let req = PromptRequest::new("Reply with just the word READY.")
        .max_turns(1)
        .timeout(Duration::from_secs(90));
    let res = h.run(req).await.unwrap_or_else(|e| {
        panic!("{}: run() failed: {e}", h.name());
    });
    assert_eq!(res.exit_code, 0, "{}: non-zero exit", h.name());
}

#[tokio::test]
async fn real_goose_smoke() {
    if !real_cli_enabled() || !which("goose") {
        eprintln!("skip: HARNESS_TEST_REAL_CLI != 1 or `goose` missing");
        return;
    }
    smoke_one(&Goose::new()).await;
}

#[tokio::test]
async fn real_claude_smoke() {
    if !real_cli_enabled() || !which("claude") {
        eprintln!("skip: HARNESS_TEST_REAL_CLI != 1 or `claude` missing");
        return;
    }
    smoke_one(&ClaudeCode::new()).await;
}
