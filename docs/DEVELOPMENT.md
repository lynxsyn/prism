# Prism Development Guide

This document keeps implementation work aligned with the converged plan.
It is a contributor document, not the source of truth for the current shipped CLI or TUI surface; use the README and user guide for that.

## 1. Product Boundaries

Prism v1 is:

- a Rust CLI/TUI
- a standalone binary first
- focused on pull requests and GitHub Actions only
- optimized for long-running terminal use

Prism v1 is not:

- a `gh` extension
- a log viewer
- a write-capable GitHub client
- an issue dashboard

## 2. Build Order

Execution order from the converged plan:

1. bootstrap the repo and Rust workspace
2. build the TUI shell with mock data
3. add config and auth resolution
4. ship the live Actions list
5. ship bounded Actions drill-down
6. run the PR GraphQL spike
7. ship the live PR pane
8. harden reliability
9. polish theme behavior
10. package releases

The order matters.

Do not:

- start the PR pane before the GraphQL spike is real
- treat reliability work as cleanup
- expand the Actions detail scope into a log product

## 3. Stack Decisions

Converged implementation choices:

- `ratatui` for rendering
- `crossterm` for terminal lifecycle and input
- `clap` for CLI parsing
- `reqwest` for HTTP
- background polling off the render loop
- GraphQL for PR summaries
- REST for Actions runs and jobs

Why not `gh api` subprocesses:

- worse header handling
- worse caching control
- worse rate-limit and ETag visibility
- harder testability

Why not a `gh` extension first:

- Prism is broader than a subcommand
- standalone packaging is cleaner
- core UX should not inherit `gh` release constraints

## 4. Non-Negotiable Guardrails

### Actions drill-down guardrail

Keep v1 detail capped at:

- run summary
- jobs list
- step progress
- failed step label

Do not add:

- raw logs
- expanded step inspector
- rerun or cancel controls

### PR GraphQL guardrail

Treat the GraphQL spike as a gate.

Required outcome:

- either prove the query shape is clean enough
- or record a fallback decision before pane implementation

### Reliability-before-polish guardrail

Before spending time on theme nuance:

- stale-state handling must work
- backoff behavior must work
- long-running sessions must be stable

## 5. UX Constraints

Prism should feel disciplined and compact.

Requirements:

- no emoji
- terminal-theme-first styling
- ASCII fallback
- minimal flicker
- identity-stable selection across refreshes
- explicit resize warning when width is below supported thresholds

Target widths:

- compact mode: around `52+` columns
- split mode: around `96+` columns

## 6. Runtime Rules

The render loop must never block on network I/O.

Target event flow:

1. input event
2. tick event
3. network result event
4. state reduction
5. redraw

Required runtime behavior:

- local elapsed timers for running jobs and runs
- adaptive polling under quota pressure
- cached last-good snapshot on refresh failure
- detail polling only while detail is open

## 7. API Discipline

Actions:

- REST
- use ETag and `If-None-Match` where possible
- capture rate-limit headers

PRs:

- GraphQL
- batch repo summary data conservatively
- map review state and CI rollup into Prism-specific reductions

Never shell out to `gh` for the normal data plane.

Allowed `gh` use:

- `gh auth token` fallback only

## 8. Suggested Contributor Loop

Once the workspace exists, the baseline loop should be:

```bash
cargo fmt
cargo clippy -- -D warnings
cargo test
cargo run -- --help
```

During UI bring-up:

- use checked-in fixtures
- validate narrow-width rendering early
- snapshot important states before wiring every live endpoint

During API bring-up:

- keep request concurrency bounded
- verify stale rendering with forced failures
- test public repos first

## 9. Docs To Keep In Sync

When implementation changes the contract, update these docs together:

- [README.md](../README.md)
- [SPEC.md](./SPEC.md)
- [EXECUTION_PLAN.md](./EXECUTION_PLAN.md)
- this file

If implementation pressure tempts a scope change, update the plan explicitly rather than drifting silently.
