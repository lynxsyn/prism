# Prism

Prism is a terminal-native GitHub dashboard for watching pull requests and GitHub Actions across a small, explicit set of repositories.

It is being built as a pure Rust CLI/TUI for operators who want live repo awareness without living in browser tabs.

## Status

Prism is in active implementation.

Current repo state:

- product spec and converged execution plan are checked in
- user-facing CLI contract is defined
- v1 distribution target is a standalone binary
- `gh` extension packaging is explicitly deferred out of v1

This README documents the intended v1 behavior and onboarding contract so the app and docs converge on the same surface.

## What Prism Is For

Prism exists to solve two recurring GitHub workflows:

1. PR triage across selected repositories
2. live CI monitoring as workflow runs queue, start, progress, fail, and complete

It sits between one-shot `gh` commands and a browser session:

- `gh` is still useful for ad hoc queries and auth bootstrap
- Prism owns the always-on dashboard view

## v1 Scope

Prism v1 is intentionally narrow.

Included:

- Rust TUI built with `ratatui` + `crossterm`
- standalone binary first
- multi-repo monitoring from config or CLI args
- compact and split layouts
- pull request overview with review and CI rollups
- live GitHub Actions list with adaptive polling
- in-app run drill-down into jobs and step progress
- browser open for selected PRs and workflow runs
- optional `gh auth token` fallback for authentication

Explicitly out of scope for v1:

- `gh` extension packaging
- browser UI or web server
- log viewer
- inline GitHub mutations
- issue tracking
- activity/event feed pane
- Windows support

## Design Principles

- clean, compact, terminal-first layout
- no emoji
- minimal glyphs with ASCII fallbacks
- theme inherits from the terminal instead of imposing a custom palette
- progress bars and state markers should feel closer to a status bar than a novelty TUI
- polling behavior must be rate-limit aware and stable in long-running sessions

## Planned Feature Set

### Pull requests

- open PRs only in v1
- repo, number, title, author, review state, CI rollup, updated time
- requested-review rows called out clearly
- open selected PR in browser

### Actions

- recent workflow runs per repo
- queued, running, passed, failed, cancelled, skipped states
- live elapsed timers for in-progress runs
- local refresh countdown and stale markers
- open selected run in browser

### Actions drill-down

- focused run metadata: workflow, branch, event, run number, age, overall state
- jobs grouped by failure/running/pending/completed
- compact progress bars for run-level job completion and job-level step completion
- most recent failed step label where GitHub exposes it
- bounded detail polling only while the detail view is open

Prism v1 is not a replacement for GitHub logs. Drill-down answers "what is happening" and "what failed", not "show every log line".

## Install

### Current release status

Public binaries are not published yet.

The intended v1 install paths are:

1. GitHub release download
2. Homebrew tap
3. `cargo install --git https://github.com/lynxsyn/prism`

The app is being built as a standalone binary first. A `gh prism` extension wrapper may be added later if it is trivially thin, but that is not part of the first release target.

### Platform target

- macOS first
- Linux shortly after
- GitHub.com first, GitHub Enterprise via explicit host config

## Authentication

Prism resolves auth in this order:

1. `PRISM_TOKEN`
2. token reference from config
3. `gh auth token` fallback

Recommended token shape:

- fine-grained PAT
- read-only where possible
- scoped only to the repos you want Prism to watch

Expected permissions:

- repository metadata: read
- pull requests: read
- actions: read

Why the `gh` fallback exists:

- Prism stays usable if you already rely on GitHub CLI auth
- Prism still remains a standalone tool and does not shell out to `gh` for normal polling

## Configuration

Default config path:

- macOS/Linux: `~/.config/prism/config.toml`

Config precedence:

1. CLI args
2. environment variables
3. config file
4. defaults

Example config:

```toml
host = "github.com"
interval = 10
mode = "split"
actions_limit = 10
prs_limit = 30

repos = [
  "owner/repo-a",
  "owner/repo-b",
  "owner/repo-c",
]

[auth]
token_env = "PRISM_TOKEN"
use_gh_fallback = true

[ui]
theme = "terminal"
open_command = ""
ascii_only = false
```

Supported environment variables:

- `PRISM_TOKEN`
- `PRISM_HOST`
- `PRISM_INTERVAL`
- `BROWSER`

Configuration intent:

- keep repo selection explicit
- avoid org-wide auto-discovery in v1
- keep rate-limit math predictable

## Usage

Target CLI surface:

```text
prism [repo...] [flags]
prism auth status
prism config init
prism repos add owner/repo
prism repos list
```

Common examples:

```bash
# Watch repos from config
prism

# Watch explicit repos for this session
prism lynxsyn/prism owner/service-a owner/service-b

# Faster refresh during active triage
prism --interval 5

# Narrow pane mode
prism --mode compact

# Override config path
prism --config ~/.config/prism/work.toml

# Check which auth source Prism will use
prism auth status
```

Core flags:

- `-r, --repo <OWNER/REPO>` repeatable repo target
- `-f, --config <PATH>` config file path
- `-i, --interval <SECONDS>` polling interval, default `10`
- `-m, --mode <compact|split>` default `split`
- `--host <HOST>` GitHub hostname, default `github.com`
- `--actions-limit <N>` workflow runs per repo, default `10`
- `--prs-limit <N>` open PR row limit, default `30`
- `--open-command <CMD>` override browser launcher
- `--no-color` disable color output

## Layout Modes

### `compact`

Use this when Prism is running in a narrow iTerm split or side pane.

Characteristics:

- one stacked column
- lower information density
- optimized for roughly `40-70` columns
- still preserves selection, refresh state, and open/drill-down actions

### `split`

This is the default operator view.

Characteristics:

- left pane for Actions
- right pane for PRs
- optimized for `80+` columns
- status bar always visible

If the terminal is too narrow for the selected mode, Prism should show a clear resize warning instead of rendering a broken layout.

## Keybindings

Mandatory v1 keybindings:

| Key | Action |
| --- | --- |
| `q` | Quit Prism |
| `r` | Force refresh immediately |
| `Tab` | Switch focus between panes |
| `j` / `Down` | Move selection down |
| `k` / `Up` | Move selection up |
| `g` | Jump to top of focused pane |
| `G` | Jump to bottom of focused pane |
| `o` / `Enter` | Open selected PR or run in browser |
| `l` | Open local detail/drill-down for selected item |
| `Esc` | Close detail or help overlay |
| `?` | Open help overlay |

Potential post-v1 additions:

- `/` for filtering
- `1` and `2` for mode switching

## Status Indicators

Prism avoids regular emoji. Status should read cleanly in a coding terminal and degrade to ASCII when needed.

Target semantic states:

| State | Meaning | Visual intent |
| --- | --- | --- |
| queued | waiting to start | subtle pending marker |
| running | actively progressing | spinner or pulse plus elapsed time |
| success | completed successfully | check-style marker |
| failure | completed with failure | cross-style marker |
| cancelled | stopped intentionally or by policy | stop marker |
| skipped | intentionally not run | neutral dash marker |
| stale | cached data shown after refresh failure | dim or warned state |

Progress representation rules:

- use compact text bars, not decorative blocks unless width and glyph support are reliable
- only show determinate progress when GitHub data is trustworthy
- if progress cannot be inferred cleanly, show running state without fake percentages

Typical shapes Prism should support:

```text
Jobs   [3/8]
Steps  [#####.....] 5/10
State  running  07:42
```

Exact glyph choice may vary with terminal capability and `ascii_only` settings, but the UI contract is the same: compact, legible, and theme-aware.

## Actions Drill-Down

Drill-down is one of Prism's main differentiators.

From the Actions pane, selecting a workflow run and opening details should reveal:

- workflow name
- repo and branch
- event type
- run number
- current run state
- elapsed duration
- run-level job completion progress
- per-job state
- per-job step progress when GitHub exposes it
- most recent failed step label when available

Important v1 boundaries:

- no log streaming
- no expandable per-step UI tree
- no rerun or cancel controls
- no second app mode; detail opens as an overlay or detail pane inside the same event loop

Polling behavior in detail view:

- run summaries continue polling on the main cadence
- selected run detail polls separately at a bounded rate only while detail is open
- closing detail stops detail polling

## Polling, Caching, and Rate Limits

Prism treats API budget as part of the product, not a hidden implementation detail.

Rules:

- use REST for Actions and GraphQL for PR summary data
- use conditional requests and cached snapshots where possible
- keep local timers for running jobs instead of polling every second
- preserve the last good snapshot if a refresh fails
- mark panes stale instead of clearing the screen

Adaptive polling tiers:

- normal: user interval, default `10s`
- low quota: slow to at least `20s`
- very low quota: slow to at least `60s`
- rate-limited: pause until reset or `Retry-After`, then resume with backoff

Prism's initial planning target is a small explicit set of repos, roughly `5-10` repos at a `10s` cadence. Wider fleets should expect slower practical polling or explicit warnings.

## Limitations

Known v1 constraints:

- no activity feed pane
- no issue dashboard
- no inline diff review
- no log viewer
- no write operations back to GitHub
- no Windows support
- repo list is explicit; v1 is not an org crawler
- `gh` integration is limited to auth fallback, not the live data plane

If the PR GraphQL mapping proves uglier than expected during implementation, that work remains gated by the spike documented in the execution plan. The Actions path is the proving ground for live-state behavior first.

## Development Workflow

Prism is being built in thin vertical slices.

Recommended execution order:

1. bootstrap the Rust workspace
2. build the TUI shell with mock data
3. wire config and auth
4. ship the live Actions list
5. ship Actions drill-down
6. prove the PR GraphQL query shape
7. ship the live PR pane
8. harden reliability before visual polish
9. package standalone releases

Contributor expectations:

- keep Prism a standalone binary first
- do not turn Actions drill-down into a log viewer
- treat rate-limit handling and stale-state behavior as core product work
- keep selection anchored to entity identity across refreshes
- validate narrow terminal widths early

Planned Rust stack:

- `ratatui`
- `crossterm`
- `tokio`
- `clap`
- `reqwest`
- `serde`
- `toml`

Once the scaffold lands, the normal contributor loop should be:

```bash
cargo fmt
cargo clippy -- -D warnings
cargo test
cargo run -- --help
```

## Docs Map

- [docs/SPEC.md](docs/SPEC.md): product specification and behavior contract
- [docs/EXECUTION_PLAN.md](docs/EXECUTION_PLAN.md): phased build plan
- [docs/USER_GUIDE.md](docs/USER_GUIDE.md): operator-focused usage and UI behavior
- [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md): contributor workflow and implementation guardrails
- [docs/ADVERSARIAL_REVIEW.md](docs/ADVERSARIAL_REVIEW.md): first review round
- [docs/ADVERSARIAL_REVIEW_ROUND2.md](docs/ADVERSARIAL_REVIEW_ROUND2.md): second review round
- [docs/ADVERSARIAL_REVIEW_ROUND3.md](docs/ADVERSARIAL_REVIEW_ROUND3.md): convergence review
