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
/// are left at zero / empty rather than guessed. See `Capabilities` on
/// each adapter for which fields it actually populates.
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
    /// Estimated cost in USD if the CLI reports it. 0 if not reported.
    pub cost_usd: f64,
    /// Wall time the subprocess ran end-to-end.
    pub wall: Duration,
}

/// What an adapter can and can't do.
///
/// Returned by `Harness::capabilities()` without spawning the subprocess
/// so consumers can route requests by capability (e.g. "I need a harness
/// that reports cost"; "this harness can't honor max_turns, treat my cap
/// as advisory") without empirical probing.
///
/// Fields are intentionally conservative — declare `false` unless the
/// adapter has a concrete code path that uses the feature on every run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capabilities {
    /// `HarnessRequest::max_turns` is passed through as a CLI flag.
    pub supports_max_turns: bool,
    /// `HarnessRequest::model` is passed through as a CLI flag.
    pub supports_model_override: bool,
    /// The CLI is invoked with a JSON output mode the adapter parses.
    /// Implies the adapter has a real chance of populating structured
    /// `RunResult` fields beyond `stdout`/`exit_code`.
    pub supports_json_output: bool,
    /// The CLI exposes input/output token counts the adapter parses into
    /// `RunResult::tokens_in` and `tokens_out`.
    pub reports_tokens: bool,
    /// The CLI exposes a cost figure the adapter parses into
    /// `RunResult::cost_usd`.
    pub reports_cost: bool,
    /// `HarnessRequest::workdir` is honored (subprocess `cwd` is set).
    pub supports_workdir: bool,
}

impl Capabilities {
    /// Internal invariant check used by the compliance test suite. Adapters
    /// MUST NOT claim a structured-field capability (reports_tokens,
    /// reports_cost) without also claiming `supports_json_output`, since
    /// parsing those fields out of free-form stdout is not robust enough
    /// to promise.
    pub fn is_consistent(&self) -> bool {
        if self.reports_tokens && !self.supports_json_output {
            return false;
        }
        if self.reports_cost && !self.supports_json_output {
            return false;
        }
        true
    }
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
