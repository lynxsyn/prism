# Prism Adversarial Review

This review attacks the execution plan as if the goal were to expose where it is most likely to fail in delivery, UX, or operational behavior.

## Findings

### 1. High: PR data is still scheduled too early relative to API-shape uncertainty

Reference: [docs/EXECUTION_PLAN.md](/Users/lynx/projects/prism/docs/EXECUTION_PLAN.md:135)

Phase 5 assumes the GraphQL path for PR review and CI rollups will be straightforward once the Actions path is working. That is optimistic.

Why this is risky:

- `statusCheckRollup` is structurally more awkward than the plan budget suggests
- PR review state and CI rollup are the most likely source of mapping bugs
- if this becomes messy, it can stall the mainline after substantial UI work is already done

Recommendation:

- add an explicit spike before full Phase 5 implementation
- the spike should prove one GraphQL query shape against real repositories before PR pane completion is considered in scope

### 2. High: Actions drill-down risks turning into an accidental log viewer

Reference: [docs/EXECUTION_PLAN.md](/Users/lynx/projects/prism/docs/EXECUTION_PLAN.md:108)

The plan says drill-down is not a log viewer, but it still adds multiple detail behaviors in one phase.

Why this is risky:

- once jobs and steps are visible, pressure to add logs, retries, or richer step detail will grow
- dense detail rendering is where compact layout discipline usually collapses
- this is the easiest place for scope creep to enter under the label of “just one more useful detail”

Recommendation:

- hard-cap Phase 4 output to job summaries plus one failed-step label
- forbid log fetching and forbid per-step expansion in v1

### 3. High: Phase 6 mixes reliability work with cosmetic polish

Reference: [docs/EXECUTION_PLAN.md](/Users/lynx/projects/prism/docs/EXECUTION_PLAN.md:163)

Hardening and visual polish are combined into one phase. That is a delivery smell.

Why this is risky:

- reliability tasks compete badly with aesthetic tweaks
- bugs in stale-state, resizing, and long-running stability are more important than theme refinement
- “polish” phases often become catch-alls and slip indefinitely

Recommendation:

- split Phase 6 into:
  - 6a reliability hardening
  - 6b visual polish
- do not start 6b until 6a passes a manual long-run check

### 4. Medium: Phase 0 is missing fixture discipline for UI-driven development

Reference: [docs/EXECUTION_PLAN.md](/Users/lynx/projects/prism/docs/EXECUTION_PLAN.md:18)

The plan says mock data will be used in Phase 1, but it does not explicitly establish checked-in fixtures as a first-class artifact.

Why this matters:

- TUIs regress visually in subtle ways
- if mock payloads are ad hoc, snapshot and integration tests will drift
- realistic failing/running payloads are required to test compact detail layouts

Recommendation:

- add a `fixtures/` directory in Phase 0 or Phase 1
- capture representative JSON for:
  - queued run
  - running run with several jobs
  - failed run with step failure
  - PR with requested review
  - PR with mixed CI states

### 5. Medium: Packaging is correctly deferred, but not aggressively enough

Reference: [docs/EXECUTION_PLAN.md](/Users/lynx/projects/prism/docs/EXECUTION_PLAN.md:219)

The plan defers packaging well, but still treats optional `gh` integration as part of the same final phase.

Why this matters:

- the standalone binary and `gh` extension wrapper have different release concerns
- the wrapper can steal attention late in the cycle when the app should be stabilizing

Recommendation:

- declare standalone binary release as the only v1 requirement
- move `gh prism` wrapper to v1.1 unless it is nearly free after release automation exists

### 6. Medium: Manual validation criteria are under-specified for terminal-theme inheritance

Reference: [docs/EXECUTION_PLAN.md](/Users/lynx/projects/prism/docs/EXECUTION_PLAN.md:163)

The plan says theme inheritance should look correct in common terminal profiles, but gives no validation matrix.

Why this matters:

- “looks correct” is too subjective
- contrast failures are easy to miss if testing happens in only one terminal profile

Recommendation:

- define a small manual matrix:
  - dark theme terminal
  - light theme terminal
  - limited-color terminal
  - ASCII-only mode

### 7. Medium: Browser-open behavior is specified, but not normalized early enough

Reference: [docs/EXECUTION_PLAN.md](/Users/lynx/projects/prism/docs/EXECUTION_PLAN.md:108)

Opening the selected PR or run sounds simple, but cross-platform behavior and user override behavior often create annoying bugs.

Recommendation:

- move `--open-command` and browser abstraction into Phase 2 or 3
- prove it before the app has many view types

### 8. Low: Linux support is mentioned but not given a gating moment

Reference: [docs/EXECUTION_PLAN.md](/Users/lynx/projects/prism/docs/EXECUTION_PLAN.md:255)

The plan says Linux validation should happen before first public release, but it is not attached to a concrete phase exit.

Recommendation:

- add Linux smoke validation to Phase 8 exit criteria

## Required Adjustments Before Execution

1. Add a dedicated GraphQL spike before full PR pane implementation.
2. Split hardening from visual polish.
3. Make checked-in realistic fixtures part of the early plan.
4. Treat standalone packaging as v1 and `gh prism` wrapper as v1.1 unless trivial.
5. Add explicit theme-validation and browser-open validation checkpoints.

## Bottom Line

The plan is directionally strong. Its main weakness is not architecture; it is delivery shape.

If execution starts without the adjustments above, the most likely failure mode is not technical impossibility. It is scope drag in the Actions detail view plus underestimating PR GraphQL mapping complexity.
