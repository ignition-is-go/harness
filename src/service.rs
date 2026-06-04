//! Tower bridge. Enabled by the `tower` cargo feature.
//!
//! Wraps any `Harness<R>` as a `tower::Service<R>` so consumers can
//! stack tower layers (retry, timeout, concurrency limit, custom
//! middleware) around a harness without us writing each one.
//!
//! The Service impl is generic over the Harness's Request, with
//! Response and Error passed through as associated types — matches the
//! generic trait shape, no impedance.
//!
//! # Example
//! ```no_run
//! # #[cfg(feature = "tower")]
//! # async fn run() -> Result<(), Box<dyn std::error::Error>> {
//! use harness::{HarnessService, cli::{Goose, PromptRequest}};
//! use tower::{Service, ServiceExt};
//! use tower::limit::ConcurrencyLimit;
//!
//! let mut svc = ConcurrencyLimit::new(
//!     HarnessService::new(Goose::new()),
//!     2,
//! );
//! let req = PromptRequest::new("Reply with just READY.").max_turns(1);
//! let res = svc.ready().await?.call(req).await?;
//! println!("messages={}", res.messages);
//! # Ok(()) }
//! ```

use crate::Harness;
use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tower::Service;

/// `tower::Service<R>` adapter for any `Harness<R>`.
///
/// Holds an `Arc<H>` so `Clone` is cheap (tower middleware patterns
/// frequently clone services across tasks).
pub struct HarnessService<H, R> {
    inner: Arc<H>,
    _req: PhantomData<fn(R)>,
}

impl<H, R> HarnessService<H, R>
where
    H: Harness<R> + 'static,
    R: Send + 'static,
{
    pub fn new(h: H) -> Self {
        Self {
            inner: Arc::new(h),
            _req: PhantomData,
        }
    }

    pub fn from_arc(h: Arc<H>) -> Self {
        Self {
            inner: h,
            _req: PhantomData,
        }
    }
}

impl<H, R> Clone for HarnessService<H, R> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            _req: PhantomData,
        }
    }
}

impl<H, R> Service<R> for HarnessService<H, R>
where
    H: Harness<R> + 'static,
    R: Send + 'static,
    H::Response: 'static,
    H::Error: 'static,
{
    type Response = H::Response;
    type Error = H::Error;
    type Future = Pin<Box<dyn Future<Output = Result<H::Response, H::Error>> + Send>>;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        // Subprocess-backed / remote-call-backed; no readiness to
        // negotiate at this layer. Concurrency / rate / backpressure
        // go in middleware above us.
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: R) -> Self::Future {
        let inner = self.inner.clone();
        Box::pin(async move { inner.run(req).await })
    }
}
