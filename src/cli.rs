use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

use crate::model::Mode;

#[derive(Debug, Parser)]
#[command(
    author,
    version,
    about = "Prism: a terminal dashboard for GitHub PRs and Actions"
)]
pub struct Cli {
    #[arg(global = true, short, long, value_name = "PATH")]
    pub config: Option<PathBuf>,
    #[arg(global = true, short = 'r', long = "repo", value_name = "OWNER/REPO")]
    pub repo: Vec<String>,
    #[arg(global = true, value_name = "OWNER/REPO")]
    pub repos: Vec<String>,
    #[arg(global = true, short, long, value_name = "SECONDS")]
    pub interval: Option<u64>,
    #[arg(global = true, short, long, value_enum)]
    pub mode: Option<Mode>,
    #[arg(global = true, long)]
    pub host: Option<String>,
    #[arg(global = true, long)]
    pub actions_limit: Option<usize>,
    #[arg(global = true, long)]
    pub prs_limit: Option<usize>,
    #[arg(global = true, long)]
    pub open_command: Option<String>,
    #[arg(global = true, long)]
    pub no_color: bool,
    #[arg(global = true, long)]
    pub ascii_only: bool,
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Auth(AuthCommand),
    Config(ConfigCommand),
}

#[derive(Debug, Args)]
pub struct AuthCommand {
    #[command(subcommand)]
    pub command: AuthSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum AuthSubcommand {
    Status,
}

#[derive(Debug, Args)]
pub struct ConfigCommand {
    #[command(subcommand)]
    pub command: ConfigSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum ConfigSubcommand {
    Init {
        #[arg(long)]
        force: bool,
    },
}
