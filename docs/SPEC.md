# Prism Specification

Related docs:

- [README.md](../README.md) for the user-facing overview
- [USER_GUIDE.md](./USER_GUIDE.md) for operator workflow and usage
- [DEVELOPMENT.md](./DEVELOPMENT.md) for contributor guardrails
- [EXECUTION_PLAN.md](./EXECUTION_PLAN.md) for implementation sequencing

## 1. Overview

Prism is a fast, keyboard-driven terminal dashboard for monitoring GitHub pull requests and GitHub Actions across one or more repositories.

Primary goals:

- give an at-a-glance operational view of repos that matter right now
- make PR review and CI triage faster than switching between browser tabs
- stay reliable under narrow terminal widths and long-running sessions
- minimize API usage through adaptive polling, request consolidation, and caching
- look clean and compact in a modern developer terminal without visual noise

Prism is a pure CLI/TUI application. It is not a web app, daemon, or browser companion.

## 2. Product Positioning

Prism sits between one-shot `gh` commands and a full browser workflow.

`gh` already solves:

- ad hoc querying
- single-run watching
- single-PR inspection
- auth bootstrapping

Prism should solve:

- continuous multi-repo situational awareness
- keyboard-driven triage in one screen
- stable live refresh without terminal flicker
- actionable status compression across PRs and workflow runs
- drill-down from workflow runs into jobs and step-level progress

Prism should not try to replace every `gh` feature. It should be the fast dashboard layer on top of GitHub data.

## 3. Goals

### 3.1 Must-have

- monitor multiple repositories at once
- show open PR state with review and CI rollups
- show recent workflow runs with live elapsed time for in-progress runs
- refresh automatically with user-configurable polling
- support a narrow-width mode that remains usable in split panes
- open the selected PR or workflow run in the browser
- expose rate-limit state and degrade gracefully under pressure
- work on macOS first, with Linux as a near-term target

### 3.2 Nice-to-have

- org-wide repo discovery
- saved repo groups and named views
- filtering by author, review state, branch, label, or CI state
- desktop notifications for important transitions
- configurable keybindings
- GitHub Enterprise hostname support

## 4. Non-goals for v1

- mutating GitHub state from the TUI
- editing PR metadata
- reviewing diffs inline
- webhook server or push-based syncing
- issue tracking
- comment timeline and activity feed
- Windows support

The activity pane from the earlier prompt is intentionally out of v1. It adds API complexity and scroll-state complexity before the core dashboard proves itself.

## 5. Target Users

### 5.1 Individual maintainer

Needs to watch a small set of personal or client repos and quickly see:

- which PRs need review
- which PRs are blocked by CI
- which workflows are currently running or failing

### 5.2 Staff engineer / tech lead

Needs a live operational view of several active repos during working hours without burning API quota or living in the browser.

### 5.3 Reviewer

Needs fast visibility into PRs where they are requested, especially when checks are pending or have just failed.

## 6. User Stories

- As a reviewer, I want PRs requesting my review highlighted so I can spot them immediately.
- As an engineer, I want active workflow runs to show live elapsed time so I can tell what is stuck versus normal.
- As a maintainer, I want a compact mode that fits in a split terminal.
- As a user, I want to hit one key to refresh immediately without waiting for the poll interval.
- As a user, I want to open the selected PR or run in the browser from the terminal.
- As a user, I want the app to slow down automatically instead of hammering the API when the rate limit gets tight.

## 7. UX Scope

## 7.1 Modes

Prism v1 ships with two layout modes:

### `compact`

- single column
- repo sections stacked vertically
- lowest information density that still preserves utility
- intended for narrow panes around 40-70 columns

### `split`

- left pane: workflow runs
- right pane: pull requests
- default mode for most users

`full` mode is deferred until the core interaction model is stable.

## 7.1.1 Visual design direction

Prism should visually behave more like a disciplined terminal status dashboard than a colorful widget demo.

Rules:

- no emoji
- minimal iconography only, using terminal-safe glyphs or ASCII fallbacks
- dense but readable spacing
- avoid heavy borders when lighter separators work
- prefer shape, contrast, and alignment over decoration
- inherit the terminal color theme where possible instead of imposing a fixed palette

The target feel is closer to a clean agent/status UI than a novelty TUI.

## 7.2 Primary views

### Actions view

Columns:

- Repo
- Workflow
- Branch
- Status
- Duration
- Triggered

Rules:

- default limit: last 10 runs per repo
- sort newest first
- in-progress rows use local elapsed timers based on `started_at`
- queued rows show queued duration when available, else age since creation
- selected runs can be drilled into without leaving Prism

### Actions drill-down view

Selecting an Actions run and opening details should show a focused run view with:

- workflow name, repo, branch, event, run number, trigger age
- overall run state and elapsed duration
- jobs list with current state
- job progress bars based on completed vs total steps where available
- failed jobs grouped first
- running jobs showing live elapsed time
- most recent failed step name when present

This is a terminal detail view, not a log viewer replacement. It should answer:

- what is running
- what already passed
- what failed
- roughly how far through the pipeline the run is

### Pull requests view

Columns:

- Repo
- #
- Title
- Author
- Review
- CI
- Updated

Rules:

- open PRs only in v1
- title truncates to fit available width
- review state derived from GitHub review metadata
- CI state is an aggregated rollup, not a list of every individual check
- requested-reviewer rows are highlighted

## 7.3 Keybindings

Mandatory:

- `q`: quit
- `r`: force refresh
- `Tab`: switch focus between panes
- `o` or `Enter`: open selected item in browser
- `l`: open local detail/drill-down for selected item
- `Esc`: close detail/help overlay
- `j` / `Down`: move selection down
- `k` / `Up`: move selection up
- `g`: jump to top of focused pane
- `G`: jump to bottom of focused pane
- `?`: help overlay

Optional for v1.1:

- `/`: filter prompt
- `1` / `2`: switch mode

## 7.4 Status bar

Bottom status bar always shows:

- current mode
- last successful refresh time
- next refresh countdown
- GitHub rate limit remaining
- current hostname
- degraded / paused state if polling is slowed or suspended
- focused item hint and drill-down hint when relevant

## 8. CLI Interface

Proposed top-level usage:

```text
prism [repo...] [flags]
prism auth status
prism config init
prism repos add owner/repo
prism repos list
```

### 8.1 Main flags

- `-r, --repo <OWNER/REPO>` repeatable
- `-f, --config <PATH>`
- `-i, --interval <SECONDS>` default `10`
- `-m, --mode <compact|split>` default `split`
- `--host <HOST>` default `github.com`
- `--actions-limit <N>` default `10`
- `--prs-limit <N>` default `30`
- `--open-command <CMD>`
- `--log-level <error|warn|info|debug|trace>`
- `--no-color`

### 8.2 Config precedence

1. CLI args
2. environment
3. config file
4. defaults

Environment variables:

- `PRISM_TOKEN`
- `PRISM_HOST`
- `PRISM_INTERVAL`
- `BROWSER`

## 9. Authentication

Prism should support three auth sources in this order:

1. `PRISM_TOKEN`
2. config file token reference or secret-store lookup
3. `gh auth token` fallback

Rationale:

- env var is simple and automation-friendly
- `gh` fallback avoids duplicate login friction
- direct token support keeps Prism independent from `gh`

### 9.1 Token guidance

Preferred:

- fine-grained PAT scoped to the target repos with read-only permissions where possible

Minimum expected access:

- repository metadata read
- pull requests read
- actions read

Classic PATs should still work, but docs should push users toward fine-grained tokens unless a specific endpoint forces otherwise.

### 9.2 Host support

v1 should support:

- GitHub.com
- GitHub Enterprise Server via `--host`

## 10. API Strategy

Prism should use a hybrid API approach.

### 10.1 REST for workflow runs

Use REST for Actions because the workflow run endpoints are direct and stable.

Primary endpoint family:

- `GET /repos/{owner}/{repo}/actions/runs`
- `GET /repos/{owner}/{repo}/actions/runs/{run_id}/jobs`

Why REST here:

- direct fit for workflow runs
- simple pagination and filtering
- easy conditional requests with ETags

### 10.2 GraphQL for pull request dashboard data

Use GraphQL for PRs so one query can return:

- PR number
- title
- author
- updated time
- draft/state
- review decision
- review requests
- CI rollup
- URL

Why GraphQL here:

- avoids N+1 REST calls for review and check state
- better fit for dashboard-shaped data
- lower request count for multi-repo polling

### 10.3 Do not shell out to `gh` for normal data fetches

Prism should not depend on `gh api` subprocesses for its core data plane.

Reasons:

- more brittle error handling
- harder to test
- harder to parse headers for rate limits and ETags
- higher per-refresh overhead

`gh` should only be used as an optional auth bootstrap and maybe for future import conveniences.

## 11. Rate-limit and Polling Strategy

This is a first-class design constraint.

### 11.1 Core rules

- serialize or tightly cap concurrent network requests
- prefer one GraphQL query batch for PRs over many REST requests
- use ETags / `If-None-Match` on REST endpoints where possible
- maintain local timers for running jobs rather than polling every second
- derive refresh countdown locally
- slow down automatically under low quota
- poll selected run details separately at a bounded interval only while the detail view is open

### 11.2 Polling tiers

Default interval is `10s`, but effective interval is adaptive:

- normal: user interval
- low quota: max(user interval, 20s)
- very low quota: max(user interval, 60s)
- rate-limited: pause until `Retry-After` or reset time, then resume with backoff

### 11.3 Backoff

On transient failure:

- network timeout: exponential backoff with jitter, cap 60s
- primary rate limit: sleep until reset
- secondary rate limit: respect `Retry-After`; otherwise wait at least 60s, then exponential backoff

### 11.4 Caching

Per endpoint / query family cache:

- last payload
- last success timestamp
- ETag if present
- last error
- rate-limit headers

If a refresh fails, Prism keeps rendering the last successful snapshot and marks the pane stale.

## 12. Data Model

Core internal models:

### `RepoTarget`

- host
- owner
- name
- display_name

### `WorkflowRunSummary`

- repo
- id
- workflow_name
- branch
- event
- status
- conclusion
- created_at
- started_at
- updated_at
- url

### `WorkflowRunDetail`

- summary
- jobs
- total_jobs
- completed_jobs
- failed_jobs
- running_jobs

### `WorkflowJobSummary`

- id
- name
- status
- conclusion
- started_at
- completed_at
- total_steps
- completed_steps
- failed_step_name

### `PullRequestSummary`

- repo
- number
- title
- author
- is_draft
- state
- review_decision
- review_requested_for_viewer
- ci_rollup
- updated_at
- url

### `RateLimitState`

- resource
- limit
- remaining
- used
- reset_at
- retry_after
- degraded_mode

## 13. Architecture

Rust stack:

- `ratatui` for rendering
- `crossterm` for terminal input and lifecycle
- `tokio` for async runtime
- `clap` for CLI parsing
- `reqwest` for HTTP
- `serde` / `serde_json`
- `toml` for config
- `directories` for config paths
- `open` or platform-specific launcher for browser opening

Optional:

- `octocrab` if it proves ergonomic for the mixed REST/GraphQL model

Recommendation:

Start with `reqwest` directly instead of `octocrab`.

Reason:

- cleaner control over headers, caching, ETags, and GraphQL payloads
- fewer abstraction leaks when dashboard-specific data shaping matters

## 13.1 High-level modules

```text
src/
├── main.rs
├── cli.rs
├── app.rs
├── config.rs
├── auth.rs
├── github/
│   ├── mod.rs
│   ├── client.rs
│   ├── rest_actions.rs
│   ├── graphql_prs.rs
│   ├── types.rs
│   └── rate_limit.rs
├── state/
│   ├── mod.rs
│   ├── models.rs
│   ├── store.rs
│   └── selection.rs
├── ui/
│   ├── mod.rs
│   ├── layout.rs
│   ├── theme.rs
│   ├── actions_table.rs
│   ├── prs_table.rs
│   ├── status_bar.rs
│   └── help.rs
└── util/
    ├── time.rs
    ├── truncate.rs
    └── browser.rs
```

## 13.2 Runtime model

Use a unidirectional event loop:

1. input events
2. timer tick events
3. network result events
4. state reduction
5. redraw

Network fetches run asynchronously outside the render loop.

The render loop never blocks on network I/O.

Actions drill-down data should be fetched through the same event system and rendered as an overlay, modal, or detail pane rather than spawning a second application mode.

## 13.3 State ownership

App state should own:

- repo list
- latest data snapshots
- selection and focus
- refresh schedule
- stale / degraded indicators
- recent errors

The UI reads immutable snapshots. Network tasks publish update events back into the app event queue.

## 14. Rendering Requirements

### 14.1 Performance budget

- full redraw should feel immediate at 10s polling
- no visible terminal flicker during updates
- idle CPU should stay low in long-running sessions
- narrow-width redraws should not explode layout cost

### 14.2 Width behavior

Compact mode must remain usable at:

- 40 columns minimum target

Split mode minimum:

- 80 columns

If width is below the supported threshold, Prism should render a clear resize warning instead of a broken layout.

### 14.3 Color and symbols

Use plain ASCII fallbacks where glyph width is unreliable.

Preferred internal status representation:

- queued: subtle pending marker
- in progress: spinner or animated pulse
- success: check mark or ASCII equivalent
- failure: cross mark or ASCII equivalent
- cancelled: stop marker
- skipped: dash marker

Theme rules:

- use terminal default foreground/background as the baseline
- derive accents conservatively from terminal capabilities
- never assume dark mode or light mode
- keep the interface readable without truecolor support

Do not use regular emoji. Terminal width correctness and compactness matter more than visual flair.

### 14.4 Progress indicators

Progress indicators are required for active workflow detail views and optional in list rows where width permits.

Rules:

- use compact text progress bars, not emoji bars
- bars must degrade cleanly to ASCII
- bars should represent known progress only
- if true progress is unknown, show indeterminate running state instead of fake completion

Primary usage:

- workflow run detail: completed jobs / total jobs
- job row detail: completed steps / total steps
- optional compact list suffix for selected in-progress item

## 15. Review and CI State Mapping

### 15.1 PR review state

Derived order:

1. `Merged`
2. `Closed`
3. `Draft`
4. `Changes requested`
5. `Approved`
6. `Review requested`
7. `Open`

### 15.2 CI rollup state

Derived order:

1. `Fail`
2. `Pending`
3. `Pass`
4. `Skipped`
5. `Unknown`

The dashboard should prefer decisiveness over completeness. Showing a single strong rollup is better than leaking every raw GitHub enum into the main table.

## 16. Errors and Degradation

Prism should fail soft.

### 16.1 Expected recoverable errors

- no token
- bad token
- missing repo permissions
- network timeout
- GitHub 5xx
- primary rate limit
- secondary rate limit
- malformed config
- repo renamed or inaccessible

### 16.2 Behavior

- app stays open when one pane refresh fails
- last known good data remains visible
- stale indicator appears on affected pane
- error count and last error summary appear in status bar or help overlay
- fatal startup errors produce actionable messages and non-zero exit codes
- selected run detail can remain open against stale cached data with a visible stale marker

## 17. Config File

Path:

- macOS/Linux: `~/.config/prism/config.toml`

Proposed shape:

```toml
host = "github.com"
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
```

## 18. Logging

Prism should separate TUI output from diagnostic logs.

Rules:

- never print logs into the active TUI surface
- write debug logs to a file only when enabled
- redact tokens and secrets always

Suggested log path:

- `~/.local/state/prism/prism.log`

## 19. Testing Strategy

### 19.1 Unit tests

Cover:

- status mapping
- review state reduction
- duration formatting
- truncation logic
- config precedence
- rate-limit backoff decisions

### 19.2 Snapshot / rendering tests

Cover:

- compact mode at narrow widths
- split mode normal width
- stale/error states
- empty-state rendering

### 19.3 Integration tests

Use mocked GitHub responses for:

- successful PR and Actions refresh
- 304 conditional response
- 403 rate-limit response
- partial repo failure

### 19.4 Manual smoke tests

- run against one public repo
- run against several private repos
- verify `gh auth token` fallback
- verify browser opening
- verify terminal resize handling
- verify long-running stability for at least 1 hour

## 20. Packaging and Distribution

### 20.1 Standalone binary

Primary distribution should be a standalone Rust binary.

Install vectors:

- Homebrew tap
- direct GitHub release download
- `cargo install --git ...` for developers

### 20.2 Optional `gh` extension wrapper

Optional secondary packaging:

- repo named `gh-prism` or a thin extension wrapper later

Do not make `gh` extension packaging the primary distribution path.

Reason:

- Prism is broader than a `gh` subcommand
- standalone binary is cleaner for terminal UX and release management

## 21. Security

- never store tokens in plaintext by default if OS keychain support is added later
- never echo tokens in logs or error output
- all browser-open targets must be GitHub URLs derived from API fields, not string-built from user input
- config parsing must reject obviously invalid repo identifiers early

## 22. Milestones

### Milestone 0: repo bootstrap

- Cargo project
- CLI skeleton
- config loader
- auth resolution
- empty app shell

Exit criteria:

- `prism --help` works
- `prism auth status` reports auth source clearly

### Milestone 1: Actions dashboard

- REST client for workflow runs
- compact and split layouts with Actions pane
- adaptive polling
- status bar

Exit criteria:

- multiple repos refresh correctly
- rate-limit state visible
- in-progress elapsed timers update locally

### Milestone 2: PR dashboard

- GraphQL client for PR summaries
- review and CI rollups
- requested-reviewer highlighting
- browser open action

Exit criteria:

- split mode is fully usable for live PR triage

### Milestone 3: hardening

- conditional request cache
- retry / backoff strategy
- snapshot tests
- resize handling
- log file support

Exit criteria:

- stable for 1-hour manual run
- no obvious flicker or runaway API usage

### Milestone 4: release

- GitHub Actions CI
- tagged binary releases
- Homebrew formula or tap
- install docs

Exit criteria:

- clean install on macOS
- release binary signed if desired

## 23. Open Questions

- Should v1 support saved repo groups in config only, or an interactive picker too?
- Should we support notifications in v1.1 or keep the tool strictly visual?
- Do we want optional issue monitoring later, or keep Prism permanently PR/Actions only?
- Is GitHub Enterprise a must-have for the first usable release, or shortly after?

## 24. Recommended Build Order

1. bootstrap repo and CLI
2. add config and auth resolution
3. build bare TUI shell with mock data
4. add Actions REST fetch path
5. add PR GraphQL fetch path
6. add adaptive polling and rate-limit handling
7. add browser open and help overlay
8. add tests and release pipeline

## 25. Decision Summary

- language: Rust
- rendering: `ratatui` + `crossterm`
- runtime: `tokio`
- CLI parser: `clap`
- HTTP: direct `reqwest`
- PR data: GraphQL
- Actions data: REST
- packaging: standalone binary first
- v1 layouts: `compact`, `split`
- v1 focus: PRs + Actions only
