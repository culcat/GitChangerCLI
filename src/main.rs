use clap::Parser;
use colored::Colorize;
use std::process;

mod app;
mod cli;
mod git;
mod interactive;
mod mcp;
mod ssh_agent;
mod ssh_config;
mod types;

use app::App;
use cli::{Cli, Commands, McpCommands};

fn main() {
    if let Err(e) = run() {
        eprintln!("\n{} {}", "Error:".red().bold(), e);
        process::exit(1);
    }
}

fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let mut app = App::new()?;

    match cli.command {
        Some(Commands::Status { verbose }) => {
            app.status(verbose)?;
        }
        Some(Commands::List { verbose }) => {
            app.list(verbose)?;
        }
        Some(Commands::Switch {
            profile,
            global,
            no_remote_update,
            no_ssh_agent,
            no_mcp,
            test,
        }) => {
            app.switch(profile, global, no_remote_update, no_ssh_agent, no_mcp, test)?;
        }
        Some(Commands::Add(args)) => {
            app.add(args)?;
        }
        Some(Commands::Edit { profile }) => {
            app.edit(profile)?;
        }
        Some(Commands::Delete {
            profile,
            yes,
            keep_ssh,
        }) => {
            app.delete(profile, yes, keep_ssh)?;
        }
        Some(Commands::Default { profile }) => {
            app.set_default(profile)?;
        }
        Some(Commands::Test { profile }) => {
            app.test(profile)?;
        }
        Some(Commands::Mcp { command }) => {
            match command {
                Some(McpCommands::Status) | None => {
                    app.mcp_status()?;
                }
                Some(McpCommands::Set {
                    profile,
                    token,
                    sync,
                    direct,
                    file,
                }) => {
                    app.mcp_set(profile, token, sync, direct, file)?;
                }
                Some(McpCommands::Get { profile, unmask }) => {
                    app.mcp_get(profile, unmask)?;
                }
                Some(McpCommands::Sync { profile, file }) => {
                    app.mcp_sync(profile, file)?;
                }
                Some(McpCommands::Remove { profile, clear_mcp }) => {
                    app.mcp_remove(profile, clear_mcp)?;
                }
                Some(McpCommands::Test { profile, token }) => {
                    app.mcp_test(profile, token)?;
                }
            }
        }
        Some(Commands::Scan) => {
            app.scan()?;
        }
        Some(Commands::Export { file }) => {
            app.export(file)?;
        }
        Some(Commands::Import { file, force }) => {
            app.import(file, force)?;
        }
        None => {
            // Default behavior when no subcommand is provided:
            // Show status, and if profiles exist, offer interactive switcher!
            app.status(false)?;

            if !app.config.profiles.is_empty() {
                println!();
                app.switch(None, false, false, false, false, false)?;
            } else {
                println!("\n{} No Git profiles configured yet.", "Notice:".yellow());
                if interactive::Interactive::confirm("Would you like to configure your first Git profile now?", true)? {
                    app.add(cli::AddArgs {
                        id: None,
                        name: None,
                        email: None,
                        transport: None,
                        key: None,
                        alias: None,
                        host: None,
                        signing_key: None,
                        token: None,
                        default: true,
                        non_interactive: false,
                    })?;
                }
            }
        }
    }

    Ok(())
}
