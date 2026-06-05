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

/// Metrics + raw trace the workflow captured from the consult harness.
/// Bundled so every Outcome variant that has post-consult information
/// can carry it without duplicating fields. `stdout` is the raw harness
/// output (typically goose's `--output-format json`) which consumers
/// can parse for per-turn details to populate UI transcripts.
#[derive(Debug, Clone, Default)]
pub struct ConsultTrace {
    pub wall: Duration,
    pub messages: u32,
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub cost_usd: f64,
    pub stdout: String,
    pub stderr: String,
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
        consult: ConsultTrace,
        notes: Vec<String>,
    },
    /// Consult produced a diff but post-verify rejected it.
    VerifyFailed {
        consult: ConsultTrace,
        verify_stderr_tail: Option<String>,
        notes: Vec<String>,
    },
    /// Consult + verify both succeeded; lander then errored.
    LandFailed {
        consult: ConsultTrace,
        lander_error: String,
        notes: Vec<String>,
    },
    /// Happy path: consult succeeded, verify passed, lander completed.
    Landed {
        consult: ConsultTrace,
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

    /// Borrow the consult trace if the outcome has one. `AlreadyResolved`
    /// + `RunnerError` (before consult) have no trace.
    pub fn consult(&self) -> Option<&ConsultTrace> {
        match self {
            Outcome::NoDiff { consult, .. }
            | Outcome::VerifyFailed { consult, .. }
            | Outcome::LandFailed { consult, .. }
            | Outcome::Landed { consult, .. } => Some(consult),
            _ => None,
        }
    }
}
