//! harness — normalize the wire shape of agentic CLI tools.
//!
//! A small abstraction over CLIs like `goose run` and `claude -p` so a
//! consumer can issue one prompt against many harnesses without coding
//! to each one's argv shape, JSON output schema, and exit-code semantics.
//!
//! The crate intentionally does only this: subprocess + parse + normalize.
//! It has no opinion about retries, cost caps, verify gates, persistence,
//! or coordination. Consumers compose those concerns themselves (a
//! `tower::Service` impl on top of `Harness` is straightforward and
//! deliberately not bundled).
//!
//! # Example
//! ```no_run
//! use harness::{Goose, Harness, HarnessRequest};
//! # async fn run() -> Result<(), Box<dyn std::error::Error>> {
//! let h = Goose::new();
//! let req = HarnessRequest::new("Summarize the README in two sentences.")
//!     .workdir(".")
//!     .max_turns(3);
//! let res = h.run(req).await?;
//! println!("messages={} wall={:?}", res.messages, res.wall);
//! # Ok(()) }
//! ```
//!
//! # Capabilities
//!
//! Different harnesses expose different knobs. Inspect what an adapter
//! supports before issuing requests rather than empirically probing:
//!
//! ```
//! use harness::{ClaudeCode, Goose, Harness};
//! let g = Goose::new();
//! let c = ClaudeCode::new();
//! assert!(g.capabilities().supports_json_output);
//! assert!(c.capabilities().reports_cost);
//! ```

mod adapter;
mod request;
#[cfg(feature = "tower")]
mod service;

pub use adapter::{ClaudeCode, Goose};
pub use request::{Capabilities, HarnessError, HarnessRequest, RunResult};
#[cfg(feature = "tower")]
pub use service::HarnessService;

use async_trait::async_trait;

/// A normalized agentic-CLI runner. Each implementation wraps one CLI
/// (`goose`, `claude`, ...) and presents the same `run` shape.
#[async_trait]
pub trait Harness: Send + Sync {
    /// Stable identifier — `"goose"`, `"claude-code"`, etc. Used for
    /// telemetry and consumer routing decisions; should never change
    /// across versions of the same adapter.
    fn name(&self) -> &str;

    /// What this adapter can honor / populate. Cheap (const-ish) — does
    /// not spawn the subprocess. Consumers route on this before sending
    /// a request that depends on, say, `supports_max_turns`.
    fn capabilities(&self) -> Capabilities;

    /// Execute one prompt-to-completion run against the underlying CLI.
    async fn run(&self, req: HarnessRequest) -> Result<RunResult, HarnessError>;
}
