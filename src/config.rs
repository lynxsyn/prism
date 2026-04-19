use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use crate::cli::Cli;
use crate::model::{Mode, RepoTarget};

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct FileConfig {
    pub host: Option<String>,
    pub interval: Option<u64>,
    pub mode: Option<Mode>,
    pub actions_limit: Option<usize>,
    pub prs_limit: Option<usize>,
    pub repos: Option<Vec<String>>,
    pub auth: Option<AuthConfig>,
    pub ui: Option<UiConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct AuthConfig {
    pub token: Option<String>,
    pub token_env: Option<String>,
    pub use_gh_fallback: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct UiConfig {
    pub theme: Option<String>,
    pub open_command: Option<String>,
    pub ascii_only: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct EffectiveConfig {
    pub host: String,
    pub interval: u64,
    pub mode: Mode,
    pub actions_limit: usize,
    pub prs_limit: usize,
    pub repos: Vec<RepoTarget>,
    pub auth: EffectiveAuthConfig,
    pub ui: EffectiveUiConfig,
    pub config_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct EffectiveAuthConfig {
    pub token: Option<String>,
    pub token_env: String,
    pub use_gh_fallback: bool,
}

#[derive(Debug, Clone)]
pub struct EffectiveUiConfig {
    pub open_command: Option<String>,
    pub ascii_only: bool,
    pub no_color: bool,
}

pub fn default_config_path() -> Result<PathBuf> {
    let dirs = ProjectDirs::from("app", "lynxsyn", "prism")
        .ok_or_else(|| anyhow!("could not determine Prism config directory"))?;
    Ok(dirs.config_dir().join("config.toml"))
}

pub fn load_effective_config(cli: &Cli) -> Result<EffectiveConfig> {
    let config_path = cli.config.clone().unwrap_or(default_config_path()?);

    let file_config = if config_path.exists() {
        let content = fs::read_to_string(&config_path)
            .with_context(|| format!("failed to read config from {}", config_path.display()))?;
        toml::from_str::<FileConfig>(&content)
            .with_context(|| format!("failed to parse {}", config_path.display()))?
    } else {
        FileConfig::default()
    };

    let configured_host = cli
        .host
        .clone()
        .or_else(|| std::env::var("PRISM_HOST").ok())
        .or(file_config.host)
        .unwrap_or_else(|| "github.com".to_string());

    let interval = cli
        .interval
        .or_else(|| {
            std::env::var("PRISM_INTERVAL")
                .ok()
                .and_then(|v| v.parse().ok())
        })
        .or(file_config.interval)
        .unwrap_or(10)
        .max(5);

    let mode = cli.mode.or(file_config.mode).unwrap_or_default();
    let actions_limit = cli
        .actions_limit
        .or(file_config.actions_limit)
        .unwrap_or(10);
    let prs_limit = cli.prs_limit.or(file_config.prs_limit).unwrap_or(30);

    let raw_repos = if !cli.repo.is_empty() || !cli.repos.is_empty() {
        cli.repo
            .iter()
            .chain(cli.repos.iter())
            .cloned()
            .collect::<Vec<_>>()
    } else {
        file_config.repos.unwrap_or_default()
    };

    let mut repos = raw_repos
        .into_iter()
        .map(|value| value.parse::<RepoTarget>().map_err(anyhow::Error::msg))
        .collect::<Result<Vec<_>>>()?;

    let explicit_hosts = repos
        .iter()
        .filter(|repo| repo.host != "github.com")
        .map(|repo| repo.host.clone())
        .collect::<BTreeSet<_>>();

    if explicit_hosts.len() > 1 {
        bail!("multiple repo hosts are not supported in one Prism session");
    }

    let host = match explicit_hosts.into_iter().next() {
        Some(explicit_host) if configured_host == "github.com" => explicit_host,
        Some(explicit_host) if explicit_host == configured_host => configured_host.clone(),
        Some(explicit_host) => {
            bail!("repo host `{explicit_host}` does not match configured host `{configured_host}`")
        }
        None => configured_host.clone(),
    };

    for repo in &mut repos {
        if repo.host == "github.com" || repo.host == host {
            repo.host = host.clone();
            continue;
        }
        bail!(
            "repo host `{}` does not match configured host `{host}`",
            repo.host
        );
    }

    let auth_cfg = file_config.auth.unwrap_or_default();
    let ui_cfg = file_config.ui.unwrap_or_default();

    Ok(EffectiveConfig {
        host: host.clone(),
        interval,
        mode,
        actions_limit,
        prs_limit,
        repos,
        auth: EffectiveAuthConfig {
            token: auth_cfg.token,
            token_env: auth_cfg
                .token_env
                .unwrap_or_else(|| "PRISM_TOKEN".to_string()),
            use_gh_fallback: auth_cfg.use_gh_fallback.unwrap_or(true),
        },
        ui: EffectiveUiConfig {
            open_command: normalize_optional_string(cli.open_command.clone())
                .or_else(|| normalize_optional_string(ui_cfg.open_command)),
            ascii_only: cli.ascii_only || ui_cfg.ascii_only.unwrap_or(false),
            no_color: cli.no_color,
        },
        config_path,
    })
}

fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    })
}

pub fn init_config(path: &Path, force: bool) -> Result<()> {
    if path.exists() && !force {
        bail!(
            "{} already exists; pass --force to overwrite",
            path.display()
        );
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    let example = r#"host = "github.com"
interval = 10
mode = "split"
actions_limit = 10
prs_limit = 30

repos = [
  "owner/repo-a",
  "owner/repo-b",
]

[auth]
token_env = "PRISM_TOKEN"
use_gh_fallback = true

[ui]
theme = "terminal"
open_command = ""
ascii_only = false
"#;

    fs::write(path, example).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;
    use crate::cli::Cli;

    #[test]
    fn parses_owner_repo() {
        let repo = "openai/codex".parse::<RepoTarget>().unwrap();
        assert_eq!(repo.host, "github.com");
        assert_eq!(repo.owner, "openai");
        assert_eq!(repo.name, "codex");
    }

    #[test]
    fn cli_repos_override_file_repos() {
        let temp = tempdir().unwrap();
        let config_path = temp.path().join("config.toml");
        fs::write(
            &config_path,
            r#"
interval = 10
repos = ["owner/from-file"]
"#,
        )
        .unwrap();

        let cli = Cli {
            config: Some(config_path),
            repo: vec!["owner/from-flag".to_string()],
            repos: vec!["owner/from-arg".to_string()],
            interval: None,
            mode: None,
            host: None,
            actions_limit: None,
            prs_limit: None,
            open_command: None,
            no_color: false,
            ascii_only: false,
            command: None,
        };

        let effective = load_effective_config(&cli).unwrap();
        let slugs = effective
            .repos
            .iter()
            .map(RepoTarget::slug)
            .collect::<Vec<_>>();
        assert_eq!(slugs, vec!["owner/from-flag", "owner/from-arg"]);
    }

    #[test]
    fn config_init_writes_template() {
        let temp = tempdir().unwrap();
        let config_path = temp.path().join("config.toml");

        init_config(&config_path, false).unwrap();

        let content = fs::read_to_string(config_path).unwrap();
        assert!(content.contains("repos = ["));
        assert!(content.contains("[auth]"));
    }

    #[test]
    fn infers_single_explicit_repo_host() {
        let cli = Cli {
            config: None,
            repo: vec![],
            repos: vec![
                "github.example.com/platform/api".to_string(),
                "platform/web".to_string(),
            ],
            interval: None,
            mode: None,
            host: None,
            actions_limit: None,
            prs_limit: None,
            open_command: None,
            no_color: false,
            ascii_only: false,
            command: None,
        };

        let effective = load_effective_config(&cli).unwrap();
        assert_eq!(effective.host, "github.example.com");
        assert_eq!(
            effective
                .repos
                .iter()
                .map(|repo| repo.host.clone())
                .collect::<Vec<_>>(),
            vec![
                "github.example.com".to_string(),
                "github.example.com".to_string()
            ]
        );
    }

    #[test]
    fn blank_open_command_is_ignored() {
        let temp = tempdir().unwrap();
        let config_path = temp.path().join("config.toml");
        fs::write(
            &config_path,
            r#"
repos = ["owner/repo"]

[ui]
open_command = "   "
"#,
        )
        .unwrap();

        let cli = Cli {
            config: Some(config_path),
            repo: vec![],
            repos: vec![],
            interval: None,
            mode: None,
            host: None,
            actions_limit: None,
            prs_limit: None,
            open_command: None,
            no_color: false,
            ascii_only: false,
            command: None,
        };

        let effective = load_effective_config(&cli).unwrap();
        assert_eq!(effective.ui.open_command, None);
    }
}
