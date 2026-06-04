//! Stack a tower layer around a Harness.
//!
//! Demonstrates the value of the `tower` feature: one cap (concurrency=1
//! here for visibility; bump for parallelism) wraps the harness without
//! us writing any middleware ourselves.
//!
//! Run with:
//!     cargo run --features tower --example with-tower
//!
//! Skips if `goose` is missing from PATH.

use harness::{Goose, HarnessRequest, HarnessService};
use std::time::Duration;
use tower::limit::ConcurrencyLimit;
use tower::{Service, ServiceExt};

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    if !which("goose") {
        eprintln!("[skip] goose not on PATH");
        return Ok(());
    }

    // Build the service stack. Each layer composes — add RetryLayer,
    // TimeoutLayer, custom middleware, etc. the same way.
    let svc = ConcurrencyLimit::new(HarnessService::new(Goose::new()), 1);

    // Drive three concurrent requests through the cap. With limit=1 they
    // serialize; bump the limit and they overlap.
    let mut handles = Vec::new();
    for i in 0..3 {
        let mut svc = svc.clone();
        let req = HarnessRequest::new(format!("Reply with the number {i}."))
            .max_turns(1)
            .timeout(Duration::from_secs(60));
        handles.push(tokio::spawn(async move {
            let ready = svc.ready().await?;
            ready.call(req).await
        }));
    }

    for (i, h) in handles.into_iter().enumerate() {
        match h.await? {
            Ok(r) => println!(
                "[{i}] ok messages={} tokens={}→{} wall={:?}",
                r.messages, r.tokens_in, r.tokens_out, r.wall
            ),
            Err(e) => eprintln!("[{i}] err: {e}"),
        }
    }

    Ok(())
}

fn which(bin: &str) -> bool {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d.join(bin).is_file()))
        .unwrap_or(false)
}
