use anyhow::{Context, Result};
use colored::Colorize;
use inquire::validator::Validation;
use inquire::{Confirm, Select, Text};
use std::fmt;
use std::path::PathBuf;

use crate::types::{GitProfile, Transport};

struct ProfileDisplayItem<'a> {
    profile: &'a GitProfile,
    is_current: bool,
}

impl<'a> fmt::Display for ProfileDisplayItem<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let current_marker = if self.is_current {
            "* ".green().bold()
        } else {
            "  ".normal()
        };

        let default_badge = if self.profile.is_default {
            " [default]".bright_blue().to_string()
        } else {
            "".to_string()
        };

        let transport_badge = match self.profile.transport {
            Transport::Ssh => {
                let alias = self.profile.ssh_host_alias.as_deref().unwrap_or("ssh");
                format!("[SSH: {}]", alias).yellow()
            }
            Transport::Https => "[HTTPS]".cyan(),
        };

        write!(
            f,
            "{}{} ({}) <{}> {} {}",
            current_marker,
            self.profile.id.bold(),
            self.profile.name,
            self.profile.email.dimmed(),
            transport_badge,
            default_badge
        )
    }
}

pub struct Interactive;

impl Interactive {
    pub fn select_profile<'a>(
        profiles: &'a [GitProfile],
        current_profile_id: Option<&str>,
        prompt_message: &str,
    ) -> Result<Option<&'a GitProfile>> {
        if profiles.is_empty() {
            println!("{}", "No profiles found in configuration.".yellow());
            return Ok(None);
        }

        let items: Vec<ProfileDisplayItem> = profiles
            .iter()
            .map(|p| ProfileDisplayItem {
                profile: p,
                is_current: current_profile_id.map(|id| id.eq_ignore_ascii_case(&p.id)).unwrap_or(false),
            })
            .collect();

        let selection = Select::new(prompt_message, items)
            .with_page_size(10)
            .prompt_skippable()
            .context("Profile selection canceled or failed")?;

        Ok(selection.map(|item| item.profile))
    }

    pub fn prompt_add_profile(
        existing_profiles: &[GitProfile],
        discovered_keys: &[PathBuf],
    ) -> Result<GitProfile> {
        println!("{}", "── Create New Git Profile ──".bright_cyan().bold());

        let existing_ids: Vec<String> = existing_profiles.iter().map(|p| p.id.to_lowercase()).collect();
        let id = Text::new("Profile identifier / nickname (e.g. 'work', 'personal'):")
            .with_validator(move |input: &str| {
                let trimmed = input.trim();
                if trimmed.is_empty() {
                    Ok(Validation::Invalid("ID cannot be empty".into()))
                } else if existing_ids.contains(&trimmed.to_lowercase()) {
                    Ok(Validation::Invalid("A profile with this ID already exists".into()))
                } else if !trimmed.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
                    Ok(Validation::Invalid("ID should contain letters, numbers, dash or underscore".into()))
                } else {
                    Ok(Validation::Valid)
                }
            })
            .prompt()?;

        let default_name = existing_profiles.first().map(|p| p.name.as_str()).unwrap_or("");
        let name_prompt = Text::new("Git Author Name (user.name):");
        let name = if !default_name.is_empty() {
            name_prompt.with_default(default_name).prompt()?
        } else {
            name_prompt.prompt()?
        };

        let email = Text::new("Git Author Email (user.email):")
            .with_validator(|input: &str| {
                let trimmed = input.trim();
                if trimmed.is_empty() || !trimmed.contains('@') || !trimmed.contains('.') {
                    Ok(Validation::Invalid("Please enter a valid email address".into()))
                } else {
                    Ok(Validation::Valid)
                }
            })
            .prompt()?;

        let transport_options = vec!["SSH (Recommended)", "HTTPS"];
        let transport_choice = Select::new("Transport method for Git remotes:", transport_options).prompt()?;

        let is_ssh = transport_choice.starts_with("SSH");
        let transport = if is_ssh { Transport::Ssh } else { Transport::Https };

        let mut ssh_key_path = None;
        let mut ssh_host_alias = None;
        let mut ssh_host_name = None;
        let mut use_ssh_agent = true;

        if is_ssh {
            let default_host = "github.com";
            ssh_host_name = Some(
                Text::new("Git remote server domain:")
                    .with_default(default_host)
                    .prompt()?,
            );

            let host_name_val = ssh_host_name.as_deref().unwrap_or(default_host);
            let suggested_alias = format!("{}-{}", host_name_val, id);
            ssh_host_alias = Some(
                Text::new("SSH host alias in ~/.ssh/config:")
                    .with_default(&suggested_alias)
                    .prompt()?,
            );

            // Select or input key path
            if !discovered_keys.is_empty() {
                let mut key_choices: Vec<String> = discovered_keys
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect();
                key_choices.push("Custom key path...".to_string());

                let selected_key = Select::new("Select SSH private key:", key_choices).prompt()?;
                if selected_key == "Custom key path..." {
                    let custom = Text::new("Enter full path to SSH private key:").prompt()?;
                    ssh_key_path = Some(PathBuf::from(shellexpand_tilde(&custom)));
                } else {
                    ssh_key_path = Some(PathBuf::from(selected_key));
                }
            } else {
                let path_input = Text::new("Path to SSH private key (e.g. ~/.ssh/id_ed25519):").prompt()?;
                ssh_key_path = Some(PathBuf::from(shellexpand_tilde(&path_input)));
            }

            use_ssh_agent = Confirm::new("Automatically add key to ssh-agent?")
                .with_default(true)
                .prompt()?;
        }

        let signing_key_input = Text::new("Commit signing key (optional, GPG/SSH key ID, press Enter to skip):")
            .prompt()?;
        let signing_key = if signing_key_input.trim().is_empty() {
            None
        } else {
            Some(signing_key_input.trim().to_string())
        };

        let token_input = Text::new("GitHub Personal Access Token for GitHub MCP (optional, press Enter to skip):")
            .prompt()?;
        let github_token = if token_input.trim().is_empty() {
            None
        } else {
            Some(token_input.trim().to_string())
        };

        let is_first = existing_profiles.is_empty();
        let is_default = if is_first {
            true
        } else {
            Confirm::new("Set this profile as default?")
                .with_default(false)
                .prompt()?
        };

        let mut profile = GitProfile::new(id, name, email, transport);
        profile.ssh_key_path = ssh_key_path;
        profile.ssh_host_alias = ssh_host_alias;
        profile.ssh_host_name = ssh_host_name;
        profile.use_ssh_agent = use_ssh_agent;
        profile.signing_key = signing_key;
        profile.github_token = github_token;
        profile.is_default = is_default;

        Ok(profile)
    }

    pub fn prompt_edit_profile(
        profile: &GitProfile,
        discovered_keys: &[PathBuf],
    ) -> Result<GitProfile> {
        println!("{}", format!("── Edit Profile: {} ──", profile.id).bright_cyan().bold());

        let name = Text::new("Git Author Name:")
            .with_default(&profile.name)
            .prompt()?;

        let email = Text::new("Git Author Email:")
            .with_default(&profile.email)
            .prompt()?;

        let transport_options = vec!["SSH (Recommended)", "HTTPS"];
        let default_idx = if profile.transport == Transport::Ssh { 0 } else { 1 };
        let transport_choice = Select::new("Transport method:", transport_options)
            .with_starting_cursor(default_idx)
            .prompt()?;

        let is_ssh = transport_choice.starts_with("SSH");
        let transport = if is_ssh { Transport::Ssh } else { Transport::Https };

        let mut ssh_key_path = profile.ssh_key_path.clone();
        let mut ssh_host_alias = profile.ssh_host_alias.clone();
        let mut ssh_host_name = profile.ssh_host_name.clone();
        let mut use_ssh_agent = profile.use_ssh_agent;

        if is_ssh {
            let default_host = ssh_host_name.as_deref().unwrap_or("github.com");
            ssh_host_name = Some(
                Text::new("Git remote server domain:")
                    .with_default(default_host)
                    .prompt()?,
            );

            let host_name_val = ssh_host_name.as_deref().unwrap_or("github.com");
            let default_alias = ssh_host_alias.unwrap_or_else(|| format!("{}-{}", host_name_val, profile.id));
            ssh_host_alias = Some(
                Text::new("SSH host alias in ~/.ssh/config:")
                    .with_default(&default_alias)
                    .prompt()?,
            );

            let current_key_str = ssh_key_path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_default();

            if !discovered_keys.is_empty() {
                let mut key_choices: Vec<String> = discovered_keys
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect();
                key_choices.push("Custom key path...".to_string());

                let start_idx = key_choices
                    .iter()
                    .position(|k| k == &current_key_str)
                    .unwrap_or(0);

                let selected_key = Select::new("Select SSH private key:", key_choices)
                    .with_starting_cursor(start_idx)
                    .prompt()?;

                if selected_key == "Custom key path..." {
                    let custom = Text::new("Enter full path to SSH private key:")
                        .with_default(&current_key_str)
                        .prompt()?;
                    ssh_key_path = Some(PathBuf::from(shellexpand_tilde(&custom)));
                } else {
                    ssh_key_path = Some(PathBuf::from(selected_key));
                }
            } else {
                let path_input = Text::new("Path to SSH private key:")
                    .with_default(&current_key_str)
                    .prompt()?;
                ssh_key_path = Some(PathBuf::from(shellexpand_tilde(&path_input)));
            }

            use_ssh_agent = Confirm::new("Automatically add key to ssh-agent?")
                .with_default(profile.use_ssh_agent)
                .prompt()?;
        }

        let default_signing = profile.signing_key.as_deref().unwrap_or("");
        let signing_key_input = Text::new("Commit signing key (optional, press Enter to clear/skip):")
            .with_default(default_signing)
            .prompt()?;
        let signing_key = if signing_key_input.trim().is_empty() {
            None
        } else {
            Some(signing_key_input.trim().to_string())
        };

        let current_tok_masked = profile
            .github_token
            .as_deref()
            .map(crate::mcp::McpConfigManager::mask_token)
            .unwrap_or_else(|| "none".to_string());

        let token_prompt = format!(
            "GitHub Personal Access Token for GitHub MCP [current: {}] (Enter to keep, '-' to clear, or paste new):",
            current_tok_masked
        );
        let token_input = Text::new(&token_prompt).prompt()?;
        let github_token = if token_input.trim().is_empty() {
            profile.github_token.clone()
        } else if token_input.trim() == "-" {
            None
        } else {
            Some(token_input.trim().to_string())
        };

        let is_default = Confirm::new("Set this profile as default?")
            .with_default(profile.is_default)
            .prompt()?;

        let mut updated = GitProfile::new(&profile.id, name, email, transport);
        updated.ssh_key_path = ssh_key_path;
        updated.ssh_host_alias = ssh_host_alias;
        updated.ssh_host_name = ssh_host_name;
        updated.use_ssh_agent = use_ssh_agent;
        updated.signing_key = signing_key;
        updated.github_token = github_token;
        updated.is_default = is_default;

        Ok(updated)
    }

    pub fn prompt_token(prompt_msg: &str) -> Result<String> {
        let token = inquire::Password::new(prompt_msg)
            .without_confirmation()
            .with_validator(|input: &str| {
                let trimmed = input.trim();
                if trimmed.is_empty() {
                    Ok(Validation::Invalid("Token cannot be empty".into()))
                } else if trimmed.contains(char::is_whitespace) {
                    Ok(Validation::Invalid("Token cannot contain whitespace".into()))
                } else {
                    Ok(Validation::Valid)
                }
            })
            .prompt()?;
        Ok(token)
    }

    pub fn confirm(message: &str, default: bool) -> Result<bool> {
        Confirm::new(message)
            .with_default(default)
            .prompt()
            .context("Confirmation prompt failed")
    }
}

pub fn shellexpand_tilde(path_str: &str) -> PathBuf {
    let trimmed = path_str.trim();
    if trimmed.starts_with('~') {
        if let Some(home) = dirs::home_dir() {
            if trimmed == "~" {
                return home;
            } else if trimmed.starts_with("~/") || trimmed.starts_with("~\\") {
                return home.join(&trimmed[2..]);
            }
        }
    }
    let p = PathBuf::from(trimmed);
    if !p.is_absolute() {
        if let Some(home) = dirs::home_dir() {
            let in_ssh = home.join(".ssh").join(trimmed);
            if in_ssh.exists() {
                return in_ssh;
            }
        }
    }
    p
}
