use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone)]
pub struct McpClientTarget {
    pub name: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct GitHubTokenInfo {
    pub login: String,
    pub name: Option<String>,
    pub email: Option<String>,
    pub scopes: Vec<String>,
    pub rate_limit_remaining: Option<u32>,
}

#[derive(Debug, Clone, Default)]
pub struct McpConfigManager;

impl McpConfigManager {
    pub fn new() -> Self {
        Self
    }

    /// Returns list of all known MCP configuration file locations for popular AI clients
    pub fn known_config_paths() -> Vec<McpClientTarget> {
        let mut targets = Vec::new();

        // 1. Check custom path from environment variable
        if let Ok(custom) = env::var("MCP_CONFIG_PATH") {
            let p = PathBuf::from(custom.trim());
            if !p.as_os_str().is_empty() {
                targets.push(McpClientTarget {
                    name: "Custom (from $MCP_CONFIG_PATH)".to_string(),
                    path: p,
                });
            }
        }

        // 2. Google Antigravity / Gemini CLI
        if let Some(home) = dirs::home_dir() {
            targets.push(McpClientTarget {
                name: "Antigravity / Gemini CLI".to_string(),
                path: home.join(".gemini").join("config").join("mcp_config.json"),
            });
            targets.push(McpClientTarget {
                name: "Antigravity Global".to_string(),
                path: home.join(".gemini").join("antigravity").join("mcp_config.json"),
            });
        }

        // 3. Claude Desktop
        #[cfg(target_os = "macos")]
        if let Some(home) = dirs::home_dir() {
            targets.push(McpClientTarget {
                name: "Claude Desktop".to_string(),
                path: home
                    .join("Library")
                    .join("Application Support")
                    .join("Claude")
                    .join("claude_desktop_config.json"),
            });
        }

        #[cfg(target_os = "windows")]
        if let Ok(appdata) = env::var("APPDATA") {
            targets.push(McpClientTarget {
                name: "Claude Desktop".to_string(),
                path: PathBuf::from(appdata)
                    .join("Claude")
                    .join("claude_desktop_config.json"),
            });
        }

        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        if let Some(config_dir) = dirs::config_dir() {
            targets.push(McpClientTarget {
                name: "Claude Desktop".to_string(),
                path: config_dir.join("Claude").join("claude_desktop_config.json"),
            });
        }

        // 4. Cursor
        if let Some(home) = dirs::home_dir() {
            targets.push(McpClientTarget {
                name: "Cursor (~/.cursor)".to_string(),
                path: home.join(".cursor").join("mcp.json"),
            });
        }
        if let Some(config_dir) = dirs::config_dir() {
            targets.push(McpClientTarget {
                name: "Cursor (~/.config/Cursor)".to_string(),
                path: config_dir.join("Cursor").join("mcp.json"),
            });
        }

        // 5. VS Code Extensions (Roo Code / Cline)
        if let Some(config_dir) = dirs::config_dir() {
            targets.push(McpClientTarget {
                name: "VS Code (Cline)".to_string(),
                path: config_dir
                    .join("Code")
                    .join("User")
                    .join("globalStorage")
                    .join("saoudrizwan.claude-dev")
                    .join("settings")
                    .join("cline_mcp_settings.json"),
            });
            targets.push(McpClientTarget {
                name: "VS Code (Roo Code)".to_string(),
                path: config_dir
                    .join("Code")
                    .join("User")
                    .join("globalStorage")
                    .join("rooveterinaryinc.cursor-mcp")
                    .join("mcp.json"),
            });
        }

        targets
    }

    /// Scan all known MCP config files on the system that currently exist
    pub fn scan_all_configs(&self) -> Vec<(McpClientTarget, Option<String>)> {
        let targets = Self::known_config_paths();
        let mut results = Vec::new();

        for target in targets {
            if target.path.exists() {
                let token = Self::extract_token_from_file(&target.path);
                results.push((target, token));
            }
        }

        results
    }

    /// Retrieve the currently active GitHub token across known MCP config files
    pub fn get_active_token(&self, custom_path: Option<&Path>) -> Option<(PathBuf, String)> {
        if let Some(p) = custom_path {
            if let Some(tok) = Self::extract_token_from_file(p) {
                return Some((p.to_path_buf(), tok));
            }
        }

        let existing = self.scan_all_configs();
        for (target, token_opt) in existing {
            if let Some(tok) = token_opt {
                return Some((target.path, tok));
            }
        }

        None
    }

    /// Extracts GitHub token from an MCP configuration JSON file
    pub fn extract_token_from_file(path: &Path) -> Option<String> {
        if !path.exists() {
            return None;
        }

        let raw = fs::read_to_string(path).ok()?;
        let root: Value = serde_json::from_str(&raw).ok()?;
        let mcp_servers = root.get("mcpServers")?.as_object()?;

        // Look for GitHub server entry
        for (key, val) in mcp_servers.iter() {
            let key_lower = key.to_lowercase();
            let cmd = val.get("command").and_then(|c| c.as_str()).unwrap_or("");
            let args = val
                .get("args")
                .and_then(|a| a.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|x| x.as_str())
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .unwrap_or_default();

            let env_obj = val.get("env").and_then(|e| e.as_object());
            let token = env_obj.and_then(|env| {
                env.get("GITHUB_PERSONAL_ACCESS_TOKEN")
                    .or_else(|| env.get("GITHUB_TOKEN"))
                    .and_then(|t| t.as_str())
                    .map(|s| s.trim().to_string())
            });

            if let Some(tok) = token {
                if !tok.is_empty() {
                    if key_lower.contains("github")
                        || cmd.contains("server-github")
                        || args.contains("server-github")
                    {
                        return Some(tok);
                    }
                    // Or if server has a GitHub token in env, return it as fallback
                    return Some(tok);
                }
            }
        }

        None
    }

    /// Updates or inserts GitHub MCP token into a specific MCP config file
    pub fn update_token_in_file(
        path: &Path,
        new_token: &str,
        create_if_missing: bool,
    ) -> Result<bool> {
        let token_trimmed = new_token.trim();

        if !path.exists() {
            if !create_if_missing {
                return Ok(false);
            }
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("Failed to create directory at {:?}", parent))?;
            }
            let root = json!({
                "mcpServers": {
                    "github-mcp-server": {
                        "command": "npx",
                        "args": ["-y", "@modelcontextprotocol/server-github"],
                        "env": {
                            "GITHUB_PERSONAL_ACCESS_TOKEN": token_trimmed
                        }
                    }
                }
            });
            let content = serde_json::to_string_pretty(&root)
                .context("Failed to serialize MCP JSON config")?;
            fs::write(path, content)
                .with_context(|| format!("Failed to write MCP config at {:?}", path))?;
            return Ok(true);
        }

        // Backup existing file
        let backup_path = path.with_extension("bak");
        let _ = fs::copy(path, &backup_path);

        let raw = fs::read_to_string(path)
            .with_context(|| format!("Failed to read MCP config at {:?}", path))?;
        let mut root: Value = serde_json::from_str(&raw)
            .with_context(|| format!("Failed to parse JSON in MCP config {:?}", path))?;

        if !root.is_object() {
            root = json!({});
        }

        let mcp_servers = match root.get_mut("mcpServers") {
            Some(v) => {
                if !v.is_object() {
                    *v = json!({});
                }
                v.as_object_mut().unwrap()
            }
            None => {
                root["mcpServers"] = json!({});
                root["mcpServers"].as_object_mut().unwrap()
            }
        };

        // Find existing GitHub server entry
        let mut target_server_key = None;
        for (key, val) in mcp_servers.iter() {
            let key_lower = key.to_lowercase();
            let cmd = val.get("command").and_then(|c| c.as_str()).unwrap_or("");
            let args = val
                .get("args")
                .and_then(|a| a.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|x| x.as_str())
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .unwrap_or_default();
            let env_has_token = val
                .get("env")
                .and_then(|e| e.as_object())
                .map(|env| {
                    env.contains_key("GITHUB_PERSONAL_ACCESS_TOKEN")
                        || env.contains_key("GITHUB_TOKEN")
                })
                .unwrap_or(false);

            if key_lower.contains("github")
                || cmd.contains("server-github")
                || args.contains("server-github")
                || env_has_token
            {
                target_server_key = Some(key.clone());
                break;
            }
        }

        let server_key = target_server_key.unwrap_or_else(|| "github-mcp-server".to_string());

        if !mcp_servers.contains_key(&server_key) {
            mcp_servers.insert(
                server_key.clone(),
                json!({
                    "command": "npx",
                    "args": ["-y", "@modelcontextprotocol/server-github"],
                    "env": {
                        "GITHUB_PERSONAL_ACCESS_TOKEN": token_trimmed
                    }
                }),
            );
        } else {
            let server_obj = mcp_servers.get_mut(&server_key).unwrap();
            if server_obj.get("env").is_none() || !server_obj["env"].is_object() {
                server_obj["env"] = json!({});
            }
            let env_map = server_obj["env"].as_object_mut().unwrap();
            env_map.insert(
                "GITHUB_PERSONAL_ACCESS_TOKEN".to_string(),
                json!(token_trimmed),
            );
            if env_map.contains_key("GITHUB_TOKEN") {
                env_map.insert("GITHUB_TOKEN".to_string(), json!(token_trimmed));
            }
        }

        let content = serde_json::to_string_pretty(&root)
            .context("Failed to serialize updated MCP config")?;

        let temp_path = path.with_extension("tmp");
        fs::write(&temp_path, content)
            .with_context(|| format!("Failed to write temporary MCP config at {:?}", temp_path))?;
        fs::rename(&temp_path, path)
            .with_context(|| format!("Failed to atomically replace MCP config at {:?}", path))?;

        Ok(true)
    }

    /// Removes GitHub MCP token from a specific MCP config file
    pub fn remove_token_from_file(path: &Path) -> Result<bool> {
        if !path.exists() {
            return Ok(false);
        }

        let raw = fs::read_to_string(path)
            .with_context(|| format!("Failed to read MCP config at {:?}", path))?;
        let mut root: Value = serde_json::from_str(&raw)
            .with_context(|| format!("Failed to parse JSON in {:?}", path))?;

        let mcp_servers = match root.get_mut("mcpServers").and_then(|v| v.as_object_mut()) {
            Some(obj) => obj,
            None => return Ok(false),
        };

        let mut changed = false;
        for (key, val) in mcp_servers.iter_mut() {
            let key_lower = key.to_lowercase();
            let cmd = val.get("command").and_then(|c| c.as_str()).unwrap_or("");
            let args = val
                .get("args")
                .and_then(|a| a.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|x| x.as_str())
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .unwrap_or_default();

            if key_lower.contains("github")
                || cmd.contains("server-github")
                || args.contains("server-github")
            {
                if let Some(env_map) = val.get_mut("env").and_then(|e| e.as_object_mut()) {
                    if env_map.remove("GITHUB_PERSONAL_ACCESS_TOKEN").is_some() {
                        changed = true;
                    }
                    if env_map.remove("GITHUB_TOKEN").is_some() {
                        changed = true;
                    }
                }
            }
        }

        if changed {
            let content = serde_json::to_string_pretty(&root)?;
            let temp_path = path.with_extension("tmp");
            fs::write(&temp_path, content)?;
            fs::rename(&temp_path, path)?;
        }

        Ok(changed)
    }

    /// Syncs a token to all detected MCP client configuration files
    pub fn sync_token(
        &self,
        token: &str,
        custom_path: Option<&Path>,
    ) -> Result<Vec<(PathBuf, bool)>> {
        if let Some(custom) = custom_path {
            let updated = Self::update_token_in_file(custom, token, true)?;
            return Ok(vec![(custom.to_path_buf(), updated)]);
        }

        let existing = self.scan_all_configs();
        let mut synced = Vec::new();

        if existing.is_empty() {
            // No existing config found, create the default Antigravity / Gemini CLI config
            let default_target = Self::known_config_paths()
                .into_iter()
                .find(|t| t.name.contains("Antigravity"))
                .unwrap_or_else(|| McpClientTarget {
                    name: "Default".to_string(),
                    path: dirs::home_dir()
                        .unwrap_or_else(|| PathBuf::from("."))
                        .join(".gemini")
                        .join("config")
                        .join("mcp_config.json"),
                });

            let updated = Self::update_token_in_file(&default_target.path, token, true)?;
            synced.push((default_target.path, updated));
        } else {
            for (target, _) in existing {
                let updated = Self::update_token_in_file(&target.path, token, false)?;
                synced.push((target.path, updated));
            }
        }

        Ok(synced)
    }

    /// Mask a token for safe terminal display (e.g. `ghp_QFoL...BkGQ`)
    pub fn mask_token(token: &str) -> String {
        let t = token.trim();
        if t.is_empty() {
            return "<empty>".to_string();
        }
        if t.len() <= 8 {
            return "********".to_string();
        }

        let prefix_len = if t.starts_with("ghp_") {
            8.min(t.len())
        } else if t.starts_with("github_pat_") {
            15.min(t.len())
        } else {
            4
        };

        let suffix_len = 4;
        if t.len() <= prefix_len + suffix_len {
            return format!("{}...", &t[..prefix_len]);
        }

        format!("{}...{}", &t[..prefix_len], &t[t.len() - suffix_len..])
    }

    /// Basic check for GitHub token format
    #[allow(dead_code)]
    pub fn is_valid_token_format(token: &str) -> bool {
        let t = token.trim();
        if t.is_empty() || t.contains(char::is_whitespace) {
            return false;
        }
        // Classic tokens: ghp_, Fine-grained: github_pat_, OAuth: gho_, etc.
        t.starts_with("ghp_")
            || t.starts_with("github_pat_")
            || t.starts_with("gho_")
            || t.starts_with("ghu_")
            || t.starts_with("ghs_")
            || (t.len() == 40 && t.chars().all(|c| c.is_ascii_hexdigit()))
    }

    /// Test and verify GitHub token against official GitHub API (`/user`)
    pub fn test_github_token(&self, token: &str) -> Result<GitHubTokenInfo> {
        let trimmed_token = token.trim();
        if trimmed_token.is_empty() {
            bail!("Token is empty");
        }

        let output = Command::new("curl")
            .args([
                "-s",
                "-i",
                "-H",
                &format!("Authorization: Bearer {}", trimmed_token),
                "-H",
                "User-Agent: GitChangerCLI",
                "-H",
                "Accept: application/vnd.github.v3+json",
                "https://api.github.com/user",
            ])
            .output()
            .context("Failed to execute curl command. Please ensure curl is installed.")?;

        if !output.status.success() {
            bail!("curl command failed with exit code {:?}", output.status.code());
        }

        let full_output = String::from_utf8_lossy(&output.stdout);
        let parts: Vec<&str> = full_output.splitn(2, "\r\n\r\n").collect();
        let (header_part, body_part) = if parts.len() == 2 {
            (parts[0], parts[1])
        } else {
            let alt_parts: Vec<&str> = full_output.splitn(2, "\n\n").collect();
            if alt_parts.len() == 2 {
                (alt_parts[0], alt_parts[1])
            } else {
                ("", full_output.as_ref())
            }
        };

        // Extract HTTP status code
        let first_line = header_part.lines().next().unwrap_or("");
        let status_code: u16 = first_line
            .split_whitespace()
            .nth(1)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);

        // Parse JSON body
        let body_val: Value = serde_json::from_str(body_part)
            .with_context(|| format!("Failed to parse GitHub API response body: {}", body_part))?;

        if status_code == 401 {
            let msg = body_val
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("Bad credentials or expired token");
            bail!("GitHub Authentication Failed (HTTP 401): {}", msg);
        }

        if status_code != 200 && status_code != 0 {
            let msg = body_val
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("Unknown error");
            bail!("GitHub API returned HTTP status {}: {}", status_code, msg);
        }

        let login = body_val
            .get("login")
            .and_then(|l| l.as_str())
            .context("GitHub response missing 'login' field")?
            .to_string();

        let name = body_val
            .get("name")
            .and_then(|n| n.as_str())
            .map(|s| s.to_string());

        let email = body_val
            .get("email")
            .and_then(|e| e.as_str())
            .map(|s| s.to_string());

        // Extract scopes from headers: x-oauth-scopes
        let mut scopes = Vec::new();
        let mut rate_limit = None;

        for line in header_part.lines() {
            let lower = line.to_lowercase();
            if lower.starts_with("x-oauth-scopes:") {
                let val = line[15..].trim();
                scopes = val
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            } else if lower.starts_with("x-ratelimit-remaining:") {
                if let Ok(limit) = line[22..].trim().parse::<u32>() {
                    rate_limit = Some(limit);
                }
            }
        }

        Ok(GitHubTokenInfo {
            login,
            name,
            email,
            scopes,
            rate_limit_remaining: rate_limit,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mask_token() {
        assert_eq!(
            McpConfigManager::mask_token("ghp_1234567890abcdef1234567890abcdef"),
            "ghp_1234...cdef"
        );
        assert_eq!(
            McpConfigManager::mask_token("github_pat_11AAAAAAA01234567890"),
            "github_pat_11AA...7890"
        );
        assert_eq!(McpConfigManager::mask_token("short"), "********");
        assert_eq!(McpConfigManager::mask_token(""), "<empty>");
    }

    #[test]
    fn test_token_format_validation() {
        assert!(McpConfigManager::is_valid_token_format("ghp_abc1234567890"));
        assert!(McpConfigManager::is_valid_token_format("github_pat_123456"));
        assert!(!McpConfigManager::is_valid_token_format("invalid token"));
        assert!(!McpConfigManager::is_valid_token_format(""));
    }

    #[test]
    fn test_update_and_extract_token_in_temp_file() {
        let temp_dir = std::env::temp_dir().join(format!("gitchanger_mcp_test_{}", std::process::id()));
        let _ = fs::create_dir_all(&temp_dir);
        let config_file = temp_dir.join("mcp_config.json");

        // 1. Create file with token
        let res = McpConfigManager::update_token_in_file(&config_file, "ghp_TestToken1234567890", true);
        assert!(res.is_ok());
        assert!(config_file.exists());

        // 2. Extract token
        let extracted = McpConfigManager::extract_token_from_file(&config_file);
        assert_eq!(extracted.as_deref(), Some("ghp_TestToken1234567890"));

        // 3. Update token
        let res_update = McpConfigManager::update_token_in_file(&config_file, "ghp_NewToken9876543210", false);
        assert!(res_update.is_ok());
        let extracted_updated = McpConfigManager::extract_token_from_file(&config_file);
        assert_eq!(extracted_updated.as_deref(), Some("ghp_NewToken9876543210"));

        // 4. Remove token
        let res_remove = McpConfigManager::remove_token_from_file(&config_file);
        assert!(res_remove.is_ok());
        let extracted_removed = McpConfigManager::extract_token_from_file(&config_file);
        assert_eq!(extracted_removed, None);

        // Clean up
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_preserves_other_servers() {
        let temp_dir = std::env::temp_dir().join(format!("gitchanger_mcp_preserve_test_{}", std::process::id()));
        let _ = fs::create_dir_all(&temp_dir);
        let config_file = temp_dir.join("mcp_config.json");

        // Write config with context7 and another custom server
        let initial_json = r#"{
            "mcpServers": {
                "context7": {
                    "url": "https://mcp.context7.com/mcp"
                },
                "postgres": {
                    "command": "npx",
                    "args": ["-y", "@modelcontextprotocol/server-postgres", "postgresql://localhost/mydb"]
                }
            }
        }"#;
        fs::write(&config_file, initial_json).unwrap();

        // Update token (should insert github-mcp-server without destroying context7 or postgres)
        let res = McpConfigManager::update_token_in_file(&config_file, "ghp_PreserveCheck12345", false);
        assert!(res.is_ok());

        let raw = fs::read_to_string(&config_file).unwrap();
        let val: Value = serde_json::from_str(&raw).unwrap();
        assert!(val["mcpServers"]["context7"].is_object());
        assert_eq!(val["mcpServers"]["context7"]["url"].as_str(), Some("https://mcp.context7.com/mcp"));
        assert!(val["mcpServers"]["postgres"].is_object());
        assert_eq!(
            val["mcpServers"]["github-mcp-server"]["env"]["GITHUB_PERSONAL_ACCESS_TOKEN"].as_str(),
            Some("ghp_PreserveCheck12345")
        );

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
