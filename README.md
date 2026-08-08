# harness (deprecated)

> This repository is retired as a general-purpose agent harness. Pulse's
> bounded issue workflow now lives in `pulse-deploy` under the
> `pulse-workflow-*` crates. Durable interactive agent sessions belong in
> `pulse-agents` and use Agent Client Protocol (ACP), not this trait.

No new adapters or dependents should be added here. Existing consumers should
migrate to the `pulse-workflow-*` crates or to `pulse-agents`' ACP runtime.

Bounded primitive for stochastic / expensive async calls
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
types. This is retained only for the legacy bounded workflow consumer.

## Workspace

This repo is a cargo workspace. Each crate is independently usable —
depend on what you need, skip what you don't.

| Crate | Purpose | Depends on |
| --- | --- | --- |
| [`harness`](crates/harness) | The trait, `Capabilities`. Tiny, no I/O. | (nothing in-tree) |
| [`harness-cli`](crates/harness-cli) | Goose + ClaudeCode adapters. `Harness<PromptRequest, Response = RunResult>`. | `harness` |
| [`harness-workflow`](crates/harness-workflow) | State-machine `Workflow<T, E, V, L, S, H>` + `Task`/`Environment`/`Verifier`/`Lander`/`Sink` traits + `ShellVerifier`. | `harness` + `harness-cli` |
| [`harness-tower`](crates/harness-tower) | `tower::Service<R>` bridge for any `Harness<R>`. | `harness` |
| [`harness-github`](crates/harness-github) | `GithubIssueTask` (wraps `gh issue view`, parses ui-watchdog axe context) + `GhPrLander` (commit / push / `gh pr create`). | `harness-workflow` |
| [`harness-git`](crates/harness-git) | `GitCloneEnv` — clones to tempdir, branches off, manages `.git/info/exclude` for build artifacts. | `harness-workflow` |
| [`harness-pcx`](crates/harness-pcx) | `PcxSink` — writes `Outcome` records to a pulse-ctx instance via JSON-RPC. | `harness-workflow` |

Same pattern as Tower (`tower-service` / `tower-layer` / `tower` /
`tower-http`): tiny focused crates that compose, no monolithic
umbrella unless consumers want one.

Future families slot in the same way: `harness-http` (direct API
clients), `harness-axe` (Playwright + axe-core in-Rust, replacing the
shell-script flavor), `harness-linear` (Linear ticket Task), etc.

## Use — agentic CLI

```rust
use harness::Harness;
use harness_cli::{Goose, PromptRequest};
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

`Workflow` wraps a stochastic consult (any `Harness<PromptRequest>`)
with a deterministic verify-then-land tail. Itself a
`Harness<WorkflowRequest>` — composable, stackable, testable identically.

```rust
use std::sync::Arc;
use std::time::Duration;
use harness::Harness;
use harness_cli::Goose;
use harness_git::GitCloneEnv;
use harness_github::{GhPrLander, GithubIssueTask};
use harness_pcx::PcxSink;
use harness_workflow::{ShellVerifier, WorkflowBuilder};

let task = Arc::new(GithubIssueTask::fetch(url).await?);
let workflow = WorkflowBuilder::new()
    .task((*task).clone_via_arc())                          // see GithubIssueTask::fetch
    .environment(
        GitCloneEnv::new(repo_ssh, "main", branch_name)
            .exclude("target/")
            .exclude("dist/")
            .commit_author("harness", "harness@example.com"),
    )
    .verifier(ShellVerifier::new("verify-axe-pulse-ctx")
        .timeout(Duration::from_secs(900)))
    .lander(GhPrLander::new(repo, "main", branch_name))
    .sink(PcxSink::new(endpoint, task)?.actor_model("ollama/gpt-oss-20b"))
    .consult(Goose::new())
    .prompt(|task, _env, _prev| format!("{}\n\n{}", task.objective(), task.body()));

let outcome = workflow.run(Default::default()).await?;
```

State machine: `prepare → preverify → consult → diff-check → verify →
land → record`. Every terminal outcome is recorded — ground truth, not
the LLM's self-report. Outcomes: `AlreadyResolved | NoDiff |
VerifyFailed | LandFailed | Landed | RunnerError`.

## Capabilities

Per-adapter matrix (declared via `Capabilities::is_consistent()` —
adapters claiming `reports_tokens` / `reports_cost` must also claim
`supports_json_output`, enforced by the compliance test suite):

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
adapter surfaces `0`.

² goose does not expose a cost figure; compute from token counts and
your model's pricing if you need it.

## Tower integration

`harness-tower::HarnessService<H, R>` wraps any `Harness<R>` as a
`tower::Service<R>`, so consumers stack middleware instead of
hand-rolling policy:

```rust
use harness_cli::{Goose, PromptRequest};
use harness_tower::HarnessService;
use tower::limit::ConcurrencyLimit;
use tower::{Service, ServiceExt};

let mut svc = ConcurrencyLimit::new(HarnessService::new(Goose::new()), 4);
let req = PromptRequest::new("...").max_turns(1);
let res = svc.ready().await?.call(req).await?;
```

From that one impl the tower ecosystem (timeout, retry, rate limit,
buffer, balance, custom layers) drops in on top of any Harness without
further adapter work.

## Build, test, smoke

```sh
cargo build --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace                        # unit + compliance
HARNESS_TEST_REAL_CLI=1 cargo test --workspace # also exercise real CLIs
cargo run -p harness-cli --example smoke
cargo run -p harness-tower --example with-tower
```

The compliance suite in `crates/harness-cli/tests/compliance.rs`
asserts trait conformance across CLI-family adapters — non-empty
unique names, consistent capabilities, known-good per-adapter
expectations. The real-CLI lane is gated on `HARNESS_TEST_REAL_CLI=1`
and tolerates missing binaries.

## Adding a new Harness family

Different request/response shape, same trait. Create a new crate
`crates/harness-<family>/`, declare the request/response types,
implement `Harness<TheirRequest, Response = TheirResponse, Error =
TheirError>`. The tower bridge (`HarnessService<H, R>`) works without
modification — it's generic over R.
