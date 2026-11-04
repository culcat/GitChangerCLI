use anyhow::{bail, Context, Result};
use std::env;
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, Default)]
pub struct SSHAgentManager;

impl SSHAgentManager {
    pub fn new() -> Self {
        Self
    }

    pub fn is_running(&self) -> bool {
        // Exit code 0 means keys present, 1 means no keys, 2 means agent not reachable
        match Command::new("ssh-add").arg("-l").output() {
            Ok(output) => output.status.code() != Some(2),
            Err(_) => false,
        }
    }

    pub fn start_agent(&self) -> Result<()> {
        if self.is_running() {
            return Ok(());
        }

        let output = Command::new("ssh-agent")
            .arg("-s")
            .output()
            .context("Failed to execute 'ssh-agent -s'")?;

        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            bail!("Failed to start ssh-agent: {}", err);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            if let Some(rest) = line.strip_prefix("SSH_AUTH_SOCK=") {
                if let Some(val) = rest.split(';').next() {
                    env::set_var("SSH_AUTH_SOCK", val);
                }
            } else if let Some(rest) = line.strip_prefix("SSH_AGENT_PID=") {
                if let Some(val) = rest.split(';').next() {
                    env::set_var("SSH_AGENT_PID", val);
                }
            }
        }

        Ok(())
    }

    pub fn add_key(&self, key_path: &Path) -> Result<()> {
        let key_str = key_path.to_string_lossy();
        let output = Command::new("ssh-add")
            .arg(key_str.as_ref())
            .output()
            .with_context(|| format!("Failed to run ssh-add for {:?}", key_path))?;

        if output.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("ssh-add failed for {:?}: {}", key_path, stderr.trim());
        }
    }

    #[allow(dead_code)]
    pub fn remove_key(&self, key_path: &Path) -> Result<()> {
        let key_str = key_path.to_string_lossy();
        let output = Command::new("ssh-add")
            .arg("-d")
            .arg(key_str.as_ref())
            .output()
            .with_context(|| format!("Failed to run ssh-add -d for {:?}", key_path))?;

        if output.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("Failed to remove SSH key {:?}: {}", key_path, stderr.trim());
        }
    }

    pub fn list_keys(&self) -> Result<Vec<String>> {
        let output = Command::new("ssh-add")
            .arg("-l")
            .output()
            .context("Failed to run 'ssh-add -l'")?;

        if !output.status.success() {
            // code 1 means agent has no identities
            if output.status.code() == Some(1) {
                return Ok(Vec::new());
            }
            let err = String::from_utf8_lossy(&output.stderr);
            bail!("Failed to list ssh-agent keys: {}", err.trim());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let keys = stdout
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| l.to_string())
            .collect();

        Ok(keys)
    }

    pub fn test_connection(&self, host_alias: &str) -> (bool, String) {
        let target = format!("git@{}", host_alias);
        let output = Command::new("ssh")
            .args([
                "-T",
                "-o",
                "BatchMode=yes",
                "-o",
                "ConnectTimeout=7",
                "-o",
                "StrictHostKeyChecking=accept-new",
                &target,
            ])
            .output();

        match output {
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout).to_string();
                let stderr = String::from_utf8_lossy(&out.stderr).to_string();
                let combined = format!("{}\n{}", stdout, stderr).trim().to_string();

                let is_github_success = combined.contains("successfully authenticated");
                let is_gitlab_success = combined.contains("Welcome to GitLab");
                let is_bitbucket_success = combined.contains("logged in as") || combined.contains("authenticated via");
                let is_generic_success = out.status.success();

                let is_success = is_github_success || is_gitlab_success || is_bitbucket_success || is_generic_success;

                (is_success, combined)
            }
            Err(e) => (false, format!("Failed to execute ssh command: {}", e)),
        }
    }
}
