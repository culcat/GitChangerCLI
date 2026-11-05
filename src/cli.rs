use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "gitchanger",
    author = "culcat",
    version,
    about = "⚡ Fast, native CLI Git account & SSH switcher",
    long_about = "Manage multiple Git profiles and SSH configurations with ease.\nSwitch accounts per repository or globally, auto-update remote URLs and manage ~/.ssh/config seamlessly."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Show current repository git identity, matching profile, and remote URLs
    #[command(alias = "current", alias = "whoami")]
    Status {
        /// Show verbose diagnostics
        #[arg(short, long)]
        verbose: bool,
    },

    /// List all configured Git profiles
    #[command(alias = "ls")]
    List {
        /// Show full details including key paths and host aliases
        #[arg(short, long)]
        verbose: bool,
    },

    /// Switch active Git profile for the current repository or globally
    #[command(alias = "use")]
    Switch {
        /// Profile ID or host alias to switch to (interactive if omitted)
        profile: Option<String>,

        /// Apply profile globally (~/.gitconfig) instead of current repository
        #[arg(short, long)]
        global: bool,

        /// Do not update Git remote origin URLs
        #[arg(long)]
        no_remote_update: bool,

        /// Do not automatically add SSH key to ssh-agent
        #[arg(long)]
        no_ssh_agent: bool,

        /// Do not automatically sync GitHub MCP configuration token
        #[arg(long)]
        no_mcp: bool,

        /// Test SSH connection after switching
        #[arg(short, long)]
        test: bool,
    },

    /// Add a new Git profile (runs interactive wizard if flags omitted)
    Add(AddArgs),

    /// Edit an existing Git profile
    Edit {
        /// Profile ID to edit (interactive if omitted)
        profile: Option<String>,
    },

    /// Delete a Git profile
    #[command(alias = "rm")]
    Delete {
        /// Profile ID to delete (interactive if omitted)
        profile: Option<String>,

        /// Do not prompt for confirmation
        #[arg(short, long)]
        yes: bool,

        /// Do not remove corresponding Host entry from ~/.ssh/config
        #[arg(long)]
        keep_ssh: bool,
    },

    /// Set a profile as default
    Default {
        /// Profile ID to set as default (interactive if omitted)
        profile: Option<String>,
    },

    /// Test SSH connection for a profile
    Test {
        /// Profile ID to test (interactive if omitted)
        profile: Option<String>,
    },

    /// Manage GitHub MCP tokens (aliases: token, tokens)
    #[command(alias = "token", alias = "tokens")]
    Mcp {
        #[command(subcommand)]
        command: Option<McpCommands>,
    },

    /// Scan ~/.ssh directory and SSH config for existing keys and hosts
    Scan,

    /// Export profiles to a file or stdout
    Export {
        /// Output file path (JSON or TOML based on extension, stdout if omitted)
        file: Option<PathBuf>,
    },

    /// Import profiles from a file
    Import {
        /// Input file path (JSON or TOML)
        file: PathBuf,

        /// Overwrite existing profiles with the same ID
        #[arg(short, long)]
        force: bool,
    },
}

#[derive(Args, Debug)]
pub struct AddArgs {
    /// Unique profile identifier (e.g. 'work', 'personal')
    #[arg(short, long)]
    pub id: Option<String>,

    /// Git author name (user.name)
    #[arg(short, long)]
    pub name: Option<String>,

    /// Git author email (user.email)
    #[arg(short, long)]
    pub email: Option<String>,

    /// Transport protocol ('ssh' or 'https')
    #[arg(short, long)]
    pub transport: Option<String>,

    /// Path to SSH private key (for SSH transport)
    #[arg(short, long)]
    pub key: Option<PathBuf>,

    /// SSH Host alias in ~/.ssh/config (e.g. 'github-work')
    #[arg(short, long)]
    pub alias: Option<String>,

    /// SSH Host name domain (e.g. 'github.com')
    #[arg(long)]
    pub host: Option<String>,

    /// Signing key (GPG or SSH public key)
    #[arg(long)]
    pub signing_key: Option<String>,

    /// GitHub Personal Access Token for GitHub MCP
    #[arg(long = "token", alias = "github-token")]
    pub token: Option<String>,

    /// Set this profile as default
    #[arg(short, long)]
    pub default: bool,

    /// Skip interactive prompts (all required fields must be supplied via flags)
    #[arg(long)]
    pub non_interactive: bool,
}

#[derive(Subcommand, Debug)]
pub enum McpCommands {
    /// Show GitHub MCP token status and detected client configurations
    Status,

    /// Set GitHub MCP token for a profile or directly in MCP client configs
    Set {
        /// Profile ID to associate the token with
        profile: Option<String>,

        /// GitHub Personal Access Token (prompted securely if omitted)
        #[arg(short, long)]
        token: Option<String>,

        /// Immediately sync token to all detected MCP client configuration files
        #[arg(long, default_value_t = true)]
        sync: bool,

        /// Apply token directly to MCP configuration files without saving to a Git profile
        #[arg(long)]
        direct: bool,

        /// Path to custom MCP config file
        #[arg(short, long)]
        file: Option<PathBuf>,
    },

    /// Get configured GitHub MCP token for a profile or current MCP configuration
    Get {
        /// Profile ID (uses active or default profile if omitted)
        profile: Option<String>,

        /// Display unmasked full token (useful for copying or scripts)
        #[arg(short = 'u', long = "unmask", alias = "show-full")]
        unmask: bool,
    },

    /// Sync a profile's GitHub token to detected MCP client configuration files
    Sync {
        /// Profile ID whose token should be synced (uses active or default if omitted)
        profile: Option<String>,

        /// Path to custom MCP config file to sync to
        #[arg(short, long)]
        file: Option<PathBuf>,
    },

    /// Remove GitHub MCP token from a profile and/or MCP client configs
    #[command(alias = "rm")]
    Remove {
        /// Profile ID to remove token from
        profile: Option<String>,

        /// Also clear token from detected MCP client configuration files
        #[arg(long)]
        clear_mcp: bool,
    },

    /// Test and verify GitHub token against GitHub API
    Test {
        /// Profile ID whose token to test (or omit to test active MCP token)
        profile: Option<String>,

        /// Custom raw token string to test directly
        #[arg(short, long)]
        token: Option<String>,
    },
}
