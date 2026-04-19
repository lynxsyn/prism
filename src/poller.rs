use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc::Sender};
use std::thread;
use std::time::Duration;

use chrono::Utc;

use crate::config::EffectiveConfig;
use crate::github::{EtagCache, GitHubClient, merge_rate_limits};
use crate::model::{
    DashboardState, DetailTarget, PullRequestSummary, RateLimitState, WorkflowRunSummary,
};

#[derive(Debug)]
pub enum PollerMessage {
    Update(DashboardUpdate),
}

#[derive(Debug, Default)]
pub struct DashboardUpdate {
    pub actions: Option<Vec<WorkflowRunSummary>>,
    pub pulls: Option<Vec<PullRequestSummary>>,
    pub detail: Option<Option<crate::model::WorkflowRunDetail>>,
    pub rate_limit: Option<RateLimitState>,
    pub errors: Vec<String>,
    pub fetched_at: chrono::DateTime<chrono::Utc>,
    pub effective_interval_secs: u64,
    pub had_success: bool,
}

#[derive(Debug, Default)]
pub struct PollerControl {
    pub refresh_now: AtomicBool,
    pub stop: AtomicBool,
    pub detail_target: Mutex<Option<DetailTarget>>,
}

impl PollerControl {
    pub fn request_refresh(&self) {
        self.refresh_now.store(true, Ordering::Relaxed);
    }

    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }

    pub fn set_detail_target(&self, detail_target: Option<DetailTarget>) {
        if let Ok(mut guard) = self.detail_target.lock() {
            *guard = detail_target;
        }
        self.request_refresh();
    }
}

pub fn spawn_poller(
    config: EffectiveConfig,
    client: GitHubClient,
    viewer_login: String,
    sender: Sender<PollerMessage>,
) -> Arc<PollerControl> {
    let control = Arc::new(PollerControl::default());
    let thread_control = Arc::clone(&control);

    thread::spawn(move || {
        let mut etag_cache = EtagCache::default();
        let mut last_actions: Vec<WorkflowRunSummary> = Vec::new();

        loop {
            if thread_control.stop.load(Ordering::Relaxed) {
                break;
            }

            let detail_target = thread_control
                .detail_target
                .lock()
                .ok()
                .and_then(|guard| guard.clone());
            let mut update = DashboardUpdate {
                fetched_at: Utc::now(),
                ..DashboardUpdate::default()
            };

            let mut rate_limits = Vec::new();

            let mut action_rows = Vec::new();
            let mut saw_action_success = false;
            for repo in &config.repos {
                match client.fetch_actions_runs(repo, config.actions_limit, etag_cache.get(repo)) {
                    Ok(result) => {
                        update.had_success = true;
                        saw_action_success = true;
                        etag_cache.update(repo, result.etag);
                        rate_limits.push(result.rate_limit);
                        if result.not_modified {
                            action_rows.extend(
                                last_actions
                                    .iter()
                                    .filter(|run| run.repo.slug() == repo.slug())
                                    .cloned(),
                            );
                        } else {
                            action_rows.extend(result.value);
                        }
                    }
                    Err(error) => {
                        rate_limits.push(error.rate_limit.clone());
                        update
                            .errors
                            .push(format!("actions {}: {error}", repo.slug()));
                    }
                }
            }

            action_rows.sort_by(|left, right| right.created_at.cmp(&left.created_at));
            if saw_action_success {
                last_actions = action_rows.clone();
                update.actions = Some(action_rows.clone());
            }

            let mut pulls = Vec::new();
            let mut saw_pull_success = false;
            for repo in &config.repos {
                match client.fetch_pull_requests(repo, config.prs_limit, &viewer_login) {
                    Ok(result) => {
                        update.had_success = true;
                        saw_pull_success = true;
                        rate_limits.push(result.rate_limit);
                        pulls.extend(result.value);
                    }
                    Err(error) => {
                        rate_limits.push(error.rate_limit.clone());
                        update.errors.push(format!("prs {}: {error}", repo.slug()));
                    }
                }
            }
            pulls.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
            if saw_pull_success {
                update.pulls = Some(pulls);
            }

            if let Some(target) = detail_target {
                let maybe_summary = update
                    .actions
                    .as_ref()
                    .or(Some(&action_rows))
                    .and_then(|runs| {
                        runs.iter().find(|run| {
                            run.id == target.run_id && run.repo.slug() == target.repo.slug()
                        })
                    })
                    .cloned();

                if let Some(summary) = maybe_summary {
                    match client.fetch_run_detail(&summary) {
                        Ok(result) => {
                            update.had_success = true;
                            rate_limits.push(result.rate_limit);
                            update.detail = Some(Some(result.value));
                        }
                        Err(error) => {
                            rate_limits.push(error.rate_limit.clone());
                            update.errors.push(format!(
                                "detail {}#{}: {error}",
                                target.repo.slug(),
                                target.run_id
                            ));
                        }
                    }
                } else {
                    update.detail = Some(None);
                }
            } else {
                update.detail = Some(None);
            }

            update.rate_limit = merge_rate_limits(rate_limits);
            update.effective_interval_secs =
                effective_interval(config.interval, update.rate_limit.as_ref());
            let sleep_secs = update.effective_interval_secs.max(1);

            let _ = sender.send(PollerMessage::Update(update));

            sleep_until_next_cycle(&thread_control, Duration::from_secs(sleep_secs));
        }
    });

    control
}

fn sleep_until_next_cycle(control: &PollerControl, default_interval: Duration) {
    let mut elapsed = Duration::ZERO;
    let step = Duration::from_millis(200);
    while elapsed < default_interval {
        if control.stop.load(Ordering::Relaxed) {
            return;
        }
        if control.refresh_now.swap(false, Ordering::Relaxed) {
            return;
        }
        thread::sleep(step);
        elapsed += step;
    }
}

fn effective_interval(base_secs: u64, rate_limit: Option<&RateLimitState>) -> u64 {
    if let Some(rate_limit) = rate_limit {
        if let Some(retry_after) = rate_limit.retry_after {
            return base_secs.max(retry_after);
        }
        if rate_limit.remaining == 0 {
            if let Some(reset_at) = rate_limit.reset_at {
                let wait_secs = (reset_at - Utc::now()).num_seconds().max(base_secs as i64) as u64;
                return wait_secs.max(base_secs);
            }
            return base_secs.max(60);
        }
        if rate_limit.remaining <= 100 {
            return base_secs.max(60);
        }
        if rate_limit.remaining <= 500 {
            return base_secs.max(20);
        }
    }

    base_secs
}

#[allow(dead_code)]
pub fn apply_update(state: &mut DashboardState, update: DashboardUpdate) {
    if let Some(actions) = update.actions {
        state.actions = actions;
    }
    if let Some(pulls) = update.pulls {
        state.pulls = pulls;
    }
    if let Some(detail) = update.detail {
        state.detail = detail;
    }
    if let Some(rate_limit) = update.rate_limit {
        state.rate_limit = Some(rate_limit);
    }
    state.errors = update.errors;
    if update.had_success {
        state.last_refresh_at = Some(update.fetched_at);
    }
    state.effective_interval_secs = update.effective_interval_secs;
    state.next_refresh_at =
        Some(update.fetched_at + chrono::Duration::seconds(update.effective_interval_secs as i64));
}

#[cfg(test)]
mod tests {
    use chrono::DateTime;

    use super::*;

    #[test]
    fn effective_interval_slows_under_low_quota() {
        let reset_at = DateTime::<Utc>::from_timestamp(1_700_000_000, 0);

        assert_eq!(
            effective_interval(
                10,
                Some(&RateLimitState {
                    limit: 5_000,
                    remaining: 400,
                    used: 4_600,
                    reset_at,
                    retry_after: None,
                }),
            ),
            20
        );
        assert_eq!(
            effective_interval(
                10,
                Some(&RateLimitState {
                    limit: 5_000,
                    remaining: 80,
                    used: 4_920,
                    reset_at,
                    retry_after: None,
                }),
            ),
            60
        );
    }

    #[test]
    fn effective_interval_prefers_retry_after() {
        assert_eq!(
            effective_interval(
                10,
                Some(&RateLimitState {
                    limit: 5_000,
                    remaining: 0,
                    used: 5_000,
                    reset_at: None,
                    retry_after: Some(75),
                }),
            ),
            75
        );
    }

    #[test]
    fn apply_update_only_advances_refresh_time_on_success() {
        let fetched_at = DateTime::<Utc>::from_timestamp(1_700_000_000, 0).unwrap();
        let previous_refresh = DateTime::<Utc>::from_timestamp(1_699_999_900, 0).unwrap();
        let mut state = DashboardState {
            last_refresh_at: Some(previous_refresh),
            ..DashboardState::default()
        };

        apply_update(
            &mut state,
            DashboardUpdate {
                fetched_at,
                effective_interval_secs: 10,
                had_success: false,
                ..DashboardUpdate::default()
            },
        );
        assert_eq!(state.last_refresh_at, Some(previous_refresh));

        apply_update(
            &mut state,
            DashboardUpdate {
                fetched_at,
                effective_interval_secs: 10,
                had_success: true,
                ..DashboardUpdate::default()
            },
        );
        assert_eq!(state.last_refresh_at, Some(fetched_at));
    }
}
