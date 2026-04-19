# Prism User Guide

This guide describes the current operator workflow for the shipped Prism CLI and TUI.

For roadmap and target behavior, see [`SPEC.md`](./SPEC.md) and [`EXECUTION_PLAN.md`](./EXECUTION_PLAN.md). This document stays anchored to what the binary does today.

## 1. Start Prism

Prism needs two things before the TUI can start:

- at least one repository target
- a GitHub token from env, config, or `gh`

Fastest setup:

```bash
prism config init
export PRISM_TOKEN=ghp_your_token_here
prism
```

Or skip config repos for a one-off watch session:

```bash
prism openai/codex rust-lang/cargo
```

If you run Prism with no repos at all, it exits before opening the TUI.

## 2. Choose Repos Deliberately

Prism is designed for a small, explicit watch list, not org-wide discovery.

Good starting shape:

- `2-5` repos
- `10s` interval
- `split` mode when you want stacked repo panes with full terminal width

Current planning envelope from the implementation:

- around `5-10` repos at `10s` polling

If you watch more repos, Prism still runs, but expect more rate-limit pressure and slower effective refresh.

Repo input formats:

- `owner/repo`
- `host/owner/repo`

Repo precedence:

- any repos passed on the CLI replace config-file repos for that launch
- `--repo` and positional repos are merged together

## 3. Authentication

Prism resolves auth in this order:

1. the env var named by `[auth].token_env`, default `PRISM_TOKEN`
2. `[auth].token` in the config file
3. `gh auth token --hostname <host>` when `[auth].use_gh_fallback = true`

Use this to inspect the winning source:

```bash
prism auth status
```

That prints:

- the effective host
- the source label, such as `env:PRISM_TOKEN`, `config`, or `gh`
- the final four token characters, masked as `****abcd`

Recommended token scope:

- repo metadata read
- pull requests read
- actions read

## 4. Configuration

Default config path depends on the platform:

- macOS: `~/Library/Application Support/app.lynxsyn.prism/config.toml`
- Linux: `$XDG_CONFIG_HOME/prism/config.toml` or `~/.config/prism/config.toml`

Starter config:

```toml
host = "github.com"
interval = 10
mode = "split"
actions_limit = 25
prs_limit = 50

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

Important current behavior:

- `interval` defaults to `10`; values below `5` are raised to `5`
- only `PRISM_HOST` and `PRISM_INTERVAL` are read directly from env
- `theme` is written into the config template, but the current TUI does not apply a theme selector yet
- `--no-color` is CLI-only; there is no config key for it

## 5. Pick The Right Layout

Prism has two launch-time modes:

### `split`

Use `split` when you want repo panes stacked top-bottom so each one can use the full terminal width.

Current minimum width:

- `60` columns

Current minimum height:

- `18` rows

What you get:

- repo A in the top pane
- repo B in the bottom pane
- `Tab` toggles the focused repo pane between Actions and Pull Requests
- `Left` and `Right` switch focus between repo panes
- both repo panes can keep their own inline detail open at the same time
- status bar across the bottom

### `compact`

Use `compact` for a narrow side pane or split terminal.

Current minimum width:

- `56` columns

What you get:

- Actions on top
- Pull Requests below
- same status bar and keybindings as split mode

If the terminal is too small for the chosen mode, Prism shows a resize warning instead of rendering broken tables.

## 6. Read The Actions Pane

The Actions pane is the live CI watch surface.

Current columns:

- `Workflow`
- `Branch`
- `State`
- `Age`
- `Dur`

What those fields mean:

- `Age`: time since the run was created
- `Dur`: run duration from `run_started_at`, or `-` if GitHub does not provide a start time

Current behavior:

- runs sort newest first
- running rows animate locally between polls
- queued runs show queued state
- completed runs reduce to success, failure, cancelled, timeout, or skipped

Use this pane to answer:

- what just started
- what is still running
- what just failed
- how old the active runs are

## 7. Read The Pull Requests Pane

The Pull Requests pane shows open PRs only.

Current columns:

- `#`
- `Title`
- `Author`
- `Review`
- `CI`
- `Updated`

Current review reductions:

- `draft`
- `approved`
- `changes`
- `review`
- `requested`
- `open`

Current CI reductions:

- `pass`
- `fail`
- `pending`
- `skipped`
- `-`

Important detail:

- direct review requests for the authenticated viewer are highlighted
- team review requests are not currently reduced into that highlight

Use this pane to answer:

- which open PRs need review
- which PRs are blocked by checks
- which PRs changed recently

## 8. Open Inline Detail

Press `Enter` while focused on an Actions list or Pull Requests list.

Actions detail currently shows:

- repo, workflow name, run title, branch, and event
- run state
- completed jobs vs total jobs
- running job count
- failed job count
- run-level progress bar
- one row per job
- job-level progress bars when Prism trusts the step data
- `indeterminate` when step progress is ambiguous
- first failed step label when GitHub exposes it

Workflow detail is intentionally bounded:

- no raw logs
- no per-step expansion tree
- no rerun or cancel controls

Workflow detail opens inline in the current pane instead of as a floating window.

PR detail currently shows:

- repo, author, review state, and CI rollup
- check completion counts
- a PR-level progress bar
- one row per check
- a progress bar for each pending, running, or completed check
- explicit `[PASS]`, `[FAIL]`, `[WAIT]`, and `[RUN ]` badges

In `split`, opening detail in one repo pane does not close detail already open in the other pane.

Use `l` or `o` to open the selected PR or workflow run in the browser.

## 9. Use The Keyboard Efficiently

Core controls:

- `q`: quit
- `r`: refresh now
- `Tab`: toggle the focused pane between Actions and Pull Requests
- `Left` / `Right`: switch focus between repo panes in `split`
- `j` / `Down`: move down
- `k` / `Up`: move up
- `g`: jump to top
- `G`: jump to bottom
- `Enter`: open detail for the selected workflow run or PR
- `l` or `o`: open the selected PR or workflow run in the browser
- `Esc`: close detail in the focused pane or help
- `?`: toggle help

There is no live mode toggle key. Choose `--mode split` or `--mode compact` when starting Prism.

## 10. Understand The Status Bar

The bottom status bar always shows:

- current mode
- last successful refresh timestamp
- next refresh countdown
- rate-limit summary
- host
- detail hint or close hint

When Prism hits refresh trouble:

- last known good data stays visible
- the status bar adds `stale`

Current rate-limit backoff:

- base interval from config or CLI
- at `<= 500` remaining requests, Prism slows to at least `20s`
- at `<= 100` remaining requests, Prism slows to at least `60s`

## 11. Open In The Browser

Press `l` or `o` on:

- a selected workflow run
- a selected PR
- the open workflow-detail view

By default Prism asks the OS to open the URL.

Override this with `--open-command` or `[ui].open_command`.

Examples:

```bash
prism --open-command 'open {url}'
prism --open-command 'xdg-open {url}'
prism --open-command 'firefox --new-tab'
```

If `{url}` is missing, Prism appends the URL to the command string.

## 12. Common Failure Cases

### No repos configured

Startup error:

```text
Error: no repositories configured; pass owner/repo arguments or create /path/to/config.toml
```

Fix:

- add `repos = [...]` to the config file
- or pass repos on the CLI

### Auth missing or wrong host

Checks:

- run `prism auth status`
- verify the `host`
- verify the source
- if relying on `gh`, verify `gh auth token --hostname <host>` works

### Terminal too small

Fix:

- switch to `--mode compact`
- widen or heighten the terminal

### Browser open command not working

Fix:

- test the command manually with a URL
- include `{url}` if the command needs the URL in the middle
- omit `{url}` if simple URL appending is enough

## 13. Current Boundaries

Prism today is a watch surface, not a full terminal GitHub client.

Current non-features:

- repo add/list management commands
- log viewing
- search or filter prompt
- GitHub mutations
- published release binaries
