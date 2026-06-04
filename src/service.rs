//! Tower integration. Enabled by the `tower` cargo feature.
//!
//! Wraps any `Harness` as a `tower::Service<HarnessRequest>` so consumers
//! can stack tower layers (retry, timeout, concurrency limit, custom
//! middleware) around a harness without us writing each one.
//!
//! # Example
//! ```no_run
//! # #[cfg(feature = "tower")]
//! # async fn run() -> Result<(), Box<dyn std::error::Error>> {
//! use harness::{Goose, HarnessRequest, HarnessService};
//! use tower::{Service, ServiceExt};
//! use tower::limit::ConcurrencyLimit;
//!
//! let mut svc = ConcurrencyLimit::new(
//!     HarnessService::new(Goose::new()),
//!     2,
//! );
//! let req = HarnessRequest::new("Reply with just READY.").max_turns(1);
//! let res = svc.ready().await?.call(req).await?;
//! println!("messages={}", res.messages);
//! # Ok(()) }
//! ```

use crate::{Harness, HarnessError, HarnessRequest, RunResult};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tower::Service;

/// `tower::Service` adapter for any `Harness`.
///
/// Holds an `Arc<H>` so `Clone` is cheap (tower middleware patterns
/// frequently clone services across tasks).
pub struct HarnessService<H: Harness + 'static> {
    inner: Arc<H>,
}

impl<H: Harness + 'static> HarnessService<H> {
    pub fn new(h: H) -> Self {
        Self { inner: Arc::new(h) }
    }

    pub fn from_arc(h: Arc<H>) -> Self {
        Self { inner: h }
    }
}

impl<H: Harness + 'static> Clone for HarnessService<H> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<H: Harness + 'static> Service<HarnessRequest> for HarnessService<H> {
    type Response = RunResult;
    type Error = HarnessError;
    // Boxed for object-safety with arbitrary `Harness` impls; the
    // futures returned by `Harness::run` are already `Send`.
    type Future = Pin<Box<dyn Future<Output = Result<RunResult, HarnessError>> + Send>>;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        // Subprocess-backed; no readiness to negotiate at this layer.
        // Concurrency / rate / backpressure go in middleware above us.
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: HarnessRequest) -> Self::Future {
        let inner = self.inner.clone();
        Box::pin(async move { inner.run(req).await })
    }
}
