use super::environment::Environment;
use super::lander::Lander;
use super::outcome::{Outcome, WorkflowRequest};
use super::sink::Sink;
use super::task::Task;
use super::verifier::{VerifyOutcome, Verifier};
use harness_cli::{CliError, PromptRequest, RunResult};
use harness::{Capabilities, Harness};
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Builds the consult prompt at run-time from the Task, the prepared
/// Environment, and the pre-verify outcome. Aliased to keep the
/// `Workflow` struct field readable.
pub type PromptBuilder<T, E> =
    Arc<dyn Fn(&T, &E, &VerifyOutcome) -> String + Send + Sync>;

/// The state-machine that wraps a stochastic consult in a
/// deterministic verify-and-land tail.
///
/// Itself a `Harness<WorkflowRequest>` — composable like any other
/// Harness, stackable with tower layers when the `tower` feature is
/// on. Produces an `Outcome` terminal classification on every run.
///
/// Built via [`WorkflowBuilder`](super::WorkflowBuilder); see its
/// docs for construction. The fields are public-pub(super) only
/// inside this crate so the builder can populate them without an
/// awkward setter surface.
pub struct Workflow<T, E, V, L, S, H>
where
    T: Task,
    E: Environment,
    V: Verifier,
    L: Lander,
    S: Sink,
    H: Harness<PromptRequest, Response = RunResult, Error = CliError>,
{
    pub(super) task: Arc<T>,
    pub(super) env: Arc<Mutex<E>>,
    pub(super) verifier: Arc<V>,
    pub(super) lander: Arc<L>,
    pub(super) sink: Arc<S>,
    pub(super) consult: Arc<H>,
    pub(super) prompt_builder: PromptBuilder<T, E>,
}

#[derive(Debug, thiserror::Error)]
pub enum WorkflowError {
    #[error("sink failure while recording outcome: {0}")]
    SinkFailure(String),
}

#[async_trait]
impl<T, E, V, L, S, H> Harness<WorkflowRequest> for Workflow<T, E, V, L, S, H>
where
    T: Task + 'static,
    E: Environment + 'static,
    V: Verifier + 'static,
    L: Lander + 'static,
    S: Sink + 'static,
    H: Harness<PromptRequest, Response = RunResult, Error = CliError> + 'static,
{
    type Response = Outcome;
    type Error = WorkflowError;

    fn name(&self) -> &str {
        "workflow"
    }

    fn capabilities(&self) -> Capabilities {
        // Workflow doesn't fit the agentic-CLI capability axes — its
        // shape is task-in / outcome-out. Capabilities of the wrapped
        // consult Harness are observable separately via `self.consult`.
        Capabilities::none()
    }

    async fn run(&self, req: WorkflowRequest) -> Result<Outcome, WorkflowError> {
        let mut notes: Vec<String> = Vec::new();

        // Stage 1: prepare environment.
        {
            let mut env = self.env.lock().await;
            if let Err(e) = env.prepare().await {
                let outcome = Outcome::RunnerError {
                    stage: "prepare",
                    error: e.to_string(),
                    notes,
                };
                self.record(&outcome).await?;
                return Ok(outcome);
            }
        }

        // Stage 2: pre-verify (unless caller asked to skip).
        let preverify = if req.skip_preverify {
            None
        } else {
            let env = self.env.lock().await;
            match self.verifier.verify(&*env, &*self.task).await {
                Ok(v) => Some(v),
                Err(e) => {
                    notes.push(format!("pre-verify error (non-fatal): {e}"));
                    None
                }
            }
        };
        if matches!(&preverify, Some(v) if v.passed) {
            notes.push("pre-verify passed on pristine state; task already resolved".into());
            let outcome = Outcome::AlreadyResolved { notes };
            self.record(&outcome).await?;
            return Ok(outcome);
        }
        let preverify_outcome = preverify.unwrap_or_default();

        // Stage 3: consult.
        let prompt = {
            let env = self.env.lock().await;
            (self.prompt_builder)(&self.task, &*env, &preverify_outcome)
        };
        let mut prompt_req = PromptRequest::new(prompt);
        if let Some(n) = req.max_turns {
            prompt_req = prompt_req.max_turns(n);
        }
        if let Some(m) = req.model.clone() {
            prompt_req = prompt_req.model(m);
        }
        for (k, v) in req.env {
            prompt_req = prompt_req.env(k, v);
        }
        let workdir_path = {
            let env = self.env.lock().await;
            env.workdir().to_owned()
        };
        prompt_req = prompt_req.workdir(workdir_path);

        let consult_result = match self.consult.run(prompt_req).await {
            Ok(r) => r,
            Err(e) => {
                let outcome = Outcome::RunnerError {
                    stage: "consult",
                    error: e.to_string(),
                    notes,
                };
                self.record(&outcome).await?;
                return Ok(outcome);
            }
        };

        let consult_wall = consult_result.wall;
        let consult_messages = consult_result.messages;
        let consult_tokens_in = consult_result.tokens_in;
        let consult_tokens_out = consult_result.tokens_out;
        let consult_cost_usd = consult_result.cost_usd;

        // Stage 4: diff present?
        let has_diff = {
            let env = self.env.lock().await;
            match env.diff_present().await {
                Ok(b) => b,
                Err(e) => {
                    let outcome = Outcome::RunnerError {
                        stage: "diff-check",
                        error: e.to_string(),
                        notes,
                    };
                    self.record(&outcome).await?;
                    return Ok(outcome);
                }
            }
        };
        if !has_diff {
            let outcome = Outcome::NoDiff {
                consult_wall,
                consult_messages,
                consult_tokens_in,
                consult_tokens_out,
                consult_cost_usd,
                notes,
            };
            self.record(&outcome).await?;
            return Ok(outcome);
        }

        // Stage 5: post-verify.
        let post_verify = {
            let env = self.env.lock().await;
            match self.verifier.verify(&*env, &*self.task).await {
                Ok(v) => v,
                Err(e) => {
                    let outcome = Outcome::RunnerError {
                        stage: "verify",
                        error: e.to_string(),
                        notes,
                    };
                    self.record(&outcome).await?;
                    return Ok(outcome);
                }
            }
        };
        if !post_verify.passed {
            let outcome = Outcome::VerifyFailed {
                consult_wall,
                consult_messages,
                consult_tokens_in,
                consult_tokens_out,
                consult_cost_usd,
                verify_stderr_tail: post_verify.stderr_tail,
                notes,
            };
            self.record(&outcome).await?;
            return Ok(outcome);
        }

        // Stage 6: land.
        let landed = {
            let env = self.env.lock().await;
            match self.lander.land(&*env, &*self.task, &post_verify).await {
                Ok(r) => r,
                Err(e) => {
                    let outcome = Outcome::LandFailed {
                        consult_wall,
                        consult_messages,
                        consult_tokens_in,
                        consult_tokens_out,
                        consult_cost_usd,
                        lander_error: e.to_string(),
                        notes,
                    };
                    self.record(&outcome).await?;
                    return Ok(outcome);
                }
            }
        };

        let outcome = Outcome::Landed {
            consult_wall,
            consult_messages,
            consult_tokens_in,
            consult_tokens_out,
            consult_cost_usd,
            landed,
            notes,
        };
        self.record(&outcome).await?;
        Ok(outcome)
    }
}

impl<T, E, V, L, S, H> Workflow<T, E, V, L, S, H>
where
    T: Task,
    E: Environment,
    V: Verifier,
    L: Lander,
    S: Sink,
    H: Harness<PromptRequest, Response = RunResult, Error = CliError>,
{
    async fn record(&self, outcome: &Outcome) -> Result<(), WorkflowError> {
        self.sink
            .record(outcome)
            .await
            .map(|_| ())
            .map_err(|e| WorkflowError::SinkFailure(e.to_string()))
    }
}
