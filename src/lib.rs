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

mod adapter;
mod request;

pub use adapter::{ClaudeCode, Goose};
pub use request::{HarnessError, HarnessRequest, RunResult};

use async_trait::async_trait;

/// A normalized agentic-CLI runner. Each implementation wraps one CLI
/// (`goose`, `claude`, ...) and presents the same `run` shape.
#[async_trait]
pub trait Harness: Send + Sync {
    /// Stable identifier — `"goose"`, `"claude-code"`, etc. Used for
    /// telemetry and consumer routing decisions; should never change
    /// across versions of the same adapter.
    fn name(&self) -> &str;

    /// Execute one prompt-to-completion run against the underlying CLI.
    async fn run(&self, req: HarnessRequest) -> Result<RunResult, HarnessError>;
}
