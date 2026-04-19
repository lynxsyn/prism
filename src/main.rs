mod app;
mod auth;
mod browser;
mod cli;
mod config;
mod github;
mod model;
mod poller;
mod ui;

use anyhow::Result;
use clap::Parser;

use crate::app::run_app;
use crate::auth::resolve_auth;
use crate::cli::{AuthSubcommand, Cli, Command, ConfigSubcommand};
use crate::config::{default_config_path, init_config, load_effective_config};

fn main() -> Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Some(Command::Config(command)) => {
            let path = cli.config.clone().unwrap_or(default_config_path()?);
            match command.command {
                ConfigSubcommand::Init { force } => {
                    init_config(&path, force)?;
                    println!("wrote {}", path.display());
                }
            }
            return Ok(());
        }
        Some(Command::Auth(command)) => {
            let config = load_effective_config(&cli)?;
            let auth = resolve_auth(&config.auth, &config.host)?;
            match command.command {
                AuthSubcommand::Status => {
                    println!("host: {}", config.host);
                    println!("source: {}", auth.source.label());
                    println!(
                        "token: ****{}",
                        auth.token
                            .chars()
                            .rev()
                            .take(4)
                            .collect::<String>()
                            .chars()
                            .rev()
                            .collect::<String>()
                    );
                }
            }
            return Ok(());
        }
        None => {}
    }

    let config = load_effective_config(&cli)?;
    if config.repos.is_empty() {
        anyhow::bail!(
            "no repositories configured; pass owner/repo arguments or create {}",
            config.config_path.display()
        );
    }
    let auth = resolve_auth(&config.auth, &config.host)?;
    run_app(config, auth)
}
