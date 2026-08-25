use harness_workflow::{Attributes, Task};
use regex::Regex;
use serde::Deserialize;
use std::process::Stdio;
use std::sync::OnceLock;
use tokio::process::Command;

pub struct GithubIssueTask {
    repo: String,
    number: u64,
    url: String,
    title: String,
    body: String,
    axe_rule_id: Option<String>,
    axe_flow_step: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum IssueError {
    #[error("malformed issue URL: {0}")]
    UrlParse(String),

    #[error("`gh` not found on PATH (install + auth required)")]
    GhMissing,

    #[error("`gh issue view` failed (rc={0}): {1}")]
    GhFailed(i32, String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("response parse failed: {0}")]
    Parse(String),
}

impl GithubIssueTask {
    pub async fn fetch(url: impl Into<String>) -> Result<Self, IssueError> {
        let url = url.into();
        let (repo, number) = parse_issue_url(&url)?;

        let out = Command::new("gh")
            .args([
                "issue",
                "view",
                &number.to_string(),
                "--repo",
                &repo,
                "--json",
                "title,body",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    IssueError::GhMissing
                } else {
                    IssueError::Io(e)
                }
            })?;

        if !out.status.success() {
            return Err(IssueError::GhFailed(
                out.status.code().unwrap_or(-1),
                String::from_utf8_lossy(&out.stderr).into_owned(),
            ));
        }

        #[derive(Deserialize)]
        struct GhResp {
            title: String,
            body: String,
        }
        let resp: GhResp =
            serde_json::from_slice(&out.stdout).map_err(|e| IssueError::Parse(e.to_string()))?;

        let (axe_rule_id, axe_flow_step) = extract_axe_context(&resp.body);

        Ok(Self {
            repo,
            number,
            url,
            title: resp.title,
            body: resp.body,
            axe_rule_id,
            axe_flow_step,
        })
    }

    pub fn repo(&self) -> &str {
        &self.repo
    }
    pub fn number(&self) -> u64 {
        self.number
    }
    pub fn url(&self) -> &str {
        &self.url
    }
    pub fn axe_rule_id(&self) -> Option<&str> {
        self.axe_rule_id.as_deref()
    }
    pub fn axe_flow_step(&self) -> Option<&str> {
        self.axe_flow_step.as_deref()
    }
}

impl Task for GithubIssueTask {
    fn id(&self) -> &str {
        &self.url
    }
    fn objective(&self) -> &str {
        &self.title
    }
    fn body(&self) -> &str {
        &self.body
    }
    fn attributes(&self) -> Attributes {
        let mut a = Attributes::new();
        a.insert("subject.external.system".into(), "github".into());
        a.insert("subject.external.repo".into(), self.repo.clone());
        a.insert("subject.external.url".into(), self.url.clone());
        a.insert("subject.external.id".into(), self.number.to_string());
        a.insert("subject.repo".into(), self.repo.clone());
        if let Some(r) = &self.axe_rule_id {
            a.insert("axe.rule_id".into(), r.clone());
        }
        if let Some(s) = &self.axe_flow_step {
            a.insert("axe.flow_step".into(), s.clone());
        }
        a
    }
}

fn parse_issue_url(url: &str) -> Result<(String, u64), IssueError> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re =
        RE.get_or_init(|| Regex::new(r"^https://github\.com/([^/]+/[^/]+)/issues/(\d+)").unwrap());
    let caps = re
        .captures(url)
        .ok_or_else(|| IssueError::UrlParse(url.to_owned()))?;
    let repo = caps.get(1).unwrap().as_str().to_owned();
    let number: u64 = caps
        .get(2)
        .unwrap()
        .as_str()
        .parse()
        .map_err(|_| IssueError::UrlParse(url.to_owned()))?;
    Ok((repo, number))
}

fn extract_axe_context(body: &str) -> (Option<String>, Option<String>) {
    static RULE_RE: OnceLock<Regex> = OnceLock::new();
    static STATE_RE: OnceLock<Regex> = OnceLock::new();
    let rule_re = RULE_RE.get_or_init(|| Regex::new(r"(?i)axe rule\s+`([a-z0-9-]+)`").unwrap());
    let state_re = STATE_RE
        .get_or_init(|| Regex::new(r#"(?i)during the ['"]([a-z0-9-]+)['"] state"#).unwrap());

    let rule = rule_re
        .captures(body)
        .and_then(|c| c.get(1).map(|m| m.as_str().to_owned()));
    let state = state_re
        .captures(body)
        .and_then(|c| c.get(1).map(|m| m.as_str().to_owned()));
    (rule, state)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_well_formed_url() {
        let (repo, num) =
            parse_issue_url("https://github.com/ignition-is-go/pulse-ctx/issues/135").unwrap();
        assert_eq!(repo, "ignition-is-go/pulse-ctx");
        assert_eq!(num, 135);
    }

    #[test]
    fn rejects_non_issue_url() {
        assert!(parse_issue_url("https://github.com/foo/bar/pull/1").is_err());
        assert!(parse_issue_url("not a url at all").is_err());
    }

    #[test]
    fn extracts_axe_context_from_ui_watchdog_body() {
        let body = "axe rule `landmark-unique` is violated during the 'routes-tab' state.";
        let (r, s) = extract_axe_context(body);
        assert_eq!(r.as_deref(), Some("landmark-unique"));
        assert_eq!(s.as_deref(), Some("routes-tab"));
    }

    #[test]
    fn axe_context_absent_for_generic_body() {
        let (r, s) = extract_axe_context("Please refactor the foo module.");
        assert!(r.is_none() && s.is_none());
    }
}
