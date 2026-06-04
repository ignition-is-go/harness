//! Workflow — a state-machine `Harness` that composes other Harnesses.
//!
//! Wraps a stochastic consult (any `Harness<PromptRequest>`) with a
//! deterministic verify-and-land tail. Same shape as the wrapped
//! Harness from the outside (`Harness<WorkflowRequest, Response =
//! WorkflowOutcome>`), so layers / tower bridge / compliance treat it
//! identically.
//!
//! ```text
//!   prepare(Environment)
//!         │
//!         ▼
//!   preverify(Verifier) ── pass ──▶ AlreadyResolved
//!         │ fail
//!         ▼
//!   consult(Harness<PromptRequest>)
//!         │
//!         ▼
//!   diff_present(Environment) ── no ──▶ NoDiff
//!         │ yes
//!         ▼
//!   verify(Verifier) ── fail ──▶ VerifyFailed
//!         │ pass
//!         ▼
//!   land(Lander) ── err ──▶ LandFailed
//!         │ ok
//!         ▼
//!     Landed
//! ```
//!
//! Each role is a swappable trait:
//!
//!   | Role          | Trait        | Pluggable for                |
//!   |---            |---           |---                           |
//!   | Task          | `Task`       | GitHub issue, Linear, CVE, …|
//!   | Environment   | `Environment`| fresh clone, worktree, FS, … |
//!   | Verifier      | `Verifier`   | shell, axe, eval rubric, …   |
//!   | Lander        | `Lander`     | PR, branch, disk, …          |
//!   | Sink          | `Sink`       | pulse-ctx, log, stdout, …    |
//!   | Consult       | `Harness<R>` | Goose, ClaudeCode, HTTP, …   |
//!
//! Construct with `Workflow::builder()` (see `builder` module).

mod builder;
mod environment;
mod lander;
mod outcome;
mod sink;
mod task;
mod verifier;
mod workflow_impl;

pub use builder::WorkflowBuilder;
pub use environment::{Environment, EnvironmentError};
pub use lander::{LandedRef, Lander, LanderError};
pub use outcome::{Attributes, Outcome, WorkflowRequest};
pub use sink::{RecordId, Sink, SinkError};
pub use task::Task;
pub use verifier::{VerifyOutcome, Verifier, VerifierError};
pub use workflow_impl::{Workflow, WorkflowError};
