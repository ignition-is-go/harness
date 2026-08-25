//! Git-clone `Environment` for `harness-workflow`.
//!
//! On `prepare`:
//! - Creates a tempdir.
//! - `git clone --depth 50 --branch <base> <url> <tempdir>`.
//! - `git checkout -b <branch>` to start the agent's work branch.
//! - Sets `user.name` / `user.email` for any commit the Lander makes.
//! - Appends optional `.git/info/exclude` patterns so `git add -A`
//!   doesn't sweep build artifacts (e.g. `target/`, `dist/`) the agent
//!   produced during verify.
//!
//! `diff_present`: `git add -A && git diff --cached --stat <base>`;
//! reports `true` iff the staged diff against the base branch has any
//! content.

use async_trait::async_trait;
use harness_workflow::{Environment, EnvironmentError};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tempfile::TempDir;
use tokio::process::Command;

pub struct GitCloneEnv {
    repo_url: String,
    base_branch: String,
    branch_name: String,
    /// Local-only ignore patterns appended to `.git/info/exclude` after
    /// clone. Common entries: `target/`, `pulse-ctx-frontend/dist/`,
    /// `node_modules/`. Tracked in this struct so `prepare` can be
    /// idempotent.
    excludes: Vec<String>,
    /// Author identity written to local git config so any commit by
    /// downstream Landers carries the runner's signature.
    commit_author: Option<(String, String)>,
    /// Allocated by `prepare`; None until then. The TempDir handle
    /// keeps the directory alive for the lifetime of the env.
    tmp: Option<TempDir>,
    /// Cached path inside the tempdir; same lifetime as `tmp`.
    workdir: PathBuf,
}

impl GitCloneEnv {
    pub fn new(
        repo_url: impl Into<String>,
        base_branch: impl Into<String>,
        branch_name: impl Into<String>,
    ) -> Self {
        Self {
            repo_url: repo_url.into(),
            base_branch: base_branch.into(),
            branch_name: branch_name.into(),
            excludes: Vec::new(),
            commit_author: None,
            tmp: None,
            workdir: PathBuf::new(),
        }
    }

    pub fn exclude(mut self, pattern: impl Into<String>) -> Self {
        self.excludes.push(pattern.into());
        self
    }

    pub fn commit_author(mut self, name: impl Into<String>, email: impl Into<String>) -> Self {
        self.commit_author = Some((name.into(), email.into()));
        self
    }
}

#[async_trait]
impl Environment for GitCloneEnv {
    fn workdir(&self) -> &Path {
        &self.workdir
    }

    async fn prepare(&mut self) -> Result<(), EnvironmentError> {
        if self.tmp.is_some() {
            return Ok(()); // already prepared; idempotent.
        }
        let tmp = TempDir::with_prefix("harness-git-").map_err(EnvironmentError::Io)?;
        let workdir = tmp.path().to_owned();

        git(
            None,
            &[
                "clone",
                "--depth",
                "50",
                "--branch",
                &self.base_branch,
                &self.repo_url,
                workdir
                    .to_str()
                    .ok_or_else(|| EnvironmentError::Prepare("workdir path not UTF-8".into()))?,
            ],
        )
        .await?;

        git(Some(&workdir), &["checkout", "-b", &self.branch_name]).await?;

        if let Some((name, email)) = &self.commit_author {
            git(Some(&workdir), &["config", "user.name", name]).await?;
            git(Some(&workdir), &["config", "user.email", email]).await?;
        }

        if !self.excludes.is_empty() {
            let exclude_path = workdir.join(".git/info/exclude");
            let mut f = std::fs::OpenOptions::new()
                .append(true)
                .create(true)
                .open(&exclude_path)
                .map_err(EnvironmentError::Io)?;
            writeln!(f, "\n# harness-git local-only excludes").map_err(EnvironmentError::Io)?;
            for pat in &self.excludes {
                writeln!(f, "{pat}").map_err(EnvironmentError::Io)?;
            }
        }

        self.workdir = workdir;
        self.tmp = Some(tmp);
        Ok(())
    }

    async fn diff_present(&self) -> Result<bool, EnvironmentError> {
        git(Some(&self.workdir), &["add", "-A"]).await?;
        let out = git_capture(
            &self.workdir,
            &["diff", "--cached", "--stat", &self.base_branch],
        )
        .await?;
        Ok(!out.trim().is_empty())
    }
}

async fn git(cwd: Option<&Path>, args: &[&str]) -> Result<(), EnvironmentError> {
    let mut cmd = Command::new("git");
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    if let Some(c) = cwd {
        cmd.current_dir(c);
    }
    let out = cmd.output().await.map_err(EnvironmentError::Io)?;
    if !out.status.success() {
        return Err(EnvironmentError::Subprocess {
            code: out.status.code().unwrap_or(-1),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        });
    }
    Ok(())
}

async fn git_capture(cwd: &Path, args: &[&str]) -> Result<String, EnvironmentError> {
    let out = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(EnvironmentError::Io)?;
    if !out.status.success() {
        return Err(EnvironmentError::Subprocess {
            code: out.status.code().unwrap_or(-1),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        });
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}
