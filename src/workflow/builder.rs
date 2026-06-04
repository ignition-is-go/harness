use super::environment::Environment;
use super::lander::Lander;
use super::sink::Sink;
use super::task::Task;
use super::verifier::{VerifyOutcome, Verifier};
use super::workflow_impl::Workflow;
use crate::cli::{CliError, PromptRequest, RunResult};
use crate::Harness;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Type-safe staged builder for [`Workflow`].
///
/// Each setter advances the type, so leaving a slot unfilled is a
/// compile error. The prompt-builder is the last required slot:
///
/// ```ignore
/// let workflow = Workflow::builder()
///     .task(GithubIssue::fetch(url)?)
///     .environment(GitCloneEnv::new(repo, branch))
///     .verifier(VerifyAxe::new())
///     .lander(GhPrLander::new(repo))
///     .sink(PcxSink::new(endpoint))
///     .consult(Goose::new())
///     .prompt(|task, env, prev| build_prompt(task, env, prev))
///     .build();
/// ```
pub struct WorkflowBuilder<T = (), E = (), V = (), L = (), S = (), H = ()> {
    pub(super) task: T,
    pub(super) env: E,
    pub(super) verifier: V,
    pub(super) lander: L,
    pub(super) sink: S,
    pub(super) consult: H,
}

impl Default for WorkflowBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkflowBuilder {
    pub fn new() -> Self {
        Self {
            task: (),
            env: (),
            verifier: (),
            lander: (),
            sink: (),
            consult: (),
        }
    }
}

impl<E, V, L, S, H> WorkflowBuilder<(), E, V, L, S, H> {
    pub fn task<T: Task>(self, t: T) -> WorkflowBuilder<T, E, V, L, S, H> {
        WorkflowBuilder {
            task: t,
            env: self.env,
            verifier: self.verifier,
            lander: self.lander,
            sink: self.sink,
            consult: self.consult,
        }
    }
}

impl<T, V, L, S, H> WorkflowBuilder<T, (), V, L, S, H> {
    pub fn environment<E: Environment>(self, e: E) -> WorkflowBuilder<T, E, V, L, S, H> {
        WorkflowBuilder {
            task: self.task,
            env: e,
            verifier: self.verifier,
            lander: self.lander,
            sink: self.sink,
            consult: self.consult,
        }
    }
}

impl<T, E, L, S, H> WorkflowBuilder<T, E, (), L, S, H> {
    pub fn verifier<V: Verifier>(self, v: V) -> WorkflowBuilder<T, E, V, L, S, H> {
        WorkflowBuilder {
            task: self.task,
            env: self.env,
            verifier: v,
            lander: self.lander,
            sink: self.sink,
            consult: self.consult,
        }
    }
}

impl<T, E, V, S, H> WorkflowBuilder<T, E, V, (), S, H> {
    pub fn lander<L: Lander>(self, l: L) -> WorkflowBuilder<T, E, V, L, S, H> {
        WorkflowBuilder {
            task: self.task,
            env: self.env,
            verifier: self.verifier,
            lander: l,
            sink: self.sink,
            consult: self.consult,
        }
    }
}

impl<T, E, V, L, H> WorkflowBuilder<T, E, V, L, (), H> {
    pub fn sink<S: Sink>(self, s: S) -> WorkflowBuilder<T, E, V, L, S, H> {
        WorkflowBuilder {
            task: self.task,
            env: self.env,
            verifier: self.verifier,
            lander: self.lander,
            sink: s,
            consult: self.consult,
        }
    }
}

impl<T, E, V, L, S> WorkflowBuilder<T, E, V, L, S, ()> {
    pub fn consult<H>(self, c: H) -> WorkflowBuilder<T, E, V, L, S, H>
    where
        H: Harness<PromptRequest, Response = RunResult, Error = CliError>,
    {
        WorkflowBuilder {
            task: self.task,
            env: self.env,
            verifier: self.verifier,
            lander: self.lander,
            sink: self.sink,
            consult: c,
        }
    }
}

impl<T, E, V, L, S, H> WorkflowBuilder<T, E, V, L, S, H>
where
    T: Task,
    E: Environment,
    V: Verifier,
    L: Lander,
    S: Sink,
    H: Harness<PromptRequest, Response = RunResult, Error = CliError>,
{
    /// Final builder step — provide the prompt-construction closure
    /// and receive the assembled Workflow. The closure runs at
    /// consult time with the Task, the prepared Environment, and the
    /// pre-verify outcome (in case structured failures are useful).
    pub fn prompt<F>(self, builder: F) -> Workflow<T, E, V, L, S, H>
    where
        F: Fn(&T, &E, &VerifyOutcome) -> String + Send + Sync + 'static,
    {
        Workflow {
            task: Arc::new(self.task),
            env: Arc::new(Mutex::new(self.env)),
            verifier: Arc::new(self.verifier),
            lander: Arc::new(self.lander),
            sink: Arc::new(self.sink),
            consult: Arc::new(self.consult),
            prompt_builder: Arc::new(builder),
        }
    }
}

// Entry point: `harness::workflow::WorkflowBuilder::new()` — the
// `Workflow` struct itself has trait bounds that the partially-built
// `()` slots can't satisfy, so a free builder is cleaner than a
// constructor on the struct.
