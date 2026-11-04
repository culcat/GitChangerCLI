use anyhow::{bail, Context, Result};
use regex::Regex;
use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::types::{GitProfile, Transport};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitScope {
    Local,
    Global,
}

impl GitScope {
    pub fn flag(&self) -> &'static str {
        match self {
            GitScope::Local => "--local",
            GitScope::Global => "--global",
        }
    }
}

impl fmt::Display for GitScope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GitScope::Local => write!(f, "local"),
            GitScope::Global => write!(f, "global"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct GitRepoStatus {
    pub is_repo: bool,
    pub repo_root: Option<PathBuf>,
    pub user_name: Option<String>,
    pub user_email: Option<String>,
    pub signing_key: Option<String>,
    pub remotes: Vec<(String, String)>,
    pub matched_profile: Option<GitProfile>,
}

#[derive(Debug, Clone)]
pub struct ApplyResult {
    pub profile_name: String,
    pub scope: GitScope,
    pub updated_user_name: String,
    pub updated_user_email: String,
    pub updated_signing_key: Option<String>,
    pub updated_remotes: Vec<(String, String, String)>, // (remote, old_url, new_url)
}

#[derive(Debug, Clone, Default)]
pub struct GitManager;

impl GitManager {
    pub fn new() -> Self {
        Self
    }

    pub fn is_git_repository(&self, cwd: &Path) -> bool {
        let output = Command::new("git")
            .args(["rev-parse", "--is-inside-work-tree"])
            .current_dir(cwd)
            .output();

        match output {
            Ok(out) => out.status.success() && String::from_utf8_lossy(&out.stdout).trim() == "true",
            Err(_) => false,
        }
    }

    pub fn get_repo_root(&self, cwd: &Path) -> Result<PathBuf> {
        let output = Command::new("git")
            .args(["rev-parse", "--show-toplevel"])
            .current_dir(cwd)
            .output()
            .context("Failed to run 'git rev-parse --show-toplevel'")?;

        if output.status.success() {
            let str_path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            Ok(PathBuf::from(str_path))
        } else {
            bail!("Not inside a git repository");
        }
    }

    pub fn get_config(&self, cwd: &Path, key: &str, scope: Option<GitScope>) -> Result<Option<String>> {
        let mut cmd = Command::new("git");
        cmd.arg("config");
        if let Some(s) = scope {
            cmd.arg(s.flag());
        }
        cmd.arg(key).current_dir(cwd);

        let output = cmd.output().context("Failed to run 'git config'")?;
        if output.status.success() {
            let val = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if val.is_empty() {
                Ok(None)
            } else {
                Ok(Some(val))
            }
        } else {
            // Non-zero exit code usually means key not found
            Ok(None)
        }
    }

    pub fn set_config(&self, cwd: &Path, key: &str, value: &str, scope: GitScope) -> Result<()> {
        let mut cmd = Command::new("git");
        cmd.args(["config", scope.flag(), key, value]).current_dir(cwd);

        let output = cmd.output().with_context(|| format!("Failed to set git config '{}'", key))?;
        if output.status.success() {
            Ok(())
        } else {
            let err = String::from_utf8_lossy(&output.stderr);
            bail!("Failed to set git config '{}': {}", key, err.trim());
        }
    }

    pub fn unset_config(&self, cwd: &Path, key: &str, scope: GitScope) -> Result<()> {
        let mut cmd = Command::new("git");
        cmd.args(["config", scope.flag(), "--unset", key]).current_dir(cwd);

        let output = cmd.output().with_context(|| format!("Failed to unset git config '{}'", key))?;
        // Exit code 5 means key doesn't exist, which is fine
        if output.status.success() || output.status.code() == Some(5) {
            Ok(())
        } else {
            let err = String::from_utf8_lossy(&output.stderr);
            bail!("Failed to unset git config '{}': {}", key, err.trim());
        }
    }

    pub fn get_remotes(&self, cwd: &Path) -> Result<Vec<(String, String)>> {
        let output = Command::new("git")
            .args(["remote", "-v"])
            .current_dir(cwd)
            .output()
            .context("Failed to run 'git remote -v'")?;

        if !output.status.success() {
            return Ok(Vec::new());
        }

        let mut remotes = Vec::new();
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            // format: origin  git@github.com:user/repo.git (fetch)
            if line.ends_with("(push)") || line.ends_with("(fetch)") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    let name = parts[0].to_string();
                    let url = parts[1].to_string();
                    if !remotes.iter().any(|(n, _)| n == &name) {
                        remotes.push((name, url));
                    }
                }
            }
        }

        Ok(remotes)
    }

    pub fn set_remote_url(&self, cwd: &Path, remote_name: &str, url: &str) -> Result<()> {
        let output = Command::new("git")
            .args(["remote", "set-url", remote_name, url])
            .current_dir(cwd)
            .output()
            .with_context(|| format!("Failed to set remote url for '{}'", remote_name))?;

        if output.status.success() {
            Ok(())
        } else {
            let err = String::from_utf8_lossy(&output.stderr);
            bail!("Failed to set remote url: {}", err.trim());
        }
    }

    pub fn convert_https_to_ssh(url: &str, ssh_host_alias: &str) -> String {
        let trimmed = url.trim();

        // Match https://[user@]host[:port]/repo.git
        let https_re = Regex::new(r"^https?://(?:[^@]+@)?([^/:]+)(?::\d+)?/(.+)$").unwrap();
        if let Some(caps) = https_re.captures(trimmed) {
            let path = caps.get(2).map(|m| m.as_str()).unwrap_or("");
            return format!("git@{}:{}", ssh_host_alias, path);
        }

        // Match git@old_host:repo.git
        let ssh_re = Regex::new(r"^(?:ssh://)?(?:git@)?([^:/]+)(?::\d+)?(?::|/)(.+)$").unwrap();
        if let Some(caps) = ssh_re.captures(trimmed) {
            let path = caps.get(2).map(|m| m.as_str()).unwrap_or("");
            return format!("git@{}:{}", ssh_host_alias, path);
        }

        trimmed.to_string()
    }

    pub fn convert_ssh_to_https(url: &str, ssh_host_name: &str) -> String {
        let trimmed = url.trim();

        // If already HTTPS with right domain
        if trimmed.starts_with("https://") {
            return trimmed.to_string();
        }

        // Match git@host:owner/repo.git or ssh://git@host/owner/repo.git
        let ssh_re = Regex::new(r"^(?:ssh://)?(?:git@)?([^:/]+)(?::\d+)?(?::|/)(.+)$").unwrap();
        if let Some(caps) = ssh_re.captures(trimmed) {
            let path = caps.get(2).map(|m| m.as_str()).unwrap_or("");
            return format!("https://{}/{}", ssh_host_name, path);
        }

        trimmed.to_string()
    }

    pub fn inspect_status(&self, cwd: &Path, profiles: &[GitProfile]) -> GitRepoStatus {
        let is_repo = self.is_git_repository(cwd);
        let repo_root = if is_repo { self.get_repo_root(cwd).ok() } else { None };

        let user_name = self.get_config(cwd, "user.name", None).ok().flatten();
        let user_email = self.get_config(cwd, "user.email", None).ok().flatten();
        let signing_key = self.get_config(cwd, "user.signingkey", None).ok().flatten();
        let remotes = if is_repo { self.get_remotes(cwd).unwrap_or_default() } else { Vec::new() };

        let mut matched_profile = None;
        if let (Some(ref name), Some(ref email)) = (&user_name, &user_email) {
            // Find by matching email and name
            let candidate = profiles.iter().find(|p| {
                p.email.eq_ignore_ascii_case(email) && p.name.eq_ignore_ascii_case(name)
            });

            if let Some(p) = candidate {
                matched_profile = Some(p.clone());
            } else {
                // Fallback: match by email alone
                let by_email = profiles.iter().find(|p| p.email.eq_ignore_ascii_case(email));
                if let Some(p) = by_email {
                    matched_profile = Some(p.clone());
                }
            }
        }

        GitRepoStatus {
            is_repo,
            repo_root,
            user_name,
            user_email,
            signing_key,
            remotes,
            matched_profile,
        }
    }

    pub fn apply_profile(
        &self,
        cwd: &Path,
        profile: &GitProfile,
        scope: GitScope,
        update_remotes: bool,
    ) -> Result<ApplyResult> {
        let is_repo = self.is_git_repository(cwd);

        if scope == GitScope::Local && !is_repo {
            bail!("Current directory is not a Git repository. Use --global to apply profile globally.");
        }

        // Set user.name & user.email
        self.set_config(cwd, "user.name", &profile.name, scope)?;
        self.set_config(cwd, "user.email", &profile.email, scope)?;

        // Set or unset signing key
        let mut updated_signing_key = None;
        if let Some(ref key) = profile.signing_key {
            self.set_config(cwd, "user.signingkey", key, scope)?;
            self.set_config(cwd, "commit.gpgsign", "true", scope)?;
            updated_signing_key = Some(key.clone());
        }

        // Update remotes if in repo and requested
        let mut updated_remotes = Vec::new();
        if is_repo && update_remotes {
            let remotes = self.get_remotes(cwd)?;
            for (remote_name, current_url) in remotes {
                let new_url = match profile.transport {
                    Transport::Ssh => {
                        if let Some(ref alias) = profile.ssh_host_alias {
                            Self::convert_https_to_ssh(&current_url, alias)
                        } else {
                            current_url.clone()
                        }
                    }
                    Transport::Https => {
                        let host = profile.ssh_host_name.as_deref().unwrap_or("github.com");
                        Self::convert_ssh_to_https(&current_url, host)
                    }
                };

                if new_url != current_url {
                    self.set_remote_url(cwd, &remote_name, &new_url)?;
                    updated_remotes.push((remote_name, current_url, new_url));
                }
            }
        }

        Ok(ApplyResult {
            profile_name: profile.name.clone(),
            scope,
            updated_user_name: profile.name.clone(),
            updated_user_email: profile.email.clone(),
            updated_signing_key,
            updated_remotes,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convert_https_to_ssh() {
        let https_url = "https://github.com/culcat/GitChangerCLI.git";
        let ssh = GitManager::convert_https_to_ssh(https_url, "gh-work");
        assert_eq!(ssh, "git@gh-work:culcat/GitChangerCLI.git");

        let https_without_git = "https://github.com/culcat/GitChangerCLI";
        let ssh2 = GitManager::convert_https_to_ssh(https_without_git, "gh-work");
        assert_eq!(ssh2, "git@gh-work:culcat/GitChangerCLI");

        let existing_ssh = "git@github.com:culcat/GitChangerCLI.git";
        let ssh3 = GitManager::convert_https_to_ssh(existing_ssh, "gh-work");
        assert_eq!(ssh3, "git@gh-work:culcat/GitChangerCLI.git");
    }

    #[test]
    fn test_convert_ssh_to_https() {
        let ssh_url = "git@gh-work:culcat/GitChangerCLI.git";
        let https = GitManager::convert_ssh_to_https(ssh_url, "github.com");
        assert_eq!(https, "https://github.com/culcat/GitChangerCLI.git");

        let already_https = "https://github.com/culcat/GitChangerCLI.git";
        let res = GitManager::convert_ssh_to_https(already_https, "github.com");
        assert_eq!(res, already_https);
    }
}
