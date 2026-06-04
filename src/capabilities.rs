/// What a `Harness` impl can and can't do.
///
/// Returned by `Harness::capabilities()` without invoking the underlying
/// call so consumers can route requests by capability (e.g. "I need a
/// harness that reports cost"; "this harness can't honor max_turns,
/// treat my cap as advisory") without empirical probing.
///
/// Fields are intentionally conservative — declare `false` unless the
/// impl has a concrete code path that uses the feature on every run.
///
/// Several fields are agentic-CLI-shaped (max_turns, model, json output,
/// tokens, cost). For Harness impls that don't fit that shape (e.g. a
/// workflow that composes other Harnesses, a RAG lookup), declare them
/// `false` and rely on the impl's structured response carrying whatever
/// metadata is meaningful.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capabilities {
    /// Request's `max_turns` is passed through as a CLI flag (or
    /// equivalent bound on the underlying call).
    pub supports_max_turns: bool,
    /// Request's `model` is passed through.
    pub supports_model_override: bool,
    /// The underlying call is invoked with a JSON output mode the impl
    /// parses. Implies the impl has a real chance of populating
    /// structured response fields beyond raw stdout.
    pub supports_json_output: bool,
    /// Response carries input/output token counts.
    pub reports_tokens: bool,
    /// Response carries a cost figure.
    pub reports_cost: bool,
    /// Request's `workdir` is honored (subprocess `cwd` is set, or
    /// equivalent context).
    pub supports_workdir: bool,
}

impl Capabilities {
    /// Internal invariant check used by the compliance test suite.
    /// Impls MUST NOT claim a structured-field capability
    /// (`reports_tokens`, `reports_cost`) without also claiming
    /// `supports_json_output`, since parsing those fields out of
    /// free-form stdout is not robust enough to promise.
    pub fn is_consistent(&self) -> bool {
        if self.reports_tokens && !self.supports_json_output {
            return false;
        }
        if self.reports_cost && !self.supports_json_output {
            return false;
        }
        true
    }

    /// All-false: the default for impls that don't fit the agentic-CLI
    /// capability shape (workflow composers, etc.). Use this as a base
    /// and toggle on whatever the impl actually does.
    pub const fn none() -> Self {
        Self {
            supports_max_turns: false,
            supports_model_override: false,
            supports_json_output: false,
            reports_tokens: false,
            reports_cost: false,
            supports_workdir: false,
        }
    }
}
