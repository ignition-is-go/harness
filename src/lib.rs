//! harness — Tower-style generic primitive for stochastic / expensive
//! async calls (LLM consults, RAG lookups, anything where you hand a
//! request to a remote thing and get an answer back).
//!
//! The trait is generic over the request type with the response and
//! error as associated types — same shape as `tower::Service`:
//!
//! ```ignore
//! pub trait Harness<Request> {
//!     type Response;
//!     type Error;
//!     async fn run(&self, req: Request) -> Result<Self::Response, Self::Error>;
//! }
//! ```
//!
//! So one trait family covers:
//! - subprocess-wrapped agentic CLIs (`Goose`, `ClaudeCode`) — `harness::cli`
//! - workflow orchestration that composes other Harnesses — `harness::workflow`
//! - whatever comes next (HTTP-direct API clients, local model runners,
//!   RAG retrievers, ...) without changing the trait
//!
//! # Example: agentic CLI
//! ```no_run
//! use harness::{Harness, cli::{Goose, PromptRequest}};
//! # async fn run() -> Result<(), Box<dyn std::error::Error>> {
//! let h = Goose::new();
//! let req = PromptRequest::new("Summarize the README in two sentences.")
//!     .workdir(".")
//!     .max_turns(3);
//! let res = h.run(req).await?;
//! println!("messages={} wall={:?}", res.messages, res.wall);
//! # Ok(()) }
//! ```

pub mod cli;
pub mod workflow;

mod capabilities;
#[cfg(feature = "tower")]
mod service;

pub use capabilities::Capabilities;
#[cfg(feature = "tower")]
pub use service::HarnessService;

use async_trait::async_trait;

/// A normalized async caller. Generic over the request type with
/// associated `Response` / `Error` — the same shape as `tower::Service`,
/// minus the `poll_ready` / `Future` plumbing that the bridge in
/// `service` adds back when the `tower` feature is on.
///
/// Implementors should keep `run` cheap to call concurrently
/// (clone-able internal state) so wrappers like
/// [`tower::limit::ConcurrencyLimit`] work without surprises.
#[async_trait]
pub trait Harness<Request>: Send + Sync
where
    Request: Send + 'static,
{
    type Response: Send;
    type Error: Send + std::error::Error;

    /// Stable identifier — `"goose"`, `"claude-code"`, `"workflow"`, etc.
    /// Used for telemetry and consumer routing decisions; should never
    /// change across versions of the same impl.
    fn name(&self) -> &str;

    /// What this impl can honor / populate. Cheap (const-ish) — does
    /// not invoke the underlying call. Consumers route on this before
    /// sending a request that depends on, say, `supports_max_turns`.
    fn capabilities(&self) -> Capabilities;

    /// Execute one request-to-response run.
    async fn run(&self, req: Request) -> Result<Self::Response, Self::Error>;
}
