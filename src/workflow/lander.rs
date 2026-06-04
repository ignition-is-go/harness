use super::environment::Environment;
use super::task::Task;
use super::verifier::VerifyOutcome;
use async_trait::async_trait;

/// What `done` looks like for this task class. Runs only after the
/// post-consult verify passes.
///
/// Concrete impls: `git commit && git push && gh pr create`,
/// branch-only push (no PR), local commit only, write the diff to a
/// file, post to a code review tool.
#[async_trait]
pub trait Lander: Send + Sync {
    async fn land(
        &self,
        env: &dyn Environment,
        task: &dyn Task,
        verify: &VerifyOutcome,
    ) -> Result<LandedRef, LanderError>;
}

/// Reference the consumer can record / open / link from the outcome.
#[derive(Debug, Clone)]
pub struct LandedRef {
    /// Commit SHA after landing, if applicable.
    pub commit_sha: Option<String>,
    /// PR / branch / artifact URL, if applicable.
    pub url: Option<String>,
    /// Adapter-specific extras (gh PR number, internal ID, ...).
    pub metadata: serde_json::Value,
}

#[derive(Debug, thiserror::Error)]
pub enum LanderError {
    #[error("io error: {0}")]
    Io(#[source] std::io::Error),

    #[error("subprocess exited {code}: {stderr}")]
    Subprocess { code: i32, stderr: String },

    #[error("remote API error: {0}")]
    Remote(String),
}
