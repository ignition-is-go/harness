//! GitHub Task + Lander implementations for `harness-workflow`.
//!
//! - `GithubIssueTask`: pulls a GitHub issue via `gh issue view`, exposes
//!   it through the `Task` trait. Optional ui-watchdog axe-context
//!   extraction surfaces `(rule_id, flow_step)` as attributes for
//!   downstream Verifiers.
//! - `GhPrLander`: after Workflow's verify passes, commits any uncommitted
//!   changes, pushes the branch, and opens a PR via `gh pr create`.
//!
//! Requires `gh` and `git` on PATH at runtime. No special permissions
//! beyond what `gh` is already authenticated for.

mod issue;
mod lander;

pub use issue::{GithubIssueTask, IssueError};
pub use lander::{GhPrLander, LanderConfig};
