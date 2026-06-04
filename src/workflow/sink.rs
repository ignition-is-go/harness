use super::outcome::Outcome;
use async_trait::async_trait;

/// Where the workflow's outcome record lands. Called once per
/// Workflow run, ALWAYS — even on failure paths.
///
/// Concrete impls: pulse-ctx record write, stdout JSON, log file,
/// multi-sink fan-out.
#[async_trait]
pub trait Sink: Send + Sync {
    async fn record(&self, outcome: &Outcome) -> Result<RecordId, SinkError>;
}

/// Identifier the Sink returned for the recorded outcome
/// (pulse-ctx record UUID, log offset, etc.).
pub type RecordId = String;

#[derive(Debug, thiserror::Error)]
pub enum SinkError {
    #[error("transport error: {0}")]
    Transport(String),

    #[error("encode error: {0}")]
    Encode(String),

    #[error("remote rejected: {0}")]
    Rejected(String),
}
