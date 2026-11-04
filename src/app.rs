use anyhow::{bail, Context, Result};
use colored::Colorize;
use comfy_table::modifiers::UTF8_ROUND_CORNERS;
use comfy_table::presets::UTF8_FULL;
use comfy_table::{Cell, Color, Row, Table};
use std::env;
use std::fs;
use std::path::PathBuf;

use crate::cli::AddArgs;
use crate::git::{GitManager, GitScope};
use crate::interactive::Interactive;
use crate::ssh_agent::SSHAgentManager;
use crate::ssh_config::SSHConfigManager;
use crate::types::{Config, GitProfile, Transport};

pub struct App {
    pub config: Config,
    pub git: GitManager,
    pub ssh_config: SSHConfigManager,
    pub ssh_agent: SSHAgentManager,
    pub cwd: PathBuf,
}

impl App {
    pub fn new() -> Result<Self> {
        let config = Config::load()?;
        let git = GitManager::new();
        let ssh_config = SSHConfigManager::new();
        let ssh_agent = SSHAgentManager::new();
        let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

        Ok(Self {
            config,
            git,
            ssh_config,
            ssh_agent,
            cwd,
        })
    }

    pub fn status(&self, verbose: bool) -> Result<()> {
        println!("{}", "⚡ GitChanger Status".bright_cyan().bold());
        println!("{}", "───────────────────".dimmed());

        let is_repo = self.git.is_git_repository(&self.cwd);
        if is_repo {
            let root = self.git.get_repo_root(&self.cwd)?;
            println!("  {} {}", "Repository:".bold(), root.display().to_string().green());
        } else {
            println!("  {} {}", "Repository:".bold(), "Not inside a git repository (showing global identity)".yellow());
        }

        let repo_status = self.git.inspect_status(&self.cwd, &self.config.profiles);

        // Active profile
        if let Some(ref profile) = repo_status.matched_profile {
            let def_badge = if profile.is_default { " [default]".bright_blue().to_string() } else { "".to_string() };
            println!(
                "  {} {} ({}) <{}>{}",
                "Active Profile:".bold(),
                profile.id.bright_green().bold(),
                profile.name,
                profile.email.cyan(),
                def_badge
            );

            match profile.transport {
                Transport::Ssh => {
                    let alias = profile.ssh_host_alias.as_deref().unwrap_or("unknown");
                    let host = profile.ssh_host_name.as_deref().unwrap_or("github.com");
                    println!("  {} SSH via host alias '{}' -> {}", "Transport:".bold(), alias.yellow(), host.dimmed());
                    if let Some(ref key) = profile.ssh_key_path {
                        println!("  {} {}", "SSH Key:".bold(), key.display().to_string().dimmed());
                    }
                }
                Transport::Https => {
                    println!("  {} HTTPS", "Transport:".bold());
                }
            }
        } else {
            let name = repo_status.user_name.as_deref().unwrap_or("Not configured");
            let email = repo_status.user_email.as_deref().unwrap_or("Not configured");
            println!(
                "  {} {} <{}> {}",
                "Git Identity:".bold(),
                name.yellow(),
                email.yellow(),
                "(No matching profile in GitChanger)".dimmed()
            );
            if !self.config.profiles.is_empty() {
                println!("  {} Run '{}' to link this repo to a profile.", "Tip:".cyan(), "gitchanger switch".bold());
            } else {
                println!("  {} Run '{}' to create your first profile.", "Tip:".cyan(), "gitchanger add".bold());
            }
        }

        if let Some(ref key) = repo_status.signing_key {
            println!("  {} {}", "Signing Key:".bold(), key.cyan());
        }

        // Remotes
        if !repo_status.remotes.is_empty() {
            println!("\n{}", "Git Remotes:".bold());
            for (name, url) in &repo_status.remotes {
                println!("  • {:<10} {}", name.bold(), url.dimmed());
            }
        }

        // Verbose info: SSH Agent & SSH Config
        if verbose {
            println!("\n{}", "SSH Diagnostics:".bold());
            let agent_running = self.ssh_agent.is_running();
            println!(
                "  {} {}",
                "SSH Agent:".bold(),
                if agent_running { "Running".green() } else { "Not running / unreachable".yellow() }
            );

            if agent_running {
                match self.ssh_agent.list_keys() {
                    Ok(keys) if keys.is_empty() => {
                        println!("  {} No keys loaded in ssh-agent", "Loaded Keys:".dimmed());
                    }
                    Ok(keys) => {
                        println!("  {}:", "Loaded Keys in Agent".bold());
                        for k in keys {
                            println!("    - {}", k.dimmed());
                        }
                    }
                    Err(e) => {
                        println!("  {} Failed to query ssh-agent: {}", "Loaded Keys:".yellow(), e);
                    }
                }
            }

            println!("  {} {}", "SSH Config File:".bold(), self.ssh_config.config_path().display().to_string().dimmed());
            println!("  {} {}", "Profiles Config:".bold(), Config::config_file_path()?.display().to_string().dimmed());
        }

        Ok(())
    }

    pub fn list(&self, verbose: bool) -> Result<()> {
        if self.config.profiles.is_empty() {
            println!("{}", "No profiles found in GitChanger.".yellow());
            println!("Run '{}' to create your first profile.", "gitchanger add".bright_cyan().bold());
            return Ok(());
        }

        let repo_status = self.git.inspect_status(&self.cwd, &self.config.profiles);
        let current_id = repo_status.matched_profile.as_ref().map(|p| p.id.as_str());

        let mut table = Table::new();
        table.load_preset(UTF8_FULL);
        table.apply_modifier(UTF8_ROUND_CORNERS);

        if verbose {
            table.set_header(vec![
                Cell::new("Active").fg(Color::Cyan),
                Cell::new("ID").fg(Color::Cyan),
                Cell::new("Name").fg(Color::Cyan),
                Cell::new("Email").fg(Color::Cyan),
                Cell::new("Transport").fg(Color::Cyan),
                Cell::new("Host Alias").fg(Color::Cyan),
                Cell::new("SSH Key Path").fg(Color::Cyan),
                Cell::new("Default").fg(Color::Cyan),
            ]);
        } else {
            table.set_header(vec![
                Cell::new("Active").fg(Color::Cyan),
                Cell::new("ID").fg(Color::Cyan),
                Cell::new("Name").fg(Color::Cyan),
                Cell::new("Email").fg(Color::Cyan),
                Cell::new("Transport").fg(Color::Cyan),
                Cell::new("Default").fg(Color::Cyan),
            ]);
        }

        for profile in &self.config.profiles {
            let is_curr = current_id.map(|id| id.eq_ignore_ascii_case(&profile.id)).unwrap_or(false);
            let active_cell = if is_curr {
                Cell::new("  ●  ").fg(Color::Green)
            } else {
                Cell::new("     ")
            };

            let default_cell = if profile.is_default {
                Cell::new("yes").fg(Color::Blue)
            } else {
                Cell::new("-").fg(Color::DarkGrey)
            };

            let transport_cell = match profile.transport {
                Transport::Ssh => Cell::new("SSH").fg(Color::Yellow),
                Transport::Https => Cell::new("HTTPS").fg(Color::Cyan),
            };

            let mut row = Row::new();
            row.add_cell(active_cell);
            row.add_cell(Cell::new(&profile.id).fg(Color::White));
            row.add_cell(Cell::new(&profile.name));
            row.add_cell(Cell::new(&profile.email));
            row.add_cell(transport_cell);

            if verbose {
                let alias_str = profile.ssh_host_alias.as_deref().unwrap_or("-");
                let key_str = profile
                    .ssh_key_path
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "-".to_string());
                row.add_cell(Cell::new(alias_str));
                row.add_cell(Cell::new(key_str));
            }

            row.add_cell(default_cell);
            table.add_row(row);
        }

        println!("{}", table);
        println!("Active marker: {} = current repository", "●".green());
        Ok(())
    }

    pub fn switch(
        &mut self,
        profile_query: Option<String>,
        global: bool,
        no_remote_update: bool,
        no_ssh_agent: bool,
        test_conn: bool,
    ) -> Result<()> {
        let repo_status = self.git.inspect_status(&self.cwd, &self.config.profiles);
        let current_id = repo_status.matched_profile.as_ref().map(|p| p.id.as_str());

        let target_profile = match profile_query {
            Some(ref query) => {
                self.config
                    .find_profile(query)
                    .cloned()
                    .with_context(|| format!("Profile '{}' not found", query))?
            }
            None => {
                if self.config.profiles.is_empty() {
                    println!("{}", "No profiles configured yet.".yellow());
                    if Interactive::confirm("Would you like to add a new profile now?", true)? {
                        let discovered_keys = self.ssh_config.scan_ssh_keys().unwrap_or_default();
                        let new_profile = Interactive::prompt_add_profile(&self.config.profiles, &discovered_keys)?;
                        self.config.add_profile(new_profile.clone())?;
                        new_profile
                    } else {
                        return Ok(());
                    }
                } else {
                    let selected = Interactive::select_profile(
                        &self.config.profiles,
                        current_id,
                        "Select Git profile to switch to:",
                    )?;
                    match selected {
                        Some(p) => p.clone(),
                        None => {
                            println!("{}", "Switch canceled.".dimmed());
                            return Ok(());
                        }
                    }
                }
            }
        };

        let scope = if global { GitScope::Global } else { GitScope::Local };
        let update_remotes = !no_remote_update;

        // Apply Git user.name and user.email + remotes
        let apply_result = self.git.apply_profile(&self.cwd, &target_profile, scope, update_remotes)?;

        // Ensure SSH Host block exists if SSH transport
        if target_profile.transport == Transport::Ssh && target_profile.ssh_host_alias.is_some() {
            if let Err(e) = self.ssh_config.add_or_update_host_block(&target_profile) {
                eprintln!("{} Failed to update ~/.ssh/config: {}", "Warning:".yellow(), e);
            }

            // SSH Agent handling
            if !no_ssh_agent && target_profile.use_ssh_agent {
                if let Some(ref key_path) = target_profile.ssh_key_path {
                    let _ = self.ssh_agent.start_agent();
                    match self.ssh_agent.add_key(key_path) {
                        Ok(_) => {
                            println!("  {} Added key {:?} to ssh-agent", "✓".green(), key_path.file_name().unwrap_or_default());
                        }
                        Err(e) => {
                            eprintln!("  {} Could not add key to ssh-agent: {}", "!".yellow(), e);
                        }
                    }
                }
            }

            // Test connection if requested
            if test_conn {
                if let Some(ref alias) = target_profile.ssh_host_alias {
                    println!("Testing SSH connection to '{}'...", alias.cyan());
                    let (ok, msg) = self.ssh_agent.test_connection(alias);
                    if ok {
                        println!("  {} SSH connection test succeeded!\n{}", "✓".green().bold(), msg.dimmed());
                    } else {
                        eprintln!("  {} SSH connection test failed:\n{}", "✗".red().bold(), msg);
                    }
                }
            }
        }

        println!(
            "\n{} Switched to profile '{}' ({}) <{}> [{}]",
            "✓".green().bold(),
            target_profile.id.bright_green().bold(),
            target_profile.name,
            target_profile.email.cyan(),
            scope.to_string().yellow()
        );

        if !apply_result.updated_remotes.is_empty() {
            println!("\n{}", "Updated Remote URLs:".bold());
            for (remote, old, new) in apply_result.updated_remotes {
                println!("  • {}: {} ➔ {}", remote.bold(), old.dimmed(), new.green());
            }
        }

        Ok(())
    }

    pub fn add(&mut self, args: AddArgs) -> Result<()> {
        let profile = if args.non_interactive || args.id.is_some() {
            // Build profile from CLI flags
            let id = args.id.context("Flag --id is required in non-interactive mode")?;
            let name = args.name.context("Flag --name is required in non-interactive mode")?;
            let email = args.email.context("Flag --email is required in non-interactive mode")?;
            let transport_str = args.transport.unwrap_or_else(|| "ssh".to_string());
            let transport: Transport = transport_str.parse()?;

            let mut p = GitProfile::new(id, name, email, transport);
            p.ssh_key_path = args.key;
            p.ssh_host_alias = args.alias;
            p.ssh_host_name = args.host.or_else(|| Some("github.com".to_string()));
            p.signing_key = args.signing_key;
            p.is_default = args.default;
            p.validate()?;
            p
        } else {
            let discovered_keys = self.ssh_config.scan_ssh_keys().unwrap_or_default();
            Interactive::prompt_add_profile(&self.config.profiles, &discovered_keys)?
        };

        // If SSH, add Host block to ~/.ssh/config
        if profile.transport == Transport::Ssh && profile.ssh_host_alias.is_some() {
            if let Err(e) = self.ssh_config.add_or_update_host_block(&profile) {
                eprintln!("{} Failed to add Host block to ~/.ssh/config: {}", "Warning:".yellow(), e);
            } else {
                println!("  {} Updated ~/.ssh/config with Host '{}'", "✓".green(), profile.ssh_host_alias.as_deref().unwrap_or(""));
            }
        }

        let id = profile.id.clone();
        self.config.add_profile(profile)?;
        println!(
            "\n{} Profile '{}' successfully created!",
            "✓".green().bold(),
            id.bright_green()
        );

        if !args.non_interactive && self.git.is_git_repository(&self.cwd) {
            if Interactive::confirm("Apply this profile to the current repository now?", true).unwrap_or(false) {
                self.switch(Some(id), false, false, false, false)?;
            }
        }

        Ok(())
    }

    pub fn edit(&mut self, profile_query: Option<String>) -> Result<()> {
        let repo_status = self.git.inspect_status(&self.cwd, &self.config.profiles);
        let current_id = repo_status.matched_profile.as_ref().map(|p| p.id.as_str());

        let target_profile = match profile_query {
            Some(ref query) => {
                self.config
                    .find_profile(query)
                    .cloned()
                    .with_context(|| format!("Profile '{}' not found", query))?
            }
            None => {
                let selected = Interactive::select_profile(
                    &self.config.profiles,
                    current_id,
                    "Select Git profile to edit:",
                )?;
                match selected {
                    Some(p) => p.clone(),
                    None => return Ok(()),
                }
            }
        };

        let old_alias = target_profile.ssh_host_alias.clone();
        let discovered_keys = self.ssh_config.scan_ssh_keys().unwrap_or_default();
        let updated = Interactive::prompt_edit_profile(&target_profile, &discovered_keys)?;

        // Update ~/.ssh/config if alias changed or new host block
        if let Some(ref old) = old_alias {
            if Some(old) != updated.ssh_host_alias.as_ref() {
                let _ = self.ssh_config.remove_host_block(old);
            }
        }

        if updated.transport == Transport::Ssh && updated.ssh_host_alias.is_some() {
            let _ = self.ssh_config.add_or_update_host_block(&updated);
        }

        self.config.update_profile(updated)?;
        println!("{} Profile updated successfully!", "✓".green().bold());
        Ok(())
    }

    pub fn delete(&mut self, profile_query: Option<String>, skip_confirm: bool, keep_ssh: bool) -> Result<()> {
        let repo_status = self.git.inspect_status(&self.cwd, &self.config.profiles);
        let current_id = repo_status.matched_profile.as_ref().map(|p| p.id.as_str());

        let target_id = match profile_query {
            Some(q) => q,
            None => {
                let selected = Interactive::select_profile(
                    &self.config.profiles,
                    current_id,
                    "Select Git profile to delete:",
                )?;
                match selected {
                    Some(p) => p.id.clone(),
                    None => return Ok(()),
                }
            }
        };

        if !skip_confirm {
            let confirmed = Interactive::confirm(
                &format!("Are you sure you want to delete profile '{}'?", target_id.bold()),
                false,
            )?;
            if !confirmed {
                println!("{}", "Deletion canceled.".dimmed());
                return Ok(());
            }
        }

        let removed = self.config.delete_profile(&target_id)?;

        // Clean up ~/.ssh/config if requested
        if !keep_ssh {
            if let Some(ref alias) = removed.ssh_host_alias {
                if let Ok(true) = self.ssh_config.remove_host_block(alias) {
                    println!("  {} Removed Host '{}' from ~/.ssh/config", "✓".green(), alias);
                }
            }
        }

        println!("{} Deleted profile '{}'.", "✓".green().bold(), target_id);
        Ok(())
    }

    pub fn set_default(&mut self, profile_query: Option<String>) -> Result<()> {
        let target_id = match profile_query {
            Some(q) => q,
            None => {
                let selected = Interactive::select_profile(
                    &self.config.profiles,
                    None,
                    "Select Git profile to set as default:",
                )?;
                match selected {
                    Some(p) => p.id.clone(),
                    None => return Ok(()),
                }
            }
        };

        self.config.set_default_profile(&target_id)?;
        println!("{} Profile '{}' set as default.", "✓".green().bold(), target_id);
        Ok(())
    }

    pub fn test(&self, profile_query: Option<String>) -> Result<()> {
        let target_profile = match profile_query {
            Some(ref q) => {
                self.config
                    .find_profile(q)
                    .cloned()
                    .with_context(|| format!("Profile '{}' not found", q))?
            }
            None => {
                let selected = Interactive::select_profile(
                    &self.config.profiles,
                    None,
                    "Select Git profile to test SSH connection:",
                )?;
                match selected {
                    Some(p) => p.clone(),
                    None => return Ok(()),
                }
            }
        };

        if target_profile.transport != Transport::Ssh {
            bail!("Profile '{}' uses HTTPS transport. SSH test is only applicable to SSH profiles.", target_profile.id);
        }

        let alias = target_profile
            .ssh_host_alias
            .as_deref()
            .context("Profile has no SSH host alias configured")?;

        println!("Testing SSH connection to host alias '{}'...", alias.cyan().bold());
        let (ok, msg) = self.ssh_agent.test_connection(alias);
        if ok {
            println!("\n{} Connection successful!\n{}", "✓".green().bold(), msg.dimmed());
        } else {
            eprintln!("\n{} Connection failed!\n{}", "✗".red().bold(), msg);
        }

        Ok(())
    }

    pub fn scan(&self) -> Result<()> {
        println!("{}", "🔍 Scanning SSH and Git Configuration...".bright_cyan().bold());
        println!("{}", "─────────────────────────────────────────".dimmed());

        let keys = self.ssh_config.scan_ssh_keys()?;
        println!("Found {} SSH private key(s) in ~/.ssh:", keys.len().to_string().cyan().bold());
        if keys.is_empty() {
            println!("  {}", "No private keys found in ~/.ssh/".dimmed());
        } else {
            for key in &keys {
                println!("  • {}", key.display().to_string().green());
            }
        }

        let host_aliases = self.ssh_config.get_all_host_aliases()?;
        println!("\nFound {} Host block(s) in ~/.ssh/config:", host_aliases.len().to_string().cyan().bold());
        if host_aliases.is_empty() {
            println!("  {}", "No Host blocks found in ~/.ssh/config".dimmed());
        } else {
            for alias in &host_aliases {
                let matched = self.config.find_profile(alias);
                let note = if let Some(p) = matched {
                    format!("(linked to profile '{}')", p.id).green()
                } else {
                    "(unlinked)".yellow()
                };
                println!("  • {:<20} {}", alias.bold(), note);
            }
        }

        println!("\n{}", "Configured GitChanger Profiles:".bold());
        for p in &self.config.profiles {
            println!("  • {:<12} {} <{}>", p.id.bright_cyan(), p.name, p.email.dimmed());
        }

        println!("\n{} Run '{}' to create a new profile with these keys.", "Tip:".cyan(), "gitchanger add".bold());
        Ok(())
    }

    pub fn export(&self, file_path: Option<PathBuf>) -> Result<()> {
        let json = serde_json::to_string_pretty(&self.config)?;
        match file_path {
            Some(path) => {
                fs::write(&path, json)
                    .with_context(|| format!("Failed to export profiles to {:?}", path))?;
                println!("{} Exported {} profile(s) to {:?}", "✓".green().bold(), self.config.profiles.len(), path);
            }
            None => {
                println!("{}", json);
            }
        }
        Ok(())
    }

    pub fn import(&mut self, file_path: PathBuf, force: bool) -> Result<()> {
        let content = fs::read_to_string(&file_path)
            .with_context(|| format!("Failed to read file {:?}", file_path))?;

        let imported: Config = if file_path.extension().and_then(|s| s.to_str()) == Some("toml") {
            toml::from_str(&content)?
        } else {
            serde_json::from_str(&content)?
        };

        let mut imported_count = 0;
        for profile in imported.profiles {
            let exists = self.config.profiles.iter().any(|p| p.id.eq_ignore_ascii_case(&profile.id));
            if exists {
                if force {
                    self.config.update_profile(profile)?;
                    imported_count += 1;
                } else {
                    println!("  {} Skipping existing profile '{}' (use --force to overwrite)", "•".yellow(), profile.id);
                }
            } else {
                self.config.add_profile(profile)?;
                imported_count += 1;
            }
        }

        println!("{} Imported {} profile(s) successfully.", "✓".green().bold(), imported_count);
        Ok(())
    }
}
