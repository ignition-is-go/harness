use super::environment::Environment;
use super::task::Task;
use async_trait::async_trait;

/// The pass/fail gate that decides whether the consult's diff is
/// acceptable. Run twice per Workflow run: once before the consult
/// (pristine state → `AlreadyResolved` short-circuit if the diff was
/// applied in some prior change), once after.
///
/// Concrete impls: shell command (exit 0 = pass), structured probe
/// (axe-core + a11y-tree dump for accessibility rewrites), eval
/// rubric, test suite.
#[async_trait]
pub trait Verifier: Send + Sync {
    async fn verify(
        &self,
        env: &dyn Environment,
        task: &dyn Task,
    ) -> Result<VerifyOutcome, VerifierError>;
}

/// What a verify pass / fail produced — structured enough for the
/// next consult round to read; opaque enough to pass through the
/// state machine without inspection.
#[derive(Debug, Default)]
pub struct VerifyOutcome {
    pub passed: bool,
    /// Last N bytes of stderr or other failure tail — surfaced into
    /// the outcome record on failure.
    pub stderr_tail: Option<String>,
    /// Structured failure detail (e.g. axe violation set) for richer
    /// recording / next-round prompting.
    pub structured: Option<serde_json::Value>,
}

#[derive(Debug, thiserror::Error)]
pub enum VerifierError {
    #[error("io error: {0}")]
    Io(#[source] std::io::Error),

    #[error("verify timed out")]
    Timeout,

    #[error("verifier internal failure: {0}")]
    Internal(String),
}
