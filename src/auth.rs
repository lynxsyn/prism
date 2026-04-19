use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::config::EffectiveAuthConfig;

#[derive(Debug, Clone)]
pub enum AuthSource {
    EnvVar(String),
    ConfigToken,
    GhCli,
}

impl AuthSource {
    pub fn label(&self) -> String {
        match self {
            Self::EnvVar(name) => format!("env:{name}"),
            Self::ConfigToken => "config".to_string(),
            Self::GhCli => "gh".to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ResolvedAuth {
    pub token: String,
    pub source: AuthSource,
}

pub fn resolve_auth(config: &EffectiveAuthConfig, host: &str) -> Result<ResolvedAuth> {
    if let Ok(token) = std::env::var(&config.token_env) {
        let trimmed = token.trim();
        if !trimmed.is_empty() {
            return Ok(ResolvedAuth {
                token: trimmed.to_string(),
                source: AuthSource::EnvVar(config.token_env.clone()),
            });
        }
    }

    if let Some(token) = &config.token {
        let trimmed = token.trim();
        if !trimmed.is_empty() {
            return Ok(ResolvedAuth {
                token: trimmed.to_string(),
                source: AuthSource::ConfigToken,
            });
        }
    }

    if config.use_gh_fallback {
        let output = Command::new("gh")
            .args(["auth", "token", "--hostname", host])
            .output()
            .context("failed to invoke `gh auth token`")?;
        if output.status.success() {
            let token = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !token.is_empty() {
                return Ok(ResolvedAuth {
                    token,
                    source: AuthSource::GhCli,
                });
            }
        }
    }

    bail!(
        "no GitHub token available; set {}, add auth.token in config, or authenticate gh",
        config.token_env
    )
}
