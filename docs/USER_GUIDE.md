# Prism User Guide

This guide describes the intended operator workflow for Prism v1.

Prism is a terminal dashboard for:

- watching pull requests across chosen repositories
- watching GitHub Actions runs update live
- drilling from a run summary into job and step progress

## 1. Start With a Small Repo Set

Prism is designed for a deliberate watch list, not a full organization firehose.

Recommended starting point:

- 2-5 active repositories
- `10s` polling
- `split` mode in a normal terminal

Planning target for v1:

- roughly `5-10` repos at a `10s` cadence

If you push beyond that, Prism should degrade gracefully, but expect slower effective refresh or explicit warnings.

## 2. Authentication Setup

Preferred auth:

1. set `PRISM_TOKEN`
2. enable `gh` fallback only as a convenience

Why:

- explicit tokens are easier to reason about
- headless or packaged use should not require GitHub CLI

Recommended token properties:

- fine-grained PAT
- repo-scoped where possible
- read-only access

Required read access:

- repository metadata
- pull requests
- actions

## 3. Config Strategy

Prism uses a single config file by default:

- `~/.config/prism/config.toml`

Minimal example:

```toml
interval = 10
mode = "split"
repos = ["owner/repo-a", "owner/repo-b"]

[auth]
token_env = "PRISM_TOKEN"
use_gh_fallback = true
```

Practical advice:

- keep the repo list explicit
- avoid mixing unrelated repos into one screen
- prefer separate config files for different contexts if needed

Examples:

- one config for personal repos
- one config for client repos
- one config for release-day monitoring

## 4. How To Read the Screen

### Split mode

Default operational view:

- left pane: Actions
- right pane: PRs
- bottom line: status bar

Use split mode when you have enough width to keep both panes readable.

### Compact mode

Use compact mode when:

- Prism is in a narrow split
- you want a lower-noise watch surface
- you care more about "what changed?" than browsing lots of columns

## 5. Pull Request Pane

The PR pane is a triage surface, not a review tool.

It should answer:

- which PRs exist
- which need my attention
- which are blocked by CI
- which are drafts or otherwise not ready

Core fields:

- repo
- PR number
- title
- author
- review state
- CI rollup
- updated time

Expected review states:

- Merged
- Closed
- Draft
- Changes requested
- Approved
- Review requested
- Open

Expected CI rollups:

- Fail
- Pending
- Pass
- Skipped
- Unknown

## 6. Actions Pane

The Actions pane is the live operations view.

It should answer:

- what just started
- what is still running
- what already failed
- how old active runs are

Core fields:

- repo
- workflow
- branch
- status
- duration
- triggered

Important behavior:

- newest runs first
- recent history retained per repo
- running rows keep counting locally between polls
- stale data remains visible if the next fetch fails

## 7. Drill-Down Behavior

Select a workflow run and open detail to inspect:

- overall run state
- job completion count
- running jobs
- failed jobs
- most recent failed step label
- per-job step progress where GitHub exposes enough step metadata

This view exists to help you decide:

- is the pipeline still moving?
- where did it fail?
- is it near the end or stuck early?

Not included:

- raw logs
- rerun controls
- cancel controls
- per-step expansion tree

## 8. Status Bar Expectations

The status bar is part of the product contract. It should always show:

- current mode
- last successful refresh
- next refresh countdown
- hostname
- rate-limit remaining
- degraded or paused state when polling slows down

This is where Prism communicates "the data is old but still usable" instead of forcing you to infer it from missing rows.

## 9. Keyboard Workflow

Primary operator rhythm:

1. keep Prism open during active work
2. watch the Actions pane for new runs and failures
3. tab to PRs when review state changes matter
4. open drill-down only for the run that needs explanation
5. open browser only when you need deeper context

Core controls:

- `Tab` switches focus
- `j` / `k` move selection
- `r` refreshes now
- `l` opens local detail
- `o` or `Enter` opens the selected item in the browser
- `?` opens help
- `q` quits

## 10. Theme and Visual Behavior

Prism intentionally avoids bright, novelty styling.

Visual rules:

- no emoji
- minimal glyphs
- compact spacing
- terminal theme is the baseline
- ASCII fallback remains usable

It should feel at home inside a terminal profile used for coding, not like a game HUD.

## 11. Failure and Degraded States

Prism should fail soft.

Expected behavior under trouble:

- keep the last good snapshot
- mark stale panes clearly
- slow down polling under quota pressure
- pause cleanly on hard rate limits
- recover automatically when allowed

The UI should prefer continuity over false freshness.

## 12. v1 Boundaries

Prism v1 stops at the dashboard layer.

No v1 support for:

- activity feed pane
- issue monitoring
- inline logs
- diff review
- GitHub write actions
- `gh prism` extension packaging

The goal is a reliable watch surface first, not a full terminal GitHub client.
