use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs;
use std::path::PathBuf;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Transport {
    Ssh,
    Https,
}

impl Transport {
    pub fn is_ssh(&self) -> bool {
        matches!(self, Transport::Ssh)
    }

    pub fn is_https(&self) -> bool {
        matches!(self, Transport::Https)
    }
}

impl fmt::Display for Transport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Transport::Ssh => write!(f, "SSH"),
            Transport::Https => write!(f, "HTTPS"),
        }
    }
}

impl FromStr for Transport {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().trim() {
            "ssh" => Ok(Transport::Ssh),
            "https" => Ok(Transport::Https),
            other => bail!("Invalid transport: '{}'. Expected 'ssh' or 'https'", other),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GitProfile {
    pub id: String,
    pub name: String,
    pub email: String,
    pub transport: Transport,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssh_key_path: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssh_host_alias: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssh_host_name: Option<String>,
    #[serde(default = "default_true")]
    pub use_ssh_agent: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signing_key: Option<String>,
    #[serde(default)]
    pub is_default: bool,
}

fn default_true() -> bool {
    true
}

impl GitProfile {
    pub fn new(id: impl Into<String>, name: impl Into<String>, email: impl Into<String>, transport: Transport) -> Self {
        Self {
            id: id.into().trim().to_string(),
            name: name.into().trim().to_string(),
            email: email.into().trim().to_string(),
            transport,
            ssh_key_path: None,
            ssh_host_alias: None,
            ssh_host_name: None,
            use_ssh_agent: true,
            signing_key: None,
            is_default: false,
        }
    }

    pub fn validate(&self) -> Result<()> {
        if self.id.trim().is_empty() {
            bail!("Profile ID cannot be empty");
        }
        if self.name.trim().is_empty() {
            bail!("Profile Git name cannot be empty");
        }
        if self.email.trim().is_empty() {
            bail!("Profile Git email cannot be empty");
        }
        if !self.email.contains('@') || !self.email.contains('.') {
            bail!("Invalid email format: '{}'", self.email);
        }

        if self.transport == Transport::Ssh {
            if let Some(ref key_path) = self.ssh_key_path {
                if key_path.as_os_str().is_empty() {
                    bail!("SSH key path cannot be empty for SSH transport");
                }
            } else {
                bail!("SSH key path is required for SSH transport");
            }

            if let Some(ref alias) = self.ssh_host_alias {
                if alias.trim().is_empty() {
                    bail!("SSH host alias cannot be empty for SSH transport");
                }
            } else {
                bail!("SSH host alias is required for SSH transport");
            }

            if let Some(ref host) = self.ssh_host_name {
                if host.trim().is_empty() {
                    bail!("SSH host name cannot be empty for SSH transport");
                }
            } else {
                bail!("SSH host name is required for SSH transport");
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct Config {
    #[serde(default)]
    pub profiles: Vec<GitProfile>,
}

impl Config {
    pub fn config_dir() -> Result<PathBuf> {
        let base = dirs::config_dir().context("Could not determine user configuration directory")?;
        Ok(base.join("gitchanger"))
    }

    pub fn config_file_path() -> Result<PathBuf> {
        Ok(Self::config_dir()?.join("config.toml"))
    }

    pub fn load() -> Result<Self> {
        let path = Self::config_file_path()?;
        if !path.exists() {
            // Check legacy JSON config if exists
            let json_path = Self::config_dir()?.join("profiles.json");
            if json_path.exists() {
                let content = fs::read_to_string(&json_path)
                    .with_context(|| format!("Failed to read JSON config at {:?}", json_path))?;
                let config: Config = serde_json::from_str(&content)
                    .with_context(|| format!("Failed to parse JSON config at {:?}", json_path))?;
                // migrate to toml
                let _ = config.save();
                return Ok(config);
            }
            return Ok(Config::default());
        }

        let content = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read config file at {:?}", path))?;
        let config: Config = toml::from_str(&content)
            .with_context(|| format!("Failed to parse TOML config at {:?}", path))?;

        Ok(config)
    }

    pub fn save(&self) -> Result<()> {
        let dir = Self::config_dir()?;
        fs::create_dir_all(&dir)
            .with_context(|| format!("Failed to create config directory at {:?}", dir))?;

        let path = Self::config_file_path()?;
        let content = toml::to_string_pretty(self)
            .context("Failed to serialize config to TOML")?;

        let temp_path = path.with_extension("tmp");
        fs::write(&temp_path, content)
            .with_context(|| format!("Failed to write temporary config to {:?}", temp_path))?;
        fs::rename(&temp_path, &path)
            .with_context(|| format!("Failed to atomically update config file at {:?}", path))?;

        Ok(())
    }

    pub fn find_profile(&self, id_or_alias: &str) -> Option<&GitProfile> {
        let query = id_or_alias.trim().to_lowercase();
        self.profiles.iter().find(|p| {
            p.id.to_lowercase() == query
                || p.name.to_lowercase() == query
                || p.ssh_host_alias.as_deref().map(|s| s.to_lowercase()) == Some(query.clone())
        })
    }

    pub fn find_profile_mut(&mut self, id_or_alias: &str) -> Option<&mut GitProfile> {
        let query = id_or_alias.trim().to_lowercase();
        self.profiles.iter_mut().find(|p| {
            p.id.to_lowercase() == query
                || p.name.to_lowercase() == query
                || p.ssh_host_alias.as_deref().map(|s| s.to_lowercase()) == Some(query.clone())
        })
    }

    pub fn get_default_profile(&self) -> Option<&GitProfile> {
        self.profiles.iter().find(|p| p.is_default)
    }

    pub fn add_profile(&mut self, profile: GitProfile) -> Result<()> {
        profile.validate()?;

        if self.profiles.iter().any(|p| p.id.eq_ignore_ascii_case(&profile.id)) {
            bail!("Profile with ID '{}' already exists", profile.id);
        }

        let is_first = self.profiles.is_empty();
        let mut new_profile = profile;
        if is_first {
            new_profile.is_default = true;
        } else if new_profile.is_default {
            for p in &mut self.profiles {
                p.is_default = false;
            }
        }

        self.profiles.push(new_profile);
        self.save()?;
        Ok(())
    }

    pub fn update_profile(&mut self, profile: GitProfile) -> Result<()> {
        profile.validate()?;

        let index = self
            .profiles
            .iter()
            .position(|p| p.id.eq_ignore_ascii_case(&profile.id))
            .with_context(|| format!("Profile '{}' not found", profile.id))?;

        if profile.is_default {
            for p in &mut self.profiles {
                p.is_default = false;
            }
        }

        self.profiles[index] = profile;
        self.save()?;
        Ok(())
    }

    pub fn delete_profile(&mut self, id_or_alias: &str) -> Result<GitProfile> {
        let index = self
            .profiles
            .iter()
            .position(|p| {
                p.id.eq_ignore_ascii_case(id_or_alias)
                    || p.ssh_host_alias.as_deref().map(|s| s.eq_ignore_ascii_case(id_or_alias)).unwrap_or(false)
            })
            .with_context(|| format!("Profile '{}' not found", id_or_alias))?;

        let removed = self.profiles.remove(index);

        if removed.is_default && !self.profiles.is_empty() {
            self.profiles[0].is_default = true;
        }

        self.save()?;
        Ok(removed)
    }

    pub fn set_default_profile(&mut self, id_or_alias: &str) -> Result<()> {
        let found = self
            .profiles
            .iter_mut()
            .find(|p| {
                p.id.eq_ignore_ascii_case(id_or_alias)
                    || p.ssh_host_alias.as_deref().map(|s| s.eq_ignore_ascii_case(id_or_alias)).unwrap_or(false)
            })
            .is_some();

        if !found {
            bail!("Profile '{}' not found", id_or_alias);
        }

        for p in &mut self.profiles {
            p.is_default = p.id.eq_ignore_ascii_case(id_or_alias)
                || p.ssh_host_alias.as_deref().map(|s| s.eq_ignore_ascii_case(id_or_alias)).unwrap_or(false);
        }

        self.save()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_profile_validation() {
        let mut p = GitProfile::new("work", "John Work", "john@work.com", Transport::Https);
        assert!(p.validate().is_ok());

        p.transport = Transport::Ssh;
        assert!(p.validate().is_err()); // missing key, alias, host

        p.ssh_key_path = Some(PathBuf::from("/home/user/.ssh/id_ed25519"));
        p.ssh_host_alias = Some("github-work".to_string());
        p.ssh_host_name = Some("github.com".to_string());
        assert!(p.validate().is_ok());

        p.email = "invalid-email".to_string();
        assert!(p.validate().is_err());
    }

    #[test]
    fn test_transport_parse() {
        assert_eq!("ssh".parse::<Transport>().unwrap(), Transport::Ssh);
        assert_eq!("HTTPS".parse::<Transport>().unwrap(), Transport::Https);
        assert!("ftp".parse::<Transport>().is_err());
    }
}
