use std::path::PathBuf;
use std::time::Duration;

/// One prompt-to-completion job for any `Harness` implementation.
///
/// Built with the fluent setters (`workdir`, `max_turns`, `model`, `env`,
/// `timeout`) so a caller can declare exactly the knobs it cares about
/// without filling in defaults it doesn't.
#[derive(Debug, Clone)]
pub struct HarnessRequest {
    pub prompt: String,
    pub workdir: Option<PathBuf>,
    pub max_turns: Option<u32>,
    pub model: Option<String>,
    pub env: Vec<(String, String)>,
    pub timeout: Option<Duration>,
}

impl HarnessRequest {
    pub fn new(prompt: impl Into<String>) -> Self {
        Self {
            prompt: prompt.into(),
            workdir: None,
            max_turns: None,
            model: None,
            env: Vec::new(),
            timeout: None,
        }
    }

    pub fn workdir(mut self, p: impl Into<PathBuf>) -> Self {
        self.workdir = Some(p.into());
        self
    }

    pub fn max_turns(mut self, n: u32) -> Self {
        self.max_turns = Some(n);
        self
    }

    pub fn model(mut self, m: impl Into<String>) -> Self {
        self.model = Some(m.into());
        self
    }

    pub fn env(mut self, k: impl Into<String>, v: impl Into<String>) -> Self {
        self.env.push((k.into(), v.into()));
        self
    }

    pub fn timeout(mut self, d: Duration) -> Self {
        self.timeout = Some(d);
        self
    }
}

/// Normalized result of one run. Fields the underlying CLI doesn't expose
/// are left at zero / empty rather than guessed.
#[derive(Debug, Clone, Default)]
pub struct RunResult {
    /// Raw stdout the CLI produced. Preserved verbatim for debugging /
    /// downstream parsing; adapter-specific structured fields below are
    /// best-effort extracts.
    pub stdout: String,
    /// Raw stderr (often where the CLI logs progress / errors).
    pub stderr: String,
    /// Process exit code. 0 = clean exit; non-zero usually means the CLI
    /// aborted (turn cap hit, budget exhausted, internal error).
    pub exit_code: i32,
    /// Best-effort count of agent turns / messages exchanged. 0 if the
    /// CLI's output didn't carry the metric.
    pub messages: u32,
    /// Best-effort token accounting. 0 if not reported.
    pub tokens_in: u64,
    pub tokens_out: u64,
    /// Wall time the subprocess ran end-to-end.
    pub wall: Duration,
}

/// Failure modes a `Harness::run` can report. `NonZeroExit` carries the
/// stdout/stderr the CLI emitted so consumers can decide whether to retry
/// or surface to a human.
#[derive(Debug, thiserror::Error)]
pub enum HarnessError {
    #[error("failed to spawn subprocess: {0}")]
    Spawn(#[source] std::io::Error),

    #[error("subprocess i/o error: {0}")]
    Io(#[source] std::io::Error),

    #[error("run exceeded timeout of {0:?}")]
    Timeout(Duration),

    #[error("failed to parse harness output: {0}")]
    Parse(String),

    #[error("harness exited with code {code}: {stderr}")]
    NonZeroExit {
        code: i32,
        stdout: String,
        stderr: String,
    },
}
