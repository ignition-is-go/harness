use async_trait::async_trait;
use harness_workflow::{
    Environment, LandedRef, Lander, LanderError, Task, VerifyOutcome,
};
use serde_json::json;
use std::process::Stdio;
use tokio::process::Command;

/// Commits any uncommitted working-tree changes, pushes the branch
/// to origin, and opens a PR via `gh pr create`.
///
/// The PR title and body are templated via `LanderConfig` so different
/// consumers (issue-fixer vs CVE remediator vs refactor) can customize
/// without rewriting the Lander.
pub struct GhPrLander {
    repo: String,
    base_branch: String,
    branch_name: String,
    config: LanderConfig,
}

/// Knobs for PR construction.
#[derive(Debug, Clone)]
pub struct LanderConfig {
    /// PR title format string with `{task_id}` and `{task_title}` placeholders.
    pub title_template: String,
    /// PR body format string with `{task_id}`, `{task_url}`, and
    /// `{credit}` placeholders.
    pub body_template: String,
    /// Credit string substituted into the body (e.g.
    /// `"Goose + ollama/gpt-oss-20b"`).
    pub credit: String,
    /// Author identity for the commit. None = leave existing git config.
    pub commit_author: Option<(String, String)>,
}

impl Default for LanderConfig {
    fn default() -> Self {
        Self {
            title_template: "fix: {task_title}".into(),
            body_template: "Resolves {task_url}.\n\nProduced by {credit}."
                .into(),
            credit: "an autonomous Workflow".into(),
            commit_author: None,
        }
    }
}

impl GhPrLander {
    pub fn new(
        repo: impl Into<String>,
        base_branch: impl Into<String>,
        branch_name: impl Into<String>,
    ) -> Self {
        Self {
            repo: repo.into(),
            base_branch: base_branch.into(),
            branch_name: branch_name.into(),
            config: LanderConfig::default(),
        }
    }

    pub fn with_config(mut self, config: LanderConfig) -> Self {
        self.config = config;
        self
    }
}

#[async_trait]
impl Lander for GhPrLander {
    async fn land(
        &self,
        env: &dyn Environment,
        task: &dyn Task,
        _verify: &VerifyOutcome,
    ) -> Result<LandedRef, LanderError> {
        let workdir = env.workdir();

        // Commit any uncommitted tracked changes (the Workflow's
        // diff_present check already staged everything via `git add -A`).
        let status = git(workdir, &["status", "--porcelain"]).await?;
        if !status.stdout_text().trim().is_empty() {
            if let Some((name, email)) = &self.config.commit_author {
                git(workdir, &["config", "user.name", name]).await?;
                git(workdir, &["config", "user.email", email]).await?;
            }
            let msg = format!(
                "fix: resolve {}\n\nProduced by {}.",
                task.id(),
                self.config.credit
            );
            git(workdir, &["commit", "-m", &msg]).await?;
        }

        let head = git(workdir, &["rev-parse", "HEAD"]).await?;
        let commit_sha = head.stdout_text().trim().to_owned();

        git(workdir, &["push", "-u", "origin", &self.branch_name]).await?;

        let title = self
            .config
            .title_template
            .replace("{task_id}", task.id())
            .replace("{task_title}", task.objective());
        let body = self
            .config
            .body_template
            .replace("{task_id}", task.id())
            .replace("{task_url}", task.id())
            .replace("{credit}", &self.config.credit);

        let gh_out = Command::new("gh")
            .args([
                "pr",
                "create",
                "--repo",
                &self.repo,
                "--base",
                &self.base_branch,
                "--head",
                &self.branch_name,
                "--title",
                &title,
                "--body",
                &body,
            ])
            .current_dir(workdir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(LanderError::Io)?;

        if !gh_out.status.success() {
            return Err(LanderError::Subprocess {
                code: gh_out.status.code().unwrap_or(-1),
                stderr: String::from_utf8_lossy(&gh_out.stderr).into_owned(),
            });
        }
        let pr_url = String::from_utf8_lossy(&gh_out.stdout)
            .trim()
            .lines()
            .last()
            .unwrap_or("")
            .to_owned();

        Ok(LandedRef {
            commit_sha: Some(commit_sha),
            url: Some(pr_url.clone()),
            metadata: json!({ "pr_url": pr_url, "branch": self.branch_name }),
        })
    }
}

struct GitOutput {
    stdout: Vec<u8>,
}

impl GitOutput {
    fn stdout_text(&self) -> std::borrow::Cow<'_, str> {
        String::from_utf8_lossy(&self.stdout)
    }
}

async fn git(cwd: &std::path::Path, args: &[&str]) -> Result<GitOutput, LanderError> {
    let out = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(LanderError::Io)?;
    if !out.status.success() {
        return Err(LanderError::Subprocess {
            code: out.status.code().unwrap_or(-1),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        });
    }
    Ok(GitOutput { stdout: out.stdout })
}
