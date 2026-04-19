use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;

use chrono::{DateTime, Utc};
use clap::ValueEnum;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Compact,
    #[default]
    Split,
}

impl fmt::Display for Mode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Compact => write!(f, "compact"),
            Self::Split => write!(f, "split"),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct RepoTarget {
    pub host: String,
    pub owner: String,
    pub name: String,
}

impl RepoTarget {
    pub fn slug(&self) -> String {
        format!("{}/{}", self.owner, self.name)
    }
}

impl fmt::Display for RepoTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.host == "github.com" {
            write!(f, "{}/{}", self.owner, self.name)
        } else {
            write!(f, "{}/{}/{}", self.host, self.owner, self.name)
        }
    }
}

impl FromStr for RepoTarget {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let trimmed = value.trim();
        let parts: Vec<&str> = trimmed.split('/').collect();
        match parts.as_slice() {
            [owner, name] if !owner.is_empty() && !name.is_empty() => Ok(Self {
                host: "github.com".to_string(),
                owner: (*owner).to_string(),
                name: (*name).to_string(),
            }),
            [host, owner, name] if !host.is_empty() && !owner.is_empty() && !name.is_empty() => {
                Ok(Self {
                    host: (*host).to_string(),
                    owner: (*owner).to_string(),
                    name: (*name).to_string(),
                })
            }
            _ => Err(format!(
                "invalid repo target `{trimmed}`; expected owner/repo or host/owner/repo"
            )),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct RateLimitState {
    pub limit: u32,
    pub remaining: u32,
    pub used: u32,
    pub reset_at: Option<DateTime<Utc>>,
    pub retry_after: Option<u64>,
}

#[derive(Clone, Debug)]
pub struct WorkflowRunSummary {
    pub repo: RepoTarget,
    pub id: u64,
    pub workflow_name: String,
    pub title: String,
    pub branch: String,
    pub event: String,
    pub status: String,
    pub conclusion: Option<String>,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
    pub url: String,
}

#[derive(Clone, Debug)]
pub struct WorkflowJobSummary {
    pub name: String,
    pub status: String,
    pub conclusion: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub total_steps: usize,
    pub completed_steps: usize,
    pub failed_step_name: Option<String>,
    pub indeterminate_progress: bool,
}

#[derive(Clone, Debug)]
pub struct WorkflowRunDetail {
    pub summary: WorkflowRunSummary,
    pub jobs: Vec<WorkflowJobSummary>,
    pub total_jobs: usize,
    pub completed_jobs: usize,
    pub failed_jobs: usize,
    pub running_jobs: usize,
}

#[derive(Clone, Debug)]
pub struct PullRequestSummary {
    pub repo: RepoTarget,
    pub number: u64,
    pub title: String,
    pub author: String,
    pub is_draft: bool,
    pub review_decision: Option<String>,
    pub review_requested_for_viewer: bool,
    pub ci_rollup: Option<String>,
    pub updated_at: DateTime<Utc>,
    pub url: String,
}

impl PullRequestSummary {
    pub fn stable_id(&self) -> String {
        format!("{}#{}", self.repo.slug(), self.number)
    }
}

#[derive(Clone, Debug)]
pub struct PullRequestCheckSummary {
    pub name: String,
    pub workflow_name: Option<String>,
    pub status: String,
    pub conclusion: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub url: Option<String>,
}

#[derive(Clone, Debug)]
pub struct PullRequestDetail {
    pub summary: PullRequestSummary,
    pub checks: Vec<PullRequestCheckSummary>,
    pub total_checks: usize,
    pub completed_checks: usize,
    pub passing_checks: usize,
    pub failing_checks: usize,
    pub running_checks: usize,
    pub pending_checks: usize,
}

#[derive(Clone, Debug)]
pub enum DetailView {
    Workflow(WorkflowRunDetail),
    PullRequest(PullRequestDetail),
}

impl DetailView {
    pub fn url(&self) -> &str {
        match self {
            Self::Workflow(detail) => detail.summary.url.as_str(),
            Self::PullRequest(detail) => detail.summary.url.as_str(),
        }
    }

    pub fn cache_key(&self) -> String {
        match self {
            Self::Workflow(detail) => DetailTarget::WorkflowRun {
                repo: detail.summary.repo.clone(),
                run_id: detail.summary.id,
            }
            .cache_key(),
            Self::PullRequest(detail) => DetailTarget::PullRequest {
                repo: detail.summary.repo.clone(),
                number: detail.summary.number,
            }
            .cache_key(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum FocusPane {
    #[default]
    Actions,
    PullRequests,
}

impl FocusPane {
    pub fn toggle(self) -> Self {
        match self {
            Self::Actions => Self::PullRequests,
            Self::PullRequests => Self::Actions,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct DashboardState {
    pub actions: Vec<WorkflowRunSummary>,
    pub pulls: Vec<PullRequestSummary>,
    pub detail_cache: HashMap<String, DetailView>,
    pub rate_limit: Option<RateLimitState>,
    pub errors: Vec<String>,
    pub last_refresh_at: Option<DateTime<Utc>>,
    pub next_refresh_at: Option<DateTime<Utc>>,
    pub effective_interval_secs: u64,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum DetailTarget {
    WorkflowRun { repo: RepoTarget, run_id: u64 },
    PullRequest { repo: RepoTarget, number: u64 },
}

impl DetailTarget {
    pub fn cache_key(&self) -> String {
        match self {
            Self::WorkflowRun { repo, run_id } => format!("workflow:{}#{run_id}", repo),
            Self::PullRequest { repo, number } => format!("pr:{}#{number}", repo),
        }
    }
}
