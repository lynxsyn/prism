# Prism Execution Plan

Related docs:

- [README.md](../README.md) for the user-facing overview
- [USER_GUIDE.md](./USER_GUIDE.md) for operator workflow and usage
- [DEVELOPMENT.md](./DEVELOPMENT.md) for contributor guardrails
- [SPEC.md](./SPEC.md) for the product behavior contract

## 1. Objective

Build Prism as a Rust terminal dashboard for:

- multi-repo PR monitoring
- live GitHub Actions monitoring
- in-app run drill-down into jobs and step progress
- clean compact terminal UX
- standalone binary first, optional `gh prism` extension packaging after core stability

This plan is the implementation sequence, not the product spec. The spec remains the source of truth for product behavior.

## 2. Delivery Principles

- build the core data plane before polish
- validate UX with mock data before wiring live APIs
- treat API budget and stale-state handling as core functionality, not cleanup
- keep the render loop deterministic and non-blocking
- ship in thin vertical slices with runnable checkpoints
- delay `gh` extension packaging until the standalone binary is operational

## 3. Implementation Phases

## Phase 0: Repository Bootstrap

### Goals

- create a healthy Rust workspace
- lock toolchain and baseline quality gates
- make local iteration fast
- establish durable fixtures for UI-driven development

### Tasks

- initialize Cargo binary project
- add `rust-toolchain.toml`
- add dependencies:
  - `ratatui`
  - `crossterm`
  - `tokio`
  - `clap`
  - `reqwest`
  - `serde`
  - `serde_json`
  - `toml`
  - `directories`
  - `anyhow` or `thiserror`
  - `tracing` and `tracing-subscriber`
- create module layout from the spec
- add `Makefile` or `justfile` for common commands
- add CI workflow for:
  - `cargo fmt --check`
  - `cargo clippy -- -D warnings`
  - `cargo test`
- add initial docs index in `docs/`
- add a checked-in `fixtures/` directory for realistic JSON payloads
- capture representative fixture payloads for:
  - queued workflow run
  - running workflow run with several jobs
  - failed workflow run with job/step failure
  - PR requesting review
  - PR with mixed CI states

### Deliverables

- compilable app skeleton
- CI running on every push and PR
- repo ready for incremental development
- baseline fixtures ready for mock UI and future tests

### Exit Criteria

- `cargo run -- --help` works
- CI is green
- empty app boots and exits cleanly
- realistic fixtures are available for mock and test use

## Phase 1: App Shell and Mocked UX

### Goals

- prove the TUI architecture before live GitHub calls
- establish layout, focus, selection, and refresh loop behavior

### Tasks

- implement app event loop
- implement terminal init/restore lifecycle
- implement panic/signal-safe terminal restoration
- implement:
  - compact mode
  - split mode
  - status bar
  - help overlay
- add keyboard handling:
  - quit
  - refresh
  - focus switch
  - selection movement
  - drill-down open/close
- create mock repositories, PRs, runs, and run-detail jobs
- implement redraw cadence separate from network cadence
- implement width-aware truncation and resize handling
- drive mock views from checked-in fixtures rather than ad hoc inline data
- add a minimal snapshot/render baseline for the core mock screens

### Deliverables

- fully interactive mock TUI
- layout stable at narrow and normal widths
- no network dependency yet

### Exit Criteria

- app is usable with mock data only
- selection and focus feel correct
- no visible flicker during redraws
- mock views are driven by checked-in fixtures
- terminal state restores correctly on normal exit and interrupted exit

## Phase 2: Config, Auth, and Runtime State

### Goals

- load real user settings
- resolve GitHub auth safely
- prepare state store for live updates

### Tasks

- implement config file loader
- implement env var overrides
- implement CLI override precedence
- validate repo identifiers and host values
- implement auth resolution order:
  - `PRISM_TOKEN`
  - config token reference
  - `gh auth token`
- implement startup diagnostics for auth status
- implement browser abstraction and `--open-command` handling
- implement state store with:
  - snapshot timestamps
  - stale markers
  - last error
  - rate-limit state
  - focused pane
  - selected rows
  - identity-stable selected entities
  - refresh anchoring rules

### Deliverables

- config-aware app shell
- auth status command
- state store ready for live network updates
- browser-open behavior normalized before live views multiply

### Exit Criteria

- `prism auth status` reports source and host
- invalid config fails clearly
- app can start with a user config and no live fetches yet
- browser-open resolution works with default behavior and explicit override

## Phase 3: Actions List Data Plane

### Goals

- make the Actions pane live first
- prove polling, caching, and stale-state behavior in the simpler read path

### Tasks

- build shared GitHub HTTP client
- add REST endpoint support for:
  - workflow runs list
- implement run summary mapping
- implement refresh scheduler
- implement adaptive polling tiers
- define and document the tested v1 support envelope for repo count vs polling interval
  - initial planning target: 5-10 repos at 10s polling
- add a warning/degradation path when configured repo count exceeds the tested envelope
- capture and store response headers:
  - ETag
  - rate limit values
  - retry-after
- implement conditional requests with `If-None-Match`
- render live run summaries into the Actions pane
- implement local elapsed timers for in-progress runs
- implement stale markers on fetch failure

### Deliverables

- live Actions list for configured repos
- visible rate-limit state in status bar
- stable polling behavior
- defined support envelope for expected repo counts

### Exit Criteria

- configured repos show current runs
- in-progress durations update locally
- API responses are cached and reused correctly on 304
- selection remains anchored to the same run across refresh-induced reordering

## Phase 4: Actions Drill-down

### Goals

- let the user inspect a selected run without leaving Prism
- expose jobs and step progress clearly

### Tasks

- add REST endpoint support for:
  - jobs for workflow run
- implement on-demand detail fetch for selected run
- add bounded detail polling only while detail view is open
- map job steps into:
  - total steps
  - completed steps
  - failed step name
- define trusted-progress rules and indeterminate fallback behavior
- render detail overlay or detail pane
- implement progress bars:
  - run-level jobs completed / total jobs
  - job-level steps completed / total steps
- group failed jobs first, then running, then queued, then completed
- add browser open for selected run/job

Explicit non-scope for this phase:

- no log fetching
- no per-step expansion UI
- no rerun/cancel controls

### Deliverables

- working run drill-down
- compact progress indicators
- live detail refresh while a run is active
- intentionally capped detail scope
- determinate vs indeterminate progress behavior defined

### Exit Criteria

- user can move from run list to job detail and back smoothly
- failing and running jobs are obvious
- detail polling shuts off when detail closes
- no log-viewer behaviors have leaked into the scope
- progress indicators do not imply false precision when step data is ambiguous

## Phase 5: GraphQL Spike for PR Data

### Goals

- prove the GitHub GraphQL query shape for PR review and CI rollups
- de-risk the hardest API mapping before PR pane implementation

### Tasks

- define one real GraphQL query against representative repositories
- verify behavior for:
  - draft PR
  - requested review
  - approved PR
  - changes requested
  - passing CI
  - failing CI
  - pending CI
- document awkward or missing fields
- confirm a stable internal mapping for rollups

### Deliverables

- validated query shape
- mapping notes for review decision and CI rollup
- explicit go/no-go decision for full PR pane work
- fallback decision recorded if the spike shows full PR scope is too costly or too messy for v1

### Exit Criteria

- real repository data proves the query is viable
- internal state mapping is defined without hand-wavy placeholders

## Phase 6: Pull Request Data Plane

### Goals

- add the second major pane
- make PR status useful without overwhelming detail

### Tasks

- define GraphQL query for repo-scoped PR summary data
- batch repo PR requests conservatively
- map GraphQL fields into:
  - draft state
  - review decision
  - requested review for viewer
  - CI rollup
- implement requested-review highlight
- implement row open in browser
- render PR table with compact truncation rules

### Deliverables

- live PR pane
- review and CI rollups visible
- selected PR opens correctly in browser

### Exit Criteria

- PR pane is useful across multiple repos
- review-requested rows are immediately visible
- CI rollups map correctly for pass/fail/pending/skipped/unknown

## Phase 7a: Reliability Hardening

### Goals

- make Prism reliable enough for daily use

### Tasks

- implement log file output
- redact secrets in all diagnostics
- add empty states and missing-permission states
- add error banner or subtle status-line alerting
- tune polling defaults based on manual runs
- verify stale-state behavior after repeated transient failures
- verify long-running session behavior over at least 1 hour

### Deliverables

- safer error handling
- stable long-running sessions

### Exit Criteria

- app remains stable over 1-hour run
- stale-state and recovery behavior are correct under failure

## Phase 7b: Visual Polish and Theme Validation

### Goals

- finish terminal-native visual behavior without destabilizing the core app

### Tasks

- add terminal-theme-first palette logic
- add ASCII fallback mode
- refine status markers and spacing
- optimize redraw behavior under rapid updates
- run explicit theme validation matrix:
  - dark terminal profile
  - light terminal profile
  - limited-color terminal
  - ASCII-only mode

### Deliverables

- polished operator-facing UI
- validated theme inheritance behavior

### Exit Criteria

- no broken UI at target widths
- theme inheritance looks correct across the validation matrix

## Phase 8: Testing Depth

### Goals

- turn manual confidence into repeatable coverage

### Tasks

- add unit tests for:
  - state mapping
  - status reduction
  - config precedence
  - backoff policy
  - progress calculation
- add render snapshot tests for:
  - compact mode
  - split mode
  - detail view
  - stale/error states
- add integration tests with mocked GitHub responses:
  - normal list refresh
  - 304 not modified
  - 403/429 rate-limit path
  - partial repo failure
  - drill-down job payload

### Deliverables

- test suite covering critical behavior

### Exit Criteria

- tests catch regressions in state mapping and rendering
- mocked network suite is stable in CI

## Phase 9: Packaging and Release

### Goals

- make Prism installable
- release the standalone binary cleanly for v1

### Tasks

- add release workflow for macOS and Linux binaries
- add checksum generation
- add install docs
- optionally add Homebrew tap or formula
- verify `gh`-based auth bootstrap works in packaged builds

### Deliverables

- tagged downloadable releases
- documented install path

### Exit Criteria

- fresh install works on macOS
- Linux smoke validation passes
- standalone binary works without `gh`

## 4. Cross-Cutting Engineering Tasks

These span multiple phases and should not be deferred to the end.

### 4.1 Observability

- structured logs
- debug toggle
- optional request timing logs

### 4.2 Performance

- bounded request concurrency
- bounded UI tick rate
- cached formatting for stable rows where useful

### 4.3 Compatibility

- macOS first
- Linux validation before first public release
- GitHub Enterprise host validation before v1.1 unless required earlier

## 5. Working Order

Recommended real execution order:

1. Phase 0
2. Phase 1
3. Phase 2
4. Phase 3
5. Phase 4
6. Phase 5
7. Phase 6
8. Phase 7a
9. Phase 7b
10. Phase 8
11. Phase 9

Do not start Phase 6 before Phase 5 proves the GraphQL shape against real repositories. The Actions path remains the better proving ground for live polling and stale-state handling.

## 6. Release Definition for v1

Prism v1 is done when all of the following are true:

- config-selected repos load correctly
- Actions list updates live with adaptive polling
- run drill-down shows jobs and step progress
- PR pane shows review and CI rollups
- browser open works
- rate-limit pressure degrades gracefully
- UI is compact and readable in a terminal split
- release binaries are published for macOS
- Linux smoke validation passes

## 7. Deferred Work

Not required before first usable release:

- filters and search prompt
- saved repo groups beyond static config
- notifications
- issue monitoring
- inline logs
- Windows support
- advanced theming controls
- writable GitHub operations
- `gh prism` extension packaging

## 8. Known Risks to Watch During Execution

- GraphQL query shape for CI rollups may be more awkward than the spec implies
- Actions job step data may vary between workflow types and edge cases
- detail polling can quietly double API load if left unconstrained
- narrow-width layout may regress as detail views become denser
- `gh` extension packaging can become a distraction before the core app is stable
