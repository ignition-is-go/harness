use super::lander::LandedRef;
use std::collections::BTreeMap;
use std::time::Duration;

/// Free-form metadata namespace (`subject.repo` = `org/name`, etc.).
pub type Attributes = BTreeMap<String, String>;

/// Inbound request the Workflow accepts — uniform across task kinds.
/// The actual task-specific knobs hang off the `Task` impl provided
/// at build time; this carries the per-run overrides.
#[derive(Debug, Clone, Default)]
pub struct WorkflowRequest {
    /// Per-run model override (passed to the consult harness).
    pub model: Option<String>,
    /// Per-run max-turns override.
    pub max_turns: Option<u32>,
    /// Skip the pre-verify pristine-state check (e.g. when the
    /// environment is known dirty already).
    pub skip_preverify: bool,
    /// Extra env passed to the consult harness.
    pub env: Vec<(String, String)>,
}

/// Terminal classification of one Workflow run. The Sink records this
/// verbatim; consumers branch on it for routing (e.g. notify on
/// LandFailed, ignore AlreadyResolved).
#[derive(Debug, Clone)]
pub enum Outcome {
    /// Pre-verify passed before the consult ran — the task was already
    /// resolved by some prior change. No diff, no PR, just a record.
    AlreadyResolved {
        notes: Vec<String>,
    },
    /// Consult ran but produced no diff in the environment.
    NoDiff {
        consult_wall: Duration,
        consult_messages: u32,
        consult_tokens_in: u64,
        consult_tokens_out: u64,
        consult_cost_usd: f64,
        notes: Vec<String>,
    },
    /// Consult produced a diff but post-verify rejected it.
    VerifyFailed {
        consult_wall: Duration,
        consult_messages: u32,
        consult_tokens_in: u64,
        consult_tokens_out: u64,
        consult_cost_usd: f64,
        verify_stderr_tail: Option<String>,
        notes: Vec<String>,
    },
    /// Consult + verify both succeeded; lander then errored.
    LandFailed {
        consult_wall: Duration,
        consult_messages: u32,
        consult_tokens_in: u64,
        consult_tokens_out: u64,
        consult_cost_usd: f64,
        lander_error: String,
        notes: Vec<String>,
    },
    /// Happy path: consult succeeded, verify passed, lander completed.
    Landed {
        consult_wall: Duration,
        consult_messages: u32,
        consult_tokens_in: u64,
        consult_tokens_out: u64,
        consult_cost_usd: f64,
        landed: LandedRef,
        notes: Vec<String>,
    },
    /// The state machine itself errored (workdir prep failed,
    /// consult harness errored, etc.). Carries the message for
    /// surfacing.
    RunnerError {
        stage: &'static str,
        error: String,
        notes: Vec<String>,
    },
}

impl Outcome {
    /// Canonical short status string for the outcome record / logs.
    pub fn status(&self) -> &'static str {
        match self {
            Outcome::AlreadyResolved { .. } => "already-resolved",
            Outcome::NoDiff { .. } => "no-diff",
            Outcome::VerifyFailed { .. } => "verify-failed",
            Outcome::LandFailed { .. } => "land-failed",
            Outcome::Landed { .. } => "landed",
            Outcome::RunnerError { .. } => "runner-error",
        }
    }

    /// Whether this outcome represents a happy-path completion.
    pub fn is_success(&self) -> bool {
        matches!(self, Outcome::Landed { .. } | Outcome::AlreadyResolved { .. })
    }
}
