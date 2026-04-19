use std::collections::HashMap;
use std::fmt;
use std::time::Duration;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use reqwest::StatusCode;
use reqwest::blocking::{Client, Response};
use reqwest::header::{
    ACCEPT, AUTHORIZATION, ETAG, HeaderMap, HeaderValue, IF_NONE_MATCH, USER_AGENT,
};
use serde::Deserialize;
use serde_json::json;

use crate::model::{
    PullRequestCheckSummary, PullRequestDetail, PullRequestSummary, RateLimitState, RepoTarget,
    WorkflowJobSummary, WorkflowRunDetail, WorkflowRunSummary,
};

const GRAPHQL_PR_QUERY: &str = r#"
query($owner:String!, $name:String!, $limit:Int!) {
  repository(owner:$owner, name:$name) {
    pullRequests(first:$limit, states:OPEN, orderBy:{field:UPDATED_AT,direction:DESC}) {
      nodes {
        number
        title
        isDraft
        updatedAt
        url
        reviewDecision
        author { login }
        reviewRequests(first:20) {
          nodes {
            requestedReviewer {
              __typename
              ... on User { login }
              ... on Team { slug name }
            }
          }
        }
        statusCheckRollup {
          state
        }
      }
    }
  }
}
"#;

const GRAPHQL_PR_DETAIL_QUERY: &str = r#"
query($owner:String!, $name:String!, $number:Int!) {
  repository(owner:$owner, name:$name) {
    pullRequest(number:$number) {
      number
      title
      isDraft
      updatedAt
      url
      reviewDecision
      author { login }
      reviewRequests(first:20) {
        nodes {
          requestedReviewer {
            __typename
            ... on User { login }
            ... on Team { slug name }
          }
        }
      }
      statusCheckRollup {
        state
        contexts(first:50) {
          nodes {
            __typename
            ... on CheckRun {
              name
              status
              conclusion
              detailsUrl
              startedAt
              completedAt
              checkSuite {
                workflowRun {
                  workflow { name }
                }
              }
            }
            ... on StatusContext {
              context
              state
              targetUrl
              createdAt
            }
          }
        }
      }
    }
  }
}
"#;

#[derive(Debug, Clone)]
pub struct FetchResult<T> {
    pub value: T,
    pub rate_limit: Option<RateLimitState>,
    pub etag: Option<String>,
    pub not_modified: bool,
}

#[derive(Debug, Clone)]
pub struct GitHubRequestError {
    pub message: String,
    pub rate_limit: Option<RateLimitState>,
}

impl GitHubRequestError {
    fn new(message: impl Into<String>, rate_limit: Option<RateLimitState>) -> Self {
        Self {
            message: message.into(),
            rate_limit,
        }
    }
}

impl fmt::Display for GitHubRequestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for GitHubRequestError {}

type GitHubRequestResult<T> = std::result::Result<T, GitHubRequestError>;

#[derive(Debug, Clone)]
pub struct GitHubClient {
    http: Client,
    host: String,
    api_base: String,
}

impl GitHubClient {
    pub fn new(host: &str, token: &str) -> Result<Self> {
        let api_base = if host == "github.com" {
            "https://api.github.com".to_string()
        } else {
            format!("https://{host}/api/v3")
        };

        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, HeaderValue::from_static("prism-cli/0.1.0"));
        headers.insert(
            ACCEPT,
            HeaderValue::from_static("application/vnd.github+json"),
        );
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {token}"))
                .context("failed to build auth header")?,
        );

        let http = Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_secs(20))
            .build()
            .context("failed to build HTTP client")?;

        Ok(Self {
            http,
            host: host.to_string(),
            api_base,
        })
    }

    pub fn viewer_login(&self) -> Result<String> {
        #[derive(Debug, Deserialize)]
        struct ViewerResponse {
            login: String,
        }

        let response = self
            .http
            .get(self.api_url("user"))
            .send()
            .context("failed to fetch current GitHub user")?;
        ensure_success(
            &response,
            "fetch GitHub user",
            parse_rate_limit(response.headers()),
        )
        .map_err(anyhow::Error::from)?;
        let parsed: ViewerResponse = response.json().context("failed to parse /user")?;
        Ok(parsed.login)
    }

    pub fn fetch_actions_runs(
        &self,
        repo: &RepoTarget,
        limit: usize,
        etag: Option<&str>,
    ) -> GitHubRequestResult<FetchResult<Vec<WorkflowRunSummary>>> {
        #[derive(Debug, Deserialize)]
        struct RunsResponse {
            workflow_runs: Vec<RestWorkflowRun>,
        }

        let url = format!(
            "{}/repos/{}/{}/actions/runs?per_page={limit}",
            self.api_base, repo.owner, repo.name
        );
        let mut request = self.http.get(url);
        if let Some(tag) = etag {
            request = request.header(IF_NONE_MATCH, tag);
        }

        let response = request.send().map_err(|error| {
            GitHubRequestError::new(format!("failed to fetch workflow runs: {error}"), None)
        })?;
        let rate_limit = parse_rate_limit(response.headers());
        let next_etag = header_to_string(response.headers(), ETAG);
        if response.status() == StatusCode::NOT_MODIFIED {
            return Ok(FetchResult {
                value: Vec::new(),
                rate_limit,
                etag: next_etag,
                not_modified: true,
            });
        }

        ensure_success(&response, "fetch workflow runs", rate_limit.clone())?;
        let payload: RunsResponse = response.json().map_err(|error| {
            GitHubRequestError::new(
                format!("failed to parse workflow runs: {error}"),
                rate_limit.clone(),
            )
        })?;
        let runs = payload
            .workflow_runs
            .into_iter()
            .map(|run| WorkflowRunSummary {
                repo: repo.clone(),
                id: run.id,
                workflow_name: run.name.unwrap_or_else(|| "workflow".to_string()),
                title: run.display_title.unwrap_or_else(|| "workflow".to_string()),
                branch: run.head_branch.unwrap_or_else(|| "-".to_string()),
                event: run.event.unwrap_or_else(|| "-".to_string()),
                status: run.status,
                conclusion: run.conclusion,
                created_at: run.created_at,
                started_at: run.run_started_at,
                updated_at: run.updated_at,
                url: run.html_url,
            })
            .collect();

        Ok(FetchResult {
            value: runs,
            rate_limit,
            etag: next_etag,
            not_modified: false,
        })
    }

    pub fn fetch_run_detail(
        &self,
        summary: &WorkflowRunSummary,
    ) -> GitHubRequestResult<FetchResult<WorkflowRunDetail>> {
        #[derive(Debug, Deserialize)]
        struct JobsResponse {
            jobs: Vec<RestWorkflowJob>,
        }

        let url = format!(
            "{}/repos/{}/{}/actions/runs/{}/jobs?per_page=100",
            self.api_base, summary.repo.owner, summary.repo.name, summary.id
        );
        let response = self.http.get(url).send().map_err(|error| {
            GitHubRequestError::new(format!("failed to fetch workflow jobs: {error}"), None)
        })?;
        let rate_limit = parse_rate_limit(response.headers());
        ensure_success(&response, "fetch workflow jobs", rate_limit.clone())?;
        let payload: JobsResponse = response.json().map_err(|error| {
            GitHubRequestError::new(
                format!("failed to parse workflow jobs: {error}"),
                rate_limit.clone(),
            )
        })?;

        let mut jobs = payload
            .jobs
            .into_iter()
            .map(map_job_summary)
            .collect::<Vec<_>>();

        jobs.sort_by_key(job_sort_key);
        let total_jobs = jobs.len();
        let completed_jobs = jobs.iter().filter(|job| job.status == "completed").count();
        let failed_jobs = jobs
            .iter()
            .filter(|job| {
                matches!(
                    job.conclusion.as_deref(),
                    Some("failure" | "timed_out" | "cancelled")
                )
            })
            .count();
        let running_jobs = jobs.iter().filter(|job| job.status != "completed").count();

        Ok(FetchResult {
            value: WorkflowRunDetail {
                summary: summary.clone(),
                jobs,
                total_jobs,
                completed_jobs,
                failed_jobs,
                running_jobs,
            },
            rate_limit,
            etag: None,
            not_modified: false,
        })
    }

    pub fn fetch_pull_requests(
        &self,
        repo: &RepoTarget,
        limit: usize,
        viewer_login: &str,
    ) -> GitHubRequestResult<FetchResult<Vec<PullRequestSummary>>> {
        #[derive(Debug, Deserialize)]
        struct GraphQLResponse {
            data: Option<GraphQLData>,
            errors: Option<Vec<GraphQLError>>,
        }

        #[derive(Debug, Deserialize)]
        struct GraphQLError {
            message: String,
        }

        #[derive(Debug, Deserialize)]
        struct GraphQLData {
            repository: Option<GraphQLRepository>,
        }

        #[derive(Debug, Deserialize)]
        struct GraphQLRepository {
            #[serde(rename = "pullRequests")]
            pull_requests: GraphQLPullRequestConnection,
        }

        #[derive(Debug, Deserialize)]
        struct GraphQLPullRequestConnection {
            nodes: Vec<GraphQLPullRequest>,
        }

        #[derive(Debug, Deserialize)]
        struct GraphQLPullRequest {
            number: u64,
            title: String,
            #[serde(rename = "isDraft")]
            is_draft: bool,
            #[serde(rename = "updatedAt")]
            updated_at: DateTime<Utc>,
            url: String,
            #[serde(rename = "reviewDecision")]
            review_decision: Option<String>,
            author: Option<GraphQLActor>,
            #[serde(rename = "reviewRequests")]
            review_requests: GraphQLReviewRequestConnection,
            #[serde(rename = "statusCheckRollup")]
            status_check_rollup: Option<GraphQLStatusCheckRollup>,
        }

        #[derive(Debug, Deserialize)]
        struct GraphQLActor {
            login: String,
        }

        #[derive(Debug, Deserialize)]
        struct GraphQLReviewRequestConnection {
            nodes: Vec<GraphQLReviewRequest>,
        }

        #[derive(Debug, Deserialize)]
        struct GraphQLReviewRequest {
            #[serde(rename = "requestedReviewer")]
            requested_reviewer: Option<GraphQLRequestedReviewer>,
        }

        #[derive(Debug, Deserialize)]
        struct GraphQLRequestedReviewer {
            #[serde(rename = "__typename")]
            kind: String,
            login: Option<String>,
        }

        #[derive(Debug, Deserialize)]
        struct GraphQLStatusCheckRollup {
            state: Option<String>,
        }

        let payload = json!({
            "query": GRAPHQL_PR_QUERY,
            "variables": {
                "owner": repo.owner,
                "name": repo.name,
                "limit": limit as i64,
            }
        });

        let response = self
            .http
            .post(self.graphql_url())
            .json(&payload)
            .send()
            .map_err(|error| {
                GitHubRequestError::new(format!("failed to fetch pull requests: {error}"), None)
            })?;
        let rate_limit = parse_rate_limit(response.headers());
        ensure_success(&response, "fetch pull requests", rate_limit.clone())?;
        let parsed: GraphQLResponse = response.json().map_err(|error| {
            GitHubRequestError::new(
                format!("failed to parse GraphQL response: {error}"),
                rate_limit.clone(),
            )
        })?;

        if let Some(errors) = parsed.errors {
            let joined = errors
                .into_iter()
                .map(|error| error.message)
                .collect::<Vec<_>>()
                .join("; ");
            return Err(GitHubRequestError::new(
                format!("GitHub GraphQL error: {joined}"),
                rate_limit,
            ));
        }

        let repo_payload = parsed
            .data
            .and_then(|data| data.repository)
            .ok_or_else(|| {
                GitHubRequestError::new(
                    "repository missing from GraphQL response",
                    rate_limit.clone(),
                )
            })?;

        let pulls = repo_payload
            .pull_requests
            .nodes
            .into_iter()
            .map(|pr| {
                let review_requested_for_viewer =
                    pr.review_requests.nodes.into_iter().any(|node| {
                        node.requested_reviewer.as_ref().is_some_and(|reviewer| {
                            match reviewer.kind.as_str() {
                                "User" => reviewer
                                    .login
                                    .as_ref()
                                    .is_some_and(|login| login.eq_ignore_ascii_case(viewer_login)),
                                "Team" => false,
                                _ => false,
                            }
                        })
                    });

                PullRequestSummary {
                    repo: repo.clone(),
                    number: pr.number,
                    title: pr.title,
                    author: pr
                        .author
                        .map(|author| author.login)
                        .unwrap_or_else(|| "ghost".to_string()),
                    is_draft: pr.is_draft,
                    review_decision: pr.review_decision,
                    review_requested_for_viewer,
                    ci_rollup: pr.status_check_rollup.and_then(|rollup| rollup.state),
                    updated_at: pr.updated_at,
                    url: pr.url,
                }
            })
            .collect::<Vec<_>>();

        Ok(FetchResult {
            value: pulls,
            rate_limit,
            etag: None,
            not_modified: false,
        })
    }

    pub fn fetch_pull_request_detail(
        &self,
        repo: &RepoTarget,
        number: u64,
        viewer_login: &str,
    ) -> GitHubRequestResult<FetchResult<PullRequestDetail>> {
        #[derive(Debug, Deserialize)]
        struct GraphQLResponse {
            data: Option<GraphQLData>,
            errors: Option<Vec<GraphQLError>>,
        }

        #[derive(Debug, Deserialize)]
        struct GraphQLError {
            message: String,
        }

        #[derive(Debug, Deserialize)]
        struct GraphQLData {
            repository: Option<GraphQLRepository>,
        }

        #[derive(Debug, Deserialize)]
        struct GraphQLRepository {
            #[serde(rename = "pullRequest")]
            pull_request: Option<GraphQLPullRequest>,
        }

        #[derive(Debug, Deserialize)]
        struct GraphQLPullRequest {
            number: u64,
            title: String,
            #[serde(rename = "isDraft")]
            is_draft: bool,
            #[serde(rename = "updatedAt")]
            updated_at: DateTime<Utc>,
            url: String,
            #[serde(rename = "reviewDecision")]
            review_decision: Option<String>,
            author: Option<GraphQLActor>,
            #[serde(rename = "reviewRequests")]
            review_requests: GraphQLReviewRequestConnection,
            #[serde(rename = "statusCheckRollup")]
            status_check_rollup: Option<GraphQLStatusCheckRollup>,
        }

        #[derive(Debug, Deserialize)]
        struct GraphQLActor {
            login: String,
        }

        #[derive(Debug, Deserialize)]
        struct GraphQLReviewRequestConnection {
            nodes: Vec<GraphQLReviewRequest>,
        }

        #[derive(Debug, Deserialize)]
        struct GraphQLReviewRequest {
            #[serde(rename = "requestedReviewer")]
            requested_reviewer: Option<GraphQLRequestedReviewer>,
        }

        #[derive(Debug, Deserialize)]
        struct GraphQLRequestedReviewer {
            #[serde(rename = "__typename")]
            kind: String,
            login: Option<String>,
        }

        #[derive(Debug, Deserialize)]
        struct GraphQLStatusCheckRollup {
            state: Option<String>,
            contexts: GraphQLCheckContextConnection,
        }

        #[derive(Debug, Deserialize)]
        struct GraphQLCheckContextConnection {
            nodes: Vec<GraphQLCheckNode>,
        }

        #[derive(Debug, Deserialize)]
        struct GraphQLCheckNode {
            #[serde(rename = "__typename")]
            kind: String,
            name: Option<String>,
            status: Option<String>,
            conclusion: Option<String>,
            #[serde(rename = "detailsUrl")]
            details_url: Option<String>,
            #[serde(rename = "startedAt")]
            started_at: Option<DateTime<Utc>>,
            #[serde(rename = "completedAt")]
            completed_at: Option<DateTime<Utc>>,
            #[serde(rename = "checkSuite")]
            check_suite: Option<GraphQLCheckSuite>,
            context: Option<String>,
            state: Option<String>,
            #[serde(rename = "targetUrl")]
            target_url: Option<String>,
            #[serde(rename = "createdAt")]
            created_at: Option<DateTime<Utc>>,
        }

        #[derive(Debug, Deserialize)]
        struct GraphQLCheckSuite {
            #[serde(rename = "workflowRun")]
            workflow_run: Option<GraphQLWorkflowRun>,
        }

        #[derive(Debug, Deserialize)]
        struct GraphQLWorkflowRun {
            workflow: Option<GraphQLWorkflow>,
        }

        #[derive(Debug, Deserialize)]
        struct GraphQLWorkflow {
            name: Option<String>,
        }

        let payload = json!({
            "query": GRAPHQL_PR_DETAIL_QUERY,
            "variables": {
                "owner": repo.owner,
                "name": repo.name,
                "number": number as i64,
            }
        });

        let response = self
            .http
            .post(self.graphql_url())
            .json(&payload)
            .send()
            .map_err(|error| {
                GitHubRequestError::new(
                    format!("failed to fetch pull request detail: {error}"),
                    None,
                )
            })?;
        let rate_limit = parse_rate_limit(response.headers());
        ensure_success(&response, "fetch pull request detail", rate_limit.clone())?;
        let parsed: GraphQLResponse = response.json().map_err(|error| {
            GitHubRequestError::new(
                format!("failed to parse pull request detail: {error}"),
                rate_limit.clone(),
            )
        })?;

        if let Some(errors) = parsed.errors {
            let joined = errors
                .into_iter()
                .map(|error| error.message)
                .collect::<Vec<_>>()
                .join("; ");
            return Err(GitHubRequestError::new(
                format!("GitHub GraphQL error: {joined}"),
                rate_limit,
            ));
        }

        let pr = parsed
            .data
            .and_then(|data| data.repository)
            .and_then(|repo| repo.pull_request)
            .ok_or_else(|| {
                GitHubRequestError::new(
                    "pull request missing from GraphQL response",
                    rate_limit.clone(),
                )
            })?;

        let review_requested_for_viewer = pr.review_requests.nodes.into_iter().any(|node| {
            node.requested_reviewer
                .as_ref()
                .is_some_and(|reviewer| match reviewer.kind.as_str() {
                    "User" => reviewer
                        .login
                        .as_ref()
                        .is_some_and(|login| login.eq_ignore_ascii_case(viewer_login)),
                    "Team" => false,
                    _ => false,
                })
        });

        let ci_rollup = pr
            .status_check_rollup
            .as_ref()
            .and_then(|rollup| rollup.state.clone());

        let summary = PullRequestSummary {
            repo: repo.clone(),
            number: pr.number,
            title: pr.title,
            author: pr
                .author
                .map(|author| author.login)
                .unwrap_or_else(|| "ghost".to_string()),
            is_draft: pr.is_draft,
            review_decision: pr.review_decision,
            review_requested_for_viewer,
            ci_rollup,
            updated_at: pr.updated_at,
            url: pr.url,
        };

        let mut checks = pr
            .status_check_rollup
            .map(|rollup| {
                rollup
                    .contexts
                    .nodes
                    .into_iter()
                    .map(|node| match node.kind.as_str() {
                        "CheckRun" => PullRequestCheckSummary {
                            name: node.name.unwrap_or_else(|| "check".to_string()),
                            workflow_name: node
                                .check_suite
                                .and_then(|suite| suite.workflow_run)
                                .and_then(|run| run.workflow)
                                .and_then(|workflow| workflow.name),
                            status: node.status.unwrap_or_else(|| "PENDING".to_string()),
                            conclusion: node.conclusion,
                            started_at: node.started_at,
                            completed_at: node.completed_at,
                            url: node.details_url,
                        },
                        _ => {
                            let state = node.state.unwrap_or_else(|| "PENDING".to_string());
                            let conclusion = match state.as_str() {
                                "SUCCESS" => Some("SUCCESS".to_string()),
                                "FAILURE" | "ERROR" => Some("FAILURE".to_string()),
                                "SKIPPED" | "NEUTRAL" => Some(state.clone()),
                                _ => None,
                            };
                            let started_at = node.created_at;
                            PullRequestCheckSummary {
                                name: node.context.unwrap_or_else(|| "status".to_string()),
                                workflow_name: None,
                                status: if conclusion.is_some() {
                                    "COMPLETED".to_string()
                                } else {
                                    state.clone()
                                },
                                conclusion,
                                started_at,
                                completed_at: if matches!(
                                    state.as_str(),
                                    "SUCCESS" | "FAILURE" | "ERROR" | "SKIPPED" | "NEUTRAL"
                                ) {
                                    started_at
                                } else {
                                    None
                                },
                                url: node.target_url,
                            }
                        }
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        checks.sort_by_key(check_sort_key);

        let total_checks = checks.len();
        let completed_checks = checks
            .iter()
            .filter(|check| check_status_bucket(check).0)
            .count();
        let passing_checks = checks
            .iter()
            .filter(|check| {
                matches!(
                    check.conclusion.as_deref(),
                    Some("SUCCESS" | "SKIPPED" | "NEUTRAL")
                )
            })
            .count();
        let failing_checks = checks
            .iter()
            .filter(|check| {
                matches!(
                    check.conclusion.as_deref(),
                    Some("FAILURE" | "ERROR" | "CANCELLED" | "TIMED_OUT" | "ACTION_REQUIRED")
                )
            })
            .count();
        let running_checks = checks
            .iter()
            .filter(|check| matches!(check.status.as_str(), "IN_PROGRESS" | "RUNNING"))
            .count();
        let pending_checks = checks
            .iter()
            .filter(|check| {
                matches!(
                    check.status.as_str(),
                    "PENDING" | "QUEUED" | "EXPECTED" | "WAITING" | "REQUESTED"
                )
            })
            .count();

        Ok(FetchResult {
            value: PullRequestDetail {
                summary,
                checks,
                total_checks,
                completed_checks,
                passing_checks,
                failing_checks,
                running_checks,
                pending_checks,
            },
            rate_limit,
            etag: None,
            not_modified: false,
        })
    }

    fn api_url(&self, path: &str) -> String {
        format!("{}/{}", self.api_base, path.trim_start_matches('/'))
    }

    fn graphql_url(&self) -> String {
        if self.host == "github.com" {
            "https://api.github.com/graphql".to_string()
        } else {
            format!("https://{}/api/graphql", self.host)
        }
    }
}

fn ensure_success(
    response: &Response,
    action: &str,
    rate_limit: Option<RateLimitState>,
) -> GitHubRequestResult<()> {
    if response.status().is_success() {
        return Ok(());
    }

    let status = response.status();
    let mut message = format!("{action} failed with HTTP {}", status.as_u16());
    if let Some(rate_limit) = &rate_limit {
        if let Some(retry_after) = rate_limit.retry_after {
            message.push_str(&format!("; retry after {retry_after}s"));
        } else if rate_limit.remaining == 0
            && let Some(reset_at) = rate_limit.reset_at
        {
            message.push_str(&format!(
                "; rate limit resets at {}",
                reset_at.format("%H:%M:%S")
            ));
        }
    }
    Err(GitHubRequestError::new(message, rate_limit))
}

fn header_to_string(headers: &HeaderMap, name: reqwest::header::HeaderName) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(ToString::to_string)
}

fn parse_rate_limit(headers: &HeaderMap) -> Option<RateLimitState> {
    let limit = headers
        .get("x-ratelimit-limit")?
        .to_str()
        .ok()?
        .parse()
        .ok()?;
    let remaining = headers
        .get("x-ratelimit-remaining")?
        .to_str()
        .ok()?
        .parse()
        .ok()?;
    let used = headers
        .get("x-ratelimit-used")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    let reset_at = headers
        .get("x-ratelimit-reset")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<i64>().ok())
        .and_then(|value| DateTime::<Utc>::from_timestamp(value, 0));
    let retry_after = headers
        .get("retry-after")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok());

    Some(RateLimitState {
        limit,
        remaining,
        used,
        reset_at,
        retry_after,
    })
}

fn map_job_summary(job: RestWorkflowJob) -> WorkflowJobSummary {
    let total_steps = job.steps.len();
    let completed_steps = job
        .steps
        .iter()
        .filter(|step| step.status == "completed")
        .count();
    let failed_step_name = job
        .steps
        .iter()
        .find(|step| {
            matches!(
                step.conclusion.as_deref(),
                Some("failure" | "cancelled" | "timed_out")
            )
        })
        .map(|step| step.name.clone());

    let has_ambiguous_steps = job.steps.iter().any(|step| step.name.starts_with("Post "));
    let indeterminate_progress =
        job.status != "completed" && (has_ambiguous_steps || total_steps == 0);

    WorkflowJobSummary {
        name: job.name,
        status: job.status,
        conclusion: job.conclusion,
        started_at: job.started_at,
        completed_at: job.completed_at,
        total_steps,
        completed_steps,
        failed_step_name,
        indeterminate_progress,
    }
}

fn check_sort_key(check: &PullRequestCheckSummary) -> (u8, String) {
    let rank = match check_status_bucket(check) {
        (false, _, true) => 0,
        (false, true, _) => 1,
        (true, _, true) => 2,
        _ => 3,
    };
    (rank, check.name.clone())
}

fn check_status_bucket(check: &PullRequestCheckSummary) -> (bool, bool, bool) {
    let completed = check.status == "COMPLETED";
    let running = matches!(check.status.as_str(), "IN_PROGRESS" | "RUNNING");
    let failed = matches!(
        check.conclusion.as_deref(),
        Some("FAILURE" | "ERROR" | "CANCELLED" | "TIMED_OUT" | "ACTION_REQUIRED")
    );
    (completed, running, failed)
}

fn job_sort_key(job: &WorkflowJobSummary) -> (u8, String) {
    let rank = match job.conclusion.as_deref() {
        Some("failure" | "timed_out" | "cancelled") => 0,
        _ if job.status != "completed" => 1,
        Some("success") => 3,
        _ => 2,
    };
    (rank, job.name.clone())
}

#[derive(Debug, Deserialize)]
struct RestWorkflowRun {
    id: u64,
    name: Option<String>,
    display_title: Option<String>,
    head_branch: Option<String>,
    status: String,
    conclusion: Option<String>,
    created_at: DateTime<Utc>,
    run_started_at: Option<DateTime<Utc>>,
    updated_at: Option<DateTime<Utc>>,
    html_url: String,
    event: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RestWorkflowJob {
    name: String,
    status: String,
    conclusion: Option<String>,
    started_at: Option<DateTime<Utc>>,
    completed_at: Option<DateTime<Utc>>,
    #[serde(default)]
    steps: Vec<RestWorkflowStep>,
}

#[derive(Debug, Deserialize)]
struct RestWorkflowStep {
    name: String,
    status: String,
    conclusion: Option<String>,
}

pub fn merge_rate_limits(
    values: impl IntoIterator<Item = Option<RateLimitState>>,
) -> Option<RateLimitState> {
    let mut merged: Option<RateLimitState> = None;
    for value in values.into_iter().flatten() {
        match &mut merged {
            Some(existing) if value.remaining < existing.remaining => *existing = value,
            None => merged = Some(value),
            _ => {}
        }
    }
    merged
}

#[derive(Default)]
pub struct EtagCache {
    tags: HashMap<String, String>,
}

impl EtagCache {
    pub fn get(&self, repo: &RepoTarget) -> Option<&str> {
        self.tags.get(&repo.slug()).map(String::as_str)
    }

    pub fn update(&mut self, repo: &RepoTarget, etag: Option<String>) {
        if let Some(tag) = etag {
            self.tags.insert(repo.slug(), tag);
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::DateTime;

    use super::*;

    #[test]
    fn merge_rate_limits_uses_lowest_remaining_budget() {
        let reset_at = DateTime::<Utc>::from_timestamp(1_700_000_000, 0);
        let merged = merge_rate_limits([
            Some(RateLimitState {
                limit: 5_000,
                remaining: 240,
                used: 4_760,
                reset_at,
                retry_after: None,
            }),
            Some(RateLimitState {
                limit: 5_000,
                remaining: 42,
                used: 4_958,
                reset_at,
                retry_after: Some(12),
            }),
        ])
        .unwrap();

        assert_eq!(merged.remaining, 42);
        assert_eq!(merged.retry_after, Some(12));
    }
}
