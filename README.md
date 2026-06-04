# harness

Normalized subprocess wrappers around agentic CLI tools.

`harness` gives one Rust trait — `Harness::run(HarnessRequest) -> RunResult`
— with concrete implementations for each agentic CLI:

| Adapter | Wraps |
| --- | --- |
| `Goose` | `goose run -i FILE --output-format json --no-session` |
| `ClaudeCode` | `claude -p PROMPT --output-format json` |

No layers, no policy, no I/O channels beyond subprocess. Consumers compose
retries / cost caps / persistence / coordination themselves; a
`tower::Service` impl on top of `Harness` is trivial and deliberately not
shipped here.

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
    println!("messages={} tokens={}→{} wall={:?}",
        res.messages, res.tokens_in, res.tokens_out, res.wall);
    Ok(())
}
```

## Smoke

```sh
cargo run --example smoke
```

Runs the same trivial prompt through every adapter whose binary is on
`PATH`, prints the normalized fields side-by-side.

## Adding an adapter

1. New `src/adapter/<name>.rs` with a struct + `impl Harness`.
2. Export from `src/adapter/mod.rs`.
3. Add a `which` branch in `examples/smoke.rs`.

The adapter is responsible for: argv construction, subprocess spawn,
output parsing into `RunResult`'s normalized fields, and exit-code →
`HarnessError` mapping. Best-effort parsing — leave fields zero where the
CLI doesn't report them rather than guessing.
