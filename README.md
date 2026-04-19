# Prism

Prism is a keyboard-driven terminal dashboard for watching GitHub Actions and pull requests across an explicit list of repositories.

This README documents the current shipped CLI and TUI behavior in `prism 0.1.0`. Planning docs in [`docs/SPEC.md`](./docs/SPEC.md) and [`docs/EXECUTION_PLAN.md`](./docs/EXECUTION_PLAN.md) describe the target direction; they are not a promise that every planned command or behavior exists today.

## What Ships Today

Prism currently provides:

- a live TUI with repo-first split panes and a stacked compact mode
- two layouts: `split` and `compact`
- workflow and PR detail drill-down with live progress bars and failure markers
- browser open for the selected PR or workflow run
- `prism auth status`
- `prism config init`

Prism does not currently provide:

- repo-management commands such as `repos add` or `repos list`
- workflow log viewing
- GitHub write actions such as rerun, cancel, merge, or review
- published binaries; source builds are the supported path right now

## Install From Source

```bash
git clone https://github.com/lynxsyn/prism
cd prism
cargo build --release
./target/release/prism --help
```

For day-to-day local use during development, `cargo run -- ...` is fine:

```bash
cargo run -- --help
```

## Quick Start

1. Create a starter config:

   ```bash
   cargo run -- config init
   ```

2. Edit the generated config file and replace the placeholder repos.

   Default config paths:

   - macOS: `~/Library/Application Support/app.lynxsyn.prism/config.toml`
   - Linux: `$XDG_CONFIG_HOME/prism/config.toml` or `~/.config/prism/config.toml`

3. Provide a token:

   ```bash
   export PRISM_TOKEN=ghp_your_token_here
   ```

   Or authenticate `gh` and let Prism fall back to `gh auth token`.

4. Start Prism:

   ```bash
   cargo run --
   ```

   Or skip config-file repos for a one-off session:

   ```bash
   cargo run -- openai/codex rust-lang/cargo
   ```

## CLI Reference

Current top-level usage:

```text
prism [OPTIONS] [OWNER/REPO]... [COMMAND]
prism auth status
prism config init [--force]
```

Available subcommands:

- `auth status`
- `config init`

Global flags:

- `-c, --config <PATH>`: use a specific config file
- `-r, --repo <OWNER/REPO>`: add a repo target; repeatable
- `[OWNER/REPO]...`: positional repo targets; combined with `--repo`
- `-i, --interval <SECONDS>`: base refresh interval, default `10`, clamped to a minimum of `5`
- `-m, --mode <compact|split>`: layout mode, default `split`
- `--host <HOST>`: GitHub host, default `github.com`
- `--actions-limit <N>`: workflow runs per repo, default `10`
- `--prs-limit <N>`: open PRs per repo, default `30`
- `--open-command <CMD>`: override how URLs are opened
- `--no-color`: disable color styling
- `--ascii-only`: replace Unicode spinners and bars with ASCII-safe output

Repo target formats:

- `owner/repo`
- `host/owner/repo`

Current host rule:

- Prism supports one effective GitHub host per session; mixed-host watch lists are rejected

Repo precedence is important:

- if you pass any repos on the CLI, Prism ignores `repos = [...]` from the config file for that run
- `--repo` values and positional repo values are merged together
- if a repo target is `owner/repo` and the effective host is not `github.com`, Prism rewrites that repo to the configured host

Examples:

```bash
# Start from config-file repos
prism

# One-off watch list
prism openai/codex rust-lang/cargo

# Mixed explicit repo flags
prism -r openai/codex rust-lang/cargo

# Compact mode for narrow terminals
prism --mode compact openai/codex

# GitHub Enterprise host, using host-wide default
prism --host github.example.com platform/api

# GitHub Enterprise host, per-repo target
prism github.example.com/platform/api

# Validate which auth source Prism will use
prism auth status
```

## Authentication

Prism resolves auth in this order:

1. the environment variable named by `auth.token_env` in config, default `PRISM_TOKEN`
2. `auth.token` from the config file
3. `gh auth token --hostname <host>` if `auth.use_gh_fallback = true`

`prism auth status` shows the effective host, the winning auth source, and the masked token suffix:

```text
host: github.com
source: gh
token: ****rl04
```

If Prism cannot resolve a token, it exits with an actionable startup error instead of launching the TUI.

Recommended token permissions:

- repository metadata: read
- pull requests: read
- actions: read

## Configuration

`prism config init` writes this starter config:

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

Current config behavior:

- `host`: defaults to `github.com`
- `interval`: default `10`; values below `5` are raised to `5`
- `mode`: `split` or `compact`
- `actions_limit`: runs fetched per repo
- `prs_limit`: open PRs fetched per repo
- `repos`: explicit watch list; Prism never auto-discovers repos
- `[auth].token`: inline token, if you want config-file auth instead of env or `gh`
- `[auth].token_env`: env var Prism should read first
- `[auth].use_gh_fallback`: defaults to `true`
- `[ui].open_command`: custom browser command; Prism substitutes `{url}` if present, otherwise appends the URL
- `[ui].ascii_only`: ASCII-safe spinners and progress bars
- `[ui].theme`: currently written by `config init`, but the shipped TUI does not apply a configurable theme yet

Only two non-auth env vars are parsed directly today:

- `PRISM_HOST`
- `PRISM_INTERVAL`

CLI flags override env and config values. Config overrides built-in defaults.

## Using The TUI

### Layout modes

- `split`: side-by-side repo panes; each pane can toggle between Actions and Pull Requests and Prism requires at least `98` columns for this mode
- `compact`: stacked Actions and Pull Requests panes; Prism requires at least `56` columns for this mode

If the terminal is narrower than the current mode supports, Prism shows a resize warning instead of trying to squeeze the tables into unreadable columns.

### Actions pane

In `split`, each repo pane shows Actions or Pull Requests for one configured repo. In `compact`, Actions stay in the top pane.

Current columns:

- `Workflow`
- `Branch`
- `State`
- `Age`
- `Dur`

Behavior:

- newest runs sort first
- running rows animate locally between polls
- duration uses `run_started_at` when GitHub provides it
- status uses success/failure/queued/running reductions

### Pull Requests pane

In `split`, use `Tab` on the focused repo pane to switch it to Pull Requests. In `compact`, Pull Requests stay in the bottom pane.

Current columns:

- `#`
- `Title`
- `Author`
- `Review`
- `CI`
- `Updated`

Behavior:

- PRs sort by most recently updated
- direct review requests for the authenticated viewer are highlighted
- review state is reduced to `draft`, `approved`, `changes`, `review`, `requested`, or `open`
- CI rollup is reduced to `pass`, `fail`, `pending`, `skipped`, or `-`

### Detail view

Press `Enter` from an Actions or Pull Requests list to open inline detail inside the current pane.

Actions detail currently shows:

- repo, workflow, run title, branch, and event
- overall run state
- job completion counts
- a run-level progress bar
- one row per job
- job-level progress bars when the step list is reliable enough
- `indeterminate` when Prism cannot trust the step progress math
- the first failed step label when GitHub exposes it

PR detail currently shows:

- repo, author, review state, and CI rollup
- check completion counts
- a PR-level progress bar
- one row per reported check
- per-check progress bars for pending, running, and completed states
- explicit `[PASS]`, `[FAIL]`, `[WAIT]`, and `[RUN ]` badges

Press `l` or `o` to open the selected PR or workflow run in the browser.

### Status bar and refresh behavior

The status bar shows:

- mode
- last successful refresh time
- next refresh countdown
- lowest remaining GitHub rate limit seen across the active requests
- host
- `stale` when the last refresh had one or more fetch errors
- `l:detail` or `esc:close`

Polling behavior:

- base interval comes from `--interval` or config, default `10`
- if the remaining rate limit drops to `500` or below, Prism slows to at least `20s`
- if the remaining rate limit drops to `100` or below, Prism slows to at least `60s`
- failed refreshes keep the last good data on screen and mark the session stale

## Keyboard Controls

- `q`: quit
- `r`: force refresh now
- `Tab`: toggle the focused pane between Actions and Pull Requests
- `Left` / `Right`: switch focus between repo panes in `split`
- `j` / `Down`: move down
- `k` / `Up`: move up
- `g`: jump to top
- `G`: jump to bottom
- `Enter`: open detail for the selected Actions row or PR row
- `l` or `o`: open the selected PR or workflow run in the browser
- `Esc`: close help or detail
- `?`: toggle the help overlay

There is no in-app keybinding to switch between `split` and `compact`; choose the mode when you launch Prism.

## Browser Opening

By default Prism asks the OS to open the selected URL.

If you need a specific launcher, use `--open-command` or `[ui].open_command`, for example:

```bash
prism --open-command 'open {url}'
prism --open-command 'xdg-open {url}'
prism --open-command 'firefox --new-tab'
```

If `{url}` is missing, Prism appends the URL to the end of the command.

## Troubleshooting

No repos configured:

```text
Error: no repositories configured; pass owner/repo arguments or create /path/to/config.toml
```

Fix it by either:

- adding repos to the config file
- passing repos on the CLI for that session

Auth problems:

- run `prism auth status`
- verify the resolved `host`
- verify the token source
- if you depend on `gh` fallback, make sure `gh auth token --hostname <host>` works

Terminal too narrow:

- switch to `--mode compact`
- widen the terminal

## Docs Map

- [`docs/USER_GUIDE.md`](./docs/USER_GUIDE.md): current operator workflow and usage
- [`docs/SPEC.md`](./docs/SPEC.md): target product behavior and current CLI contract notes
- [`docs/EXECUTION_PLAN.md`](./docs/EXECUTION_PLAN.md): implementation sequencing
- [`docs/DEVELOPMENT.md`](./docs/DEVELOPMENT.md): contributor guardrails
