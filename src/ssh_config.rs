use anyhow::{bail, Context, Result};
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use crate::types::GitProfile;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostBlock {
    pub start_line: usize,
    pub end_line: usize,
    pub host: String,
    pub lines: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct SSHConfigManager {
    config_path: PathBuf,
}

impl Default for SSHConfigManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SSHConfigManager {
    pub fn new() -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let config_path = home.join(".ssh").join("config");
        Self { config_path }
    }

    #[allow(dead_code)]
    pub fn with_path(path: impl Into<PathBuf>) -> Self {
        Self {
            config_path: path.into(),
        }
    }

    pub fn config_path(&self) -> &Path {
        &self.config_path
    }

    pub fn ssh_dir(&self) -> PathBuf {
        self.config_path.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| {
            dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")).join(".ssh")
        })
    }

    pub fn read_config(&self) -> Result<String> {
        if !self.config_path.exists() {
            return Ok(String::new());
        }
        fs::read_to_string(&self.config_path)
            .with_context(|| format!("Failed to read SSH config at {:?}", self.config_path))
    }

    pub fn write_config(&self, content: &str) -> Result<()> {
        let ssh_dir = self.ssh_dir();
        if !ssh_dir.exists() {
            fs::create_dir_all(&ssh_dir)
                .with_context(|| format!("Failed to create ~/.ssh directory at {:?}", ssh_dir))?;
            #[cfg(unix)]
            {
                let mut perms = fs::metadata(&ssh_dir)?.permissions();
                perms.set_mode(0o700);
                let _ = fs::set_permissions(&ssh_dir, perms);
            }
        }

        // Backup existing config
        if self.config_path.exists() {
            let backup_path = self.config_path.with_extension("bak");
            let _ = fs::copy(&self.config_path, &backup_path);
        }

        fs::write(&self.config_path, content)
            .with_context(|| format!("Failed to write SSH config at {:?}", self.config_path))?;

        #[cfg(unix)]
        {
            let mut perms = fs::metadata(&self.config_path)?.permissions();
            perms.set_mode(0o600);
            let _ = fs::set_permissions(&self.config_path, perms);
        }

        Ok(())
    }

    pub fn parse_host_blocks(&self, config_content: &str) -> Vec<HostBlock> {
        let lines: Vec<String> = config_content.lines().map(|s| s.to_string()).collect();
        let mut blocks = Vec::new();
        let mut i = 0;

        while i < lines.len() {
            let trimmed = lines[i].trim();
            if trimmed.to_lowercase().starts_with("host ") {
                let host_pattern = trimmed[5..].trim().to_string();
                let start = i;
                let mut block_lines = Vec::new();
                block_lines.push(lines[i].clone());

                i += 1;
                while i < lines.len() {
                    let cur_trimmed = lines[i].trim();
                    if !cur_trimmed.is_empty()
                        && !lines[i].starts_with(' ')
                        && !lines[i].starts_with('\t')
                        && cur_trimmed.to_lowercase().starts_with("host ")
                    {
                        break;
                    }
                    block_lines.push(lines[i].clone());
                    i += 1;
                }

                blocks.push(HostBlock {
                    start_line: start,
                    end_line: i,
                    host: host_pattern,
                    lines: block_lines,
                });
                continue;
            }
            i += 1;
        }

        blocks
    }

    pub fn generate_host_block(profile: &GitProfile) -> Result<String> {
        let alias = profile
            .ssh_host_alias
            .as_deref()
            .context("Profile is missing ssh_host_alias")?;
        let hostname = profile
            .ssh_host_name
            .as_deref()
            .context("Profile is missing ssh_host_name")?;
        let key_path = profile
            .ssh_key_path
            .as_ref()
            .context("Profile is missing ssh_key_path")?;

        let key_str = if key_path.is_absolute() {
            if let Some(home) = dirs::home_dir() {
                if let Ok(rel) = key_path.strip_prefix(&home) {
                    format!("~/{}", rel.display())
                } else {
                    key_path.to_string_lossy().to_string()
                }
            } else {
                key_path.to_string_lossy().to_string()
            }
        } else if !key_path.starts_with("~") {
            format!("~/.ssh/{}", key_path.display())
        } else {
            key_path.to_string_lossy().to_string()
        };

        Ok(format!(
            "Host {}\n    HostName {}\n    User git\n    IdentityFile {}\n    IdentitiesOnly yes\n",
            alias, hostname, key_str
        ))
    }

    pub fn add_or_update_host_block(&self, profile: &GitProfile) -> Result<()> {
        let alias = profile
            .ssh_host_alias
            .as_deref()
            .context("Profile requires ssh_host_alias for SSH configuration")?;

        if let Some(ref key_path) = profile.ssh_key_path {
            self.validate_ssh_key(key_path)?;
        }

        let content = self.read_config()?;
        let blocks = self.parse_host_blocks(&content);
        let lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
        let new_block_str = Self::generate_host_block(profile)?;
        let new_block_lines: Vec<String> = new_block_str.lines().map(|s| s.to_string()).collect();

        let existing = blocks.iter().position(|b| b.host.eq_ignore_ascii_case(alias));

        let new_content = if let Some(idx) = existing {
            let block = &blocks[idx];
            let mut result_lines = Vec::new();
            result_lines.extend_from_slice(&lines[..block.start_line]);
            result_lines.extend(new_block_lines);
            result_lines.extend_from_slice(&lines[block.end_line..]);
            result_lines.join("\n") + "\n"
        } else {
            let mut trimmed = content.trim_end().to_string();
            if !trimmed.is_empty() {
                trimmed.push_str("\n\n");
            }
            trimmed.push_str(&new_block_str);
            trimmed
        };

        self.write_config(&new_content)
    }

    pub fn remove_host_block(&self, alias: &str) -> Result<bool> {
        let content = self.read_config()?;
        let blocks = self.parse_host_blocks(&content);
        let lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();

        if let Some(block) = blocks.iter().find(|b| b.host.eq_ignore_ascii_case(alias)) {
            let mut result_lines = Vec::new();
            result_lines.extend_from_slice(&lines[..block.start_line]);
            result_lines.extend_from_slice(&lines[block.end_line..]);

            let mut new_content = result_lines.join("\n").trim_end().to_string();
            if !new_content.is_empty() {
                new_content.push('\n');
            }
            self.write_config(&new_content)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn get_all_host_aliases(&self) -> Result<Vec<String>> {
        let content = self.read_config()?;
        let blocks = self.parse_host_blocks(&content);
        Ok(blocks.into_iter().map(|b| b.host).collect())
    }

    pub fn resolve_key_path(&self, key_path: &Path) -> PathBuf {
        if key_path.starts_with("~") {
            if let Some(home) = dirs::home_dir() {
                home.join(key_path.strip_prefix("~").unwrap_or(key_path))
            } else {
                key_path.to_path_buf()
            }
        } else if !key_path.is_absolute() {
            let in_ssh = self.ssh_dir().join(key_path);
            if in_ssh.exists() {
                in_ssh
            } else {
                key_path.to_path_buf()
            }
        } else {
            key_path.to_path_buf()
        }
    }

    pub fn validate_ssh_key(&self, key_path: &Path) -> Result<()> {
        let resolved = self.resolve_key_path(key_path);

        if !resolved.exists() {
            bail!("SSH key file does not exist: {:?}", resolved);
        }

        #[cfg(unix)]
        {
            if let Ok(metadata) = fs::metadata(&resolved) {
                let mode = metadata.permissions().mode() & 0o777;
                // Private keys should ideally be 0600 or 0400
                if mode != 0o600 && mode != 0o400 {
                    eprintln!(
                        "Warning: SSH key permissions for {:?} are {:o}. Recommended is 600.",
                        resolved, mode
                    );
                }
            }
        }

        Ok(())
    }

    fn is_ssh_private_key_file(path: &Path, file_name: &str) -> bool {
        if file_name.starts_with("id_") || file_name.ends_with(".pem") || file_name.ends_with(".key") {
            return true;
        }

        // Check if matching .pub file exists
        let pub_path = path.parent().map(|p| p.join(format!("{}.pub", file_name)));
        if let Some(pub_p) = pub_path {
            if pub_p.exists() {
                return true;
            }
        }

        // Check file content for private key signature
        if let Ok(mut file) = fs::File::open(path) {
            use std::io::Read;
            let mut buffer = [0u8; 256];
            if let Ok(n) = file.read(&mut buffer) {
                if buffer[..n].windows(11).any(|window| window == b"PRIVATE KEY") {
                    return true;
                }
            }
        }

        false
    }

    pub fn scan_ssh_keys(&self) -> Result<Vec<PathBuf>> {
        let ssh_dir = self.ssh_dir();
        if !ssh_dir.exists() {
            return Ok(Vec::new());
        }

        let mut keys = Vec::new();
        for entry in fs::read_dir(&ssh_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() {
                let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                let is_pub = file_name.ends_with(".pub");
                let is_known = file_name.starts_with("known_hosts") || file_name == "config" || file_name == "authorized_keys";
                let is_bak = file_name.ends_with(".bak") || file_name.ends_with(".old");

                if !is_pub && !is_known && !is_bak && Self::is_ssh_private_key_file(&path, file_name) {
                    keys.push(path);
                }
            }
        }

        keys.sort();
        Ok(keys)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Transport;

    #[test]
    fn test_parse_host_blocks() {
        let content = r#"
Host github.com
    User git
    IdentityFile ~/.ssh/id_rsa

Host gitlab-work
    HostName gitlab.com
    User git
    IdentityFile ~/.ssh/id_ed25519
    IdentitiesOnly yes
"#;
        let manager = SSHConfigManager::new();
        let blocks = manager.parse_host_blocks(content);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].host, "github.com");
        assert_eq!(blocks[1].host, "gitlab-work");
    }

    #[test]
    fn test_add_and_update_host_block() {
        let temp_dir = tempfile_dir("ssh_test");
        let config_path = temp_dir.join("config");
        let manager = SSHConfigManager::with_path(&config_path);

        let mut profile = GitProfile::new("test", "Test User", "test@example.com", Transport::Ssh);
        profile.ssh_host_alias = Some("gh-test".to_string());
        profile.ssh_host_name = Some("github.com".to_string());
        let dummy_key = temp_dir.join("id_ed25519");
        fs::write(&dummy_key, "dummy key").unwrap();
        profile.ssh_key_path = Some(dummy_key);

        // Add
        manager.add_or_update_host_block(&profile).unwrap();
        let content = manager.read_config().unwrap();
        assert!(content.contains("Host gh-test"));
        assert!(content.contains("HostName github.com"));

        // Remove
        assert!(manager.remove_host_block("gh-test").unwrap());
        let content = manager.read_config().unwrap();
        assert!(!content.contains("Host gh-test"));
    }

    #[test]
    fn test_scan_ssh_keys_detects_custom_named_keys() {
        let temp_dir = tempfile_dir("ssh_scan_test");
        let config_path = temp_dir.join("config");
        let manager = SSHConfigManager::with_path(&config_path);

        // Key with .pub companion
        let custom_key = temp_dir.join("culcat");
        let custom_pub = temp_dir.join("culcat.pub");
        fs::write(&custom_key, "-----BEGIN OPENSSH PRIVATE KEY-----\ndummy").unwrap();
        fs::write(&custom_pub, "ssh-ed25519 AAAA...").unwrap();

        let scanned = manager.scan_ssh_keys().unwrap();
        assert!(scanned.contains(&custom_key));
    }

    fn tempfile_dir(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("gitchanger_test_{}_{}", name, std::process::id()));
        let _ = fs::create_dir_all(&path);
        path
    }
}
