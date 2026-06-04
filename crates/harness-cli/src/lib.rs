//! Subprocess-wrapped agentic CLI tools — Goose, Claude Code, and
//! whatever else lands in the same shape.
//!
//! All impls here are `Harness<PromptRequest, Response = RunResult,
//! Error = CliError>` — one concrete request/response/error family.
//! Other Harness families (workflow orchestration, HTTP-direct API
//! clients, RAG retrievers) declare their own types and impl
//! `Harness<TheirRequest>` against the same trait.

mod claude_code;
mod goose;
mod prompt;

pub use claude_code::ClaudeCode;
pub use goose::Goose;
pub use prompt::{CliError, PromptRequest, RunResult};
