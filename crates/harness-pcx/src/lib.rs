//! Pulse-ctx `Sink` implementation for `harness-workflow`.
//!
//! Posts `command_WriteContextRecord` over JSON-RPC. The endpoint
//! defaults to the in-org pulse-ctx instance but is fully
//! configurable — same Sink can target any pulse-ctx deployment
//! (dev, staging, prod, ephemeral test instance).
//!
//! Attributes on the record:
//! - Everything from `task.attributes()` (subject.*, axe.*, etc.)
//! - `actor.runtime` = `"harness-workflow"` (override with builder)
//! - `actor.host` = hostname
//! - `topic.kind` = derived from Outcome variant (`observation` for
//!   AlreadyResolved/Landed, `incident` for the failure variants)
//! - `source.mechanism` = `"agent-write"`
//! - `tier` = `"unreviewed"` (override with builder)

use async_trait::async_trait;
use harness_workflow::{Outcome, RecordId, Sink, SinkError, Task};
use serde_json::{json, Value};
use std::time::Duration;

pub const DEFAULT_ENDPOINT: &str = "http://pulse-ctx-01:8765/myko/mcp";

pub struct PcxSink<T: Task> {
    endpoint: String,
    http: reqwest::Client,
    task: std::sync::Arc<T>,
    actor_runtime: String,
    actor_operator: Option<String>,
    actor_model: Option<String>,
    actor_version: Option<String>,
    tier: String,
}

impl<T: Task> PcxSink<T> {
    pub fn new(endpoint: impl Into<String>, task: std::sync::Arc<T>) -> Result<Self, SinkError> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(|e| SinkError::Transport(e.to_string()))?;
        Ok(Self {
            endpoint: endpoint.into(),
            http,
            task,
            actor_runtime: "harness-workflow".into(),
            actor_operator: None,
            actor_model: None,
            actor_version: None,
            tier: "unreviewed".into(),
        })
    }

    pub fn actor_runtime(mut self, s: impl Into<String>) -> Self {
        self.actor_runtime = s.into();
        self
    }

    pub fn actor_operator(mut self, s: impl Into<String>) -> Self {
        self.actor_operator = Some(s.into());
        self
    }

    pub fn actor_model(mut self, s: impl Into<String>) -> Self {
        self.actor_model = Some(s.into());
        self
    }

    pub fn actor_version(mut self, s: impl Into<String>) -> Self {
        self.actor_version = Some(s.into());
        self
    }

    pub fn tier(mut self, s: impl Into<String>) -> Self {
        self.tier = s.into();
        self
    }

    fn topic_kind_for(o: &Outcome) -> &'static str {
        match o {
            Outcome::Landed { .. } | Outcome::AlreadyResolved { .. } => "observation",
            Outcome::NoDiff { .. }
            | Outcome::VerifyFailed { .. }
            | Outcome::LandFailed { .. }
            | Outcome::RunnerError { .. } => "incident",
        }
    }

    fn build_attributes(&self, outcome: &Outcome) -> Value {
        let mut attrs: serde_json::Map<String, Value> = serde_json::Map::new();
        for (k, v) in self.task.attributes() {
            attrs.insert(k, Value::String(v));
        }
        attrs.insert(
            "actor.runtime".into(),
            Value::String(self.actor_runtime.clone()),
        );
        attrs.insert(
            "actor.host".into(),
            Value::String(hostname()),
        );
        if let Some(v) = &self.actor_operator {
            attrs.insert("actor.operator".into(), Value::String(v.clone()));
        }
        if let Some(v) = &self.actor_model {
            attrs.insert("actor.model".into(), Value::String(v.clone()));
        }
        if let Some(v) = &self.actor_version {
            attrs.insert("actor.version".into(), Value::String(v.clone()));
        }
        attrs.insert(
            "topic.kind".into(),
            Value::String(Self::topic_kind_for(outcome).into()),
        );
        attrs.insert(
            "source.mechanism".into(),
            Value::String("agent-write".into()),
        );
        attrs.insert("tier".into(), Value::String(self.tier.clone()));
        attrs.insert(
            "workflow.status".into(),
            Value::String(outcome.status().into()),
        );
        // Per-outcome consult metrics (where available).
        if let Some(metrics) = consult_metrics(outcome) {
            attrs.extend(metrics);
        }
        Value::Object(attrs)
    }

    fn build_body(&self, outcome: &Outcome) -> String {
        let mut lines = vec![
            format!("workflow run on {} — status: {}", self.task.id(), outcome.status()),
            String::new(),
        ];
        match outcome {
            Outcome::AlreadyResolved { notes } => {
                lines.push("pre-verify passed on pristine state".into());
                push_notes(&mut lines, notes);
            }
            Outcome::NoDiff {
                consult_wall,
                consult_messages,
                consult_tokens_in,
                consult_tokens_out,
                consult_cost_usd,
                notes,
            } => {
                push_consult(
                    &mut lines,
                    *consult_wall,
                    *consult_messages,
                    *consult_tokens_in,
                    *consult_tokens_out,
                    *consult_cost_usd,
                );
                lines.push("no diff produced".into());
                push_notes(&mut lines, notes);
            }
            Outcome::VerifyFailed {
                consult_wall,
                consult_messages,
                consult_tokens_in,
                consult_tokens_out,
                consult_cost_usd,
                verify_stderr_tail,
                notes,
            } => {
                push_consult(
                    &mut lines,
                    *consult_wall,
                    *consult_messages,
                    *consult_tokens_in,
                    *consult_tokens_out,
                    *consult_cost_usd,
                );
                lines.push("verify rejected the diff".into());
                if let Some(tail) = verify_stderr_tail {
                    lines.push(format!("verify stderr tail:\n{tail}"));
                }
                push_notes(&mut lines, notes);
            }
            Outcome::LandFailed {
                consult_wall,
                consult_messages,
                consult_tokens_in,
                consult_tokens_out,
                consult_cost_usd,
                lander_error,
                notes,
            } => {
                push_consult(
                    &mut lines,
                    *consult_wall,
                    *consult_messages,
                    *consult_tokens_in,
                    *consult_tokens_out,
                    *consult_cost_usd,
                );
                lines.push(format!("lander failed: {lander_error}"));
                push_notes(&mut lines, notes);
            }
            Outcome::Landed {
                consult_wall,
                consult_messages,
                consult_tokens_in,
                consult_tokens_out,
                consult_cost_usd,
                landed,
                notes,
            } => {
                push_consult(
                    &mut lines,
                    *consult_wall,
                    *consult_messages,
                    *consult_tokens_in,
                    *consult_tokens_out,
                    *consult_cost_usd,
                );
                if let Some(sha) = &landed.commit_sha {
                    lines.push(format!("commit: {sha}"));
                }
                if let Some(url) = &landed.url {
                    lines.push(format!("landed at: {url}"));
                }
                push_notes(&mut lines, notes);
            }
            Outcome::RunnerError {
                stage,
                error,
                notes,
            } => {
                lines.push(format!("runner error at stage `{stage}`: {error}"));
                push_notes(&mut lines, notes);
            }
        }
        lines.join("\n")
    }
}

#[async_trait]
impl<T: Task + 'static> Sink for PcxSink<T> {
    async fn record(&self, outcome: &Outcome) -> Result<RecordId, SinkError> {
        let body = self.build_body(outcome);
        let attrs = self.build_attributes(outcome);
        let payload = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": "command_WriteContextRecord",
                "arguments": { "body": body, "attributes": attrs }
            }
        });

        let resp: Value = self
            .http
            .post(&self.endpoint)
            .json(&payload)
            .send()
            .await
            .map_err(|e| SinkError::Transport(e.to_string()))?
            .error_for_status()
            .map_err(|e| SinkError::Rejected(e.to_string()))?
            .json()
            .await
            .map_err(|e| SinkError::Encode(e.to_string()))?;

        if let Some(err) = resp.get("error") {
            return Err(SinkError::Rejected(err.to_string()));
        }
        let result = resp.get("result").cloned().unwrap_or(Value::Null);
        if result.get("isError").and_then(|v| v.as_bool()).unwrap_or(false) {
            let txt = result
                .pointer("/content/0/text")
                .and_then(|v| v.as_str())
                .unwrap_or("(no error text)");
            return Err(SinkError::Rejected(txt.into()));
        }

        // Unwrap content[0].text if JSON-encoded text (auto-derived myko endpoints).
        if let Some(text) = result.pointer("/content/0/text").and_then(|v| v.as_str()) {
            if let Ok(parsed) = serde_json::from_str::<Value>(text) {
                if let Some(id) = parsed.get("result").and_then(|v| v.as_str()) {
                    return Ok(id.into());
                }
                if let Some(id) = parsed.get("id").and_then(|v| v.as_str()) {
                    return Ok(id.into());
                }
                return Ok(parsed.to_string());
            }
            return Ok(text.into());
        }
        Ok(result.to_string())
    }
}

fn consult_metrics(o: &Outcome) -> Option<serde_json::Map<String, Value>> {
    let (wall, msgs, ti, to, cost) = match o {
        Outcome::NoDiff {
            consult_wall,
            consult_messages,
            consult_tokens_in,
            consult_tokens_out,
            consult_cost_usd,
            ..
        }
        | Outcome::VerifyFailed {
            consult_wall,
            consult_messages,
            consult_tokens_in,
            consult_tokens_out,
            consult_cost_usd,
            ..
        }
        | Outcome::LandFailed {
            consult_wall,
            consult_messages,
            consult_tokens_in,
            consult_tokens_out,
            consult_cost_usd,
            ..
        }
        | Outcome::Landed {
            consult_wall,
            consult_messages,
            consult_tokens_in,
            consult_tokens_out,
            consult_cost_usd,
            ..
        } => (
            *consult_wall,
            *consult_messages,
            *consult_tokens_in,
            *consult_tokens_out,
            *consult_cost_usd,
        ),
        _ => return None,
    };
    let mut m = serde_json::Map::new();
    m.insert(
        "consult.wall_seconds".into(),
        Value::String(format!("{:.1}", wall.as_secs_f64())),
    );
    m.insert("consult.messages".into(), Value::String(msgs.to_string()));
    m.insert("consult.tokens_in".into(), Value::String(ti.to_string()));
    m.insert("consult.tokens_out".into(), Value::String(to.to_string()));
    m.insert(
        "consult.cost_usd".into(),
        Value::String(format!("{cost:.4}")),
    );
    Some(m)
}

fn push_consult(
    lines: &mut Vec<String>,
    wall: Duration,
    messages: u32,
    tokens_in: u64,
    tokens_out: u64,
    cost: f64,
) {
    lines.push(format!("consult wall: {:.1}s", wall.as_secs_f64()));
    lines.push(format!("consult messages: {messages}"));
    lines.push(format!(
        "consult tokens: {tokens_in}→{tokens_out}"
    ));
    lines.push(format!("consult cost: ${cost:.4}"));
}

fn push_notes(lines: &mut Vec<String>, notes: &[String]) {
    if notes.is_empty() {
        return;
    }
    lines.push(String::new());
    lines.push("notes:".into());
    for n in notes {
        lines.push(format!("  - {n}"));
    }
}

fn hostname() -> String {
    std::env::var("HOSTNAME")
        .or_else(|_| {
            std::process::Command::new("hostname")
                .output()
                .ok()
                .and_then(|o| String::from_utf8(o.stdout).ok())
                .map(|s| s.trim().to_owned())
                .ok_or(std::env::VarError::NotPresent)
        })
        .unwrap_or_else(|_| "unknown".into())
}
