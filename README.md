# harness

Normalized subprocess wrappers around agentic CLI tools.

`harness` gives one Rust trait — `Harness::run(HarnessRequest) -> RunResult`
— with concrete implementations for each agentic CLI:

| Adapter | Wraps |
| --- | --- |
| `Goose` | `goose run -i FILE --output-format json --no-session` |
| `ClaudeCode` | `claude -p PROMPT --output-format json` |

No layers, no policy, no I/O channels beyond subprocess. Consumers compose
retries / cost caps / persistence / coordination themselves; the
optional `tower` feature provides a `tower::Service` impl so the entire
[tower](https://docs.rs/tower) middleware ecosystem (timeout, retry,
concurrency limit, rate limit, custom layers) drops in on top without
us writing each one.

## Use

```rust
use harness::{Goose, Harness, HarnessRequest};
use std::time::Duration;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let h = Goose::new();
    let req = HarnessRequest::new("Summarize README.md in two sentences.")
        .workdir(".")
        .max_turns(3)
        .timeout(Duration::from_secs(60));
    let res = h.run(req).await?;
    println!("messages={} tokens={}→{} cost=${:.4} wall={:?}",
        res.messages, res.tokens_in, res.tokens_out, res.cost_usd, res.wall);
    Ok(())
}
```

## Capabilities

Different harnesses honor different `HarnessRequest` fields and populate
different `RunResult` fields. Inspect what an adapter supports without
spawning the subprocess:

```rust
use harness::{ClaudeCode, Goose, Harness};
let c = ClaudeCode::new().capabilities();
if c.reports_cost { /* route cost-sensitive jobs here */ }
```

Per-adapter matrix:

| Capability                  | `Goose`    | `ClaudeCode` |
| ---                         | ---        | ---          |
| `supports_max_turns`        | yes        | yes          |
| `supports_model_override`   | yes        | yes          |
| `supports_json_output`      | yes        | yes          |
| `reports_tokens`            | best-effort¹ | yes        |
| `reports_cost`              | no²        | yes          |
| `supports_workdir`          | yes        | yes          |

¹ goose's JSON output carries token totals when the configured provider
emits them; some provider configurations leave the field absent and the
adapter then surfaces `0`.

² goose does not expose a cost figure; compute from token counts and your
model's pricing if you need it.

Adapters MUST keep `Capabilities::is_consistent()` true — e.g. claiming
`reports_tokens` without `supports_json_output` is rejected by the
compliance test suite (see below).

## Tower integration (`--features tower`)

`HarnessService<H>` wraps any `Harness` as a `tower::Service<HarnessRequest>`,
so consumers stack middleware instead of hand-rolling policy:

```rust
use harness::{Goose, HarnessRequest, HarnessService};
use tower::limit::ConcurrencyLimit;
use tower::{Service, ServiceExt};

let mut svc = ConcurrencyLimit::new(HarnessService::new(Goose::new()), 4);
let req = HarnessRequest::new("...").max_turns(1);
let res = svc.ready().await?.call(req).await?;
```

From that one impl the tower ecosystem becomes available without further
adapter work — `tower::timeout::Timeout`, `tower::retry::Retry`,
`tower::limit::RateLimit`, `tower::buffer::Buffer`, `tower::balance` for
fanning across N harness instances, custom `Layer`s for cost caps /
context-record emission / handshake injection.

See `examples/with-tower.rs`:

```sh
cargo run --features tower --example with-tower
```

## Smoke

```sh
cargo run --example smoke
```

Runs the same trivial prompt through every adapter whose binary is on
`PATH`, prints capabilities + normalized fields side-by-side.

## Tests

```sh
cargo test                                            # unit + compliance
HARNESS_TEST_REAL_CLI=1 cargo test                    # also exercise real CLIs
```

The compliance suite (`tests/compliance.rs`) runs the same assertions
against every adapter — non-empty unique names, consistent capabilities,
each adapter's known-good capability values. The real-CLI lane is gated;
it tolerates missing binaries and skips rather than fails.

## Adding an adapter

1. New `src/adapter/<name>.rs` with a struct + `impl Harness`.
2. Export from `src/adapter/mod.rs`.
3. Implement `name()`, `capabilities()`, `run()`.
4. Add a `which` branch in `examples/smoke.rs`.
5. Add the struct to `all_adapters()` in `tests/compliance.rs` and any
   adapter-specific capability test.
6. Add a row to the matrix above.

The adapter is responsible for: argv construction, subprocess spawn,
output parsing into `RunResult`'s normalized fields, and exit-code →
`HarnessError` mapping. Best-effort parsing — leave fields zero where the
CLI doesn't report them rather than guessing, and reflect that in
`Capabilities`.
