use async_trait::async_trait;
use std::path::Path;

/// Where the consult step does its work, and the source-of-truth for
/// whether it changed anything.
#[async_trait]
pub trait Environment: Send + Sync {
    /// Working directory the consult agent operates in.
    fn workdir(&self) -> &Path;

    /// Initialize the environment. For git impls: clone the repo,
    /// check out the base branch, branch off, write any local-only
    /// ignore patterns. May spawn subprocesses; should be idempotent.
    async fn prepare(&mut self) -> Result<(), EnvironmentError>;

    /// Whether the consult step actually changed anything we should
    /// verify and land. For git impls: `git add -A && git diff
    /// --cached --stat <base>` non-empty.
    async fn diff_present(&self) -> Result<bool, EnvironmentError>;
}

#[derive(Debug, thiserror::Error)]
pub enum EnvironmentError {
    #[error("prepare failed: {0}")]
    Prepare(String),

    #[error("io error: {0}")]
    Io(#[source] std::io::Error),

    #[error("subprocess exited {code}: {stderr}")]
    Subprocess { code: i32, stderr: String },
}
