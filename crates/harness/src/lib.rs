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
//! This crate is the **trait crate only** — no impls, no policy. Pull
//! in `harness-cli` for agentic-CLI adapters, `harness-workflow` for
//! the orchestration primitive, `harness-tower` for the tower::Service
//! bridge. Each adapter family lives in its own crate so consumers pay
//! only for what they use.

mod capabilities;

pub use capabilities::Capabilities;

use async_trait::async_trait;

/// A normalized async caller. Generic over the request type with
/// associated `Response` / `Error` — the same shape as `tower::Service`,
/// minus the `poll_ready` / `Future` plumbing that `harness-tower`
/// adds back as a bridge when consumers want middleware composition.
#[async_trait]
pub trait Harness<Request>: Send + Sync
where
    Request: Send + 'static,
{
    type Response: Send;
    type Error: Send + std::error::Error;

    fn name(&self) -> &str;
    fn capabilities(&self) -> Capabilities;
    async fn run(&self, req: Request) -> Result<Self::Response, Self::Error>;
}
