# harness

Tower-style generic primitive for stochastic / expensive async calls
(LLM consults, RAG lookups, anything where you hand a request to a
remote thing and get an answer back).

```rust
#[async_trait]
pub trait Harness<Request>: Send + Sync {
    type Response;
    type Error;
    fn name(&self) -> &str;
    fn capabilities(&self) -> Capabilities;
    async fn run(&self, req: Request) -> Result<Self::Response, Self::Error>;
}
```

Generic over the request type, with response and error as associated
types — same shape as `tower::Service`. One trait family covers many
adapter families.

## Adapter families

| Module | Trait instantiation | Concrete impls |
| --- | --- | --- |
| [`harness::cli`](src/cli/) | `Harness<PromptRequest, Response = RunResult, Error = CliError>` | `Goose`, `ClaudeCode` |
| [`harness::workflow`](src/workflow/) | `Harness<WorkflowRequest, Response = Outcome, Error = WorkflowError>` | `Workflow<T, E, V, L, S, H>` — composes other harnesses |

Future families slot in the same way without changing the trait:
HTTP-direct API clients, local model runners, RAG retrievers.

## Use — agentic CLI

```rust
use harness::{Harness, cli::{Goose, PromptRequest}};
use std::time::Duration;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let h = Goose::new();
    let req = PromptRequest::new("Summarize README.md in two sentences.")
        .workdir(".")
        .max_turns(3)
        .timeout(Duration::from_secs(60));
    let res = h.run(req).await?;
    println!("messages={} tokens={}→{} cost=${:.4} wall={:?}",
        res.messages, res.tokens_in, res.tokens_out, res.cost_usd, res.wall);
    Ok(())
}
```

## Use — Workflow (compose other harnesses)

`Workflow` wraps a stochastic consult (any CLI-family `Harness`) with a
deterministic verify-then-land tail. Itself a `Harness<WorkflowRequest>` —
composable, stackable, testable identically to any other Harness.

```rust
use harness::workflow::WorkflowBuilder;

let workflow = WorkflowBuilder::new()
    .task(GithubIssueTask::fetch(url).await?)
    .environment(GitCloneEnv::new(repo, base))
    .verifier(AxeVerifier::new())
    .lander(GhPrLander::new(repo))
    .sink(PcxSink::new(endpoint))
    .consult(Goose::new())
    .prompt(|task, env, _prev| build_prompt(task, env));

let outcome = workflow.run(Default::default()).await?;
match outcome {
    Outcome::Landed { .. } => { /* PR opened */ }
    Outcome::AlreadyResolved { .. } => { /* pre-verify passed */ }
    Outcome::NoDiff { .. } | Outcome::VerifyFailed { .. } => { /* consult didn't produce a valid fix */ }
    _ => { /* other terminal */ }
}
```

The state machine:

```
prepare(Environment)
   → preverify(Verifier) ─ pass ─▶ AlreadyResolved
   ↓ fail
   consult(Harness<PromptRequest>)
   ↓
   diff_present(Environment) ─ no ─▶ NoDiff
   ↓ yes
   verify(Verifier) ─ fail ─▶ VerifyFailed
   ↓ pass
   land(Lander) ─ err ─▶ LandFailed
   ↓ ok
   Landed
```

Every terminal outcome is recorded through the `Sink` — even failures.
The runner's view of what happened is ground truth, not the LLM's
self-report.

## Capabilities

```rust
use harness::{cli::ClaudeCode, Harness};
let c = ClaudeCode::new().capabilities();
if c.reports_cost { /* route cost-sensitive jobs here */ }
```

Per-CLI-adapter matrix:

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

`Workflow` declares `Capabilities::none()` — its capability shape is
task-in / outcome-out, observable via the wrapped consult Harness.

## Tower integration (`--features tower`)

`HarnessService<H, R>` wraps any `Harness<R>` as a `tower::Service<R>`,
so consumers stack middleware instead of hand-rolling policy:

```rust
use harness::{HarnessService, cli::{Goose, PromptRequest}};
use tower::limit::ConcurrencyLimit;
use tower::{Service, ServiceExt};

let mut svc = ConcurrencyLimit::new(HarnessService::new(Goose::new()), 4);
let req = PromptRequest::new("...").max_turns(1);
let res = svc.ready().await?.call(req).await?;
```

From that one impl the tower ecosystem (timeout, retry, rate limit,
buffer, balance, custom layers) drops in on top of any Harness without
further adapter work.

## Smoke

```sh
cargo run --example smoke                       # CLI adapters
cargo run --features tower --example with-tower # + tower middleware
```

## Tests

```sh
cargo test                            # unit + compliance
HARNESS_TEST_REAL_CLI=1 cargo test    # also exercise real CLIs
```

The compliance suite asserts trait conformance across CLI-family
adapters — non-empty unique names, consistent capabilities, known-good
per-adapter expectations. Real-CLI lane is gated; tolerates missing
binaries and skips rather than fails.

## Adding a CLI-family adapter

1. `src/cli/<name>.rs` with a struct + `impl Harness<PromptRequest>`.
2. Export from `src/cli/mod.rs`.
3. Implement `name()`, `capabilities()`, `run()`.
4. Add a `which` branch in `examples/smoke.rs`.
5. Add the struct to `all_adapters()` in `tests/compliance.rs` and any
   adapter-specific capability test.
6. Add a row to the capability matrix above.

## Adding a new Harness family

Different request/response shape, same trait. Pick a module (e.g.
`harness::http`, `harness::rag`), declare the request/response types,
implement `Harness<TheirRequest, Response = TheirResponse, Error = TheirError>`,
ship.

The tower bridge (`HarnessService<H, R>`) is generic enough to work with
any family without modification.
