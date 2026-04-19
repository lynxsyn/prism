# Prism Adversarial Review Round 2

This round reviews the revised execution plan after the first set of fixes were folded in.

## Findings

### 1. High: Live list reordering can destroy usability if selection is not identity-stable

Reference: [docs/EXECUTION_PLAN.md](/Users/lynx/projects/prism/docs/EXECUTION_PLAN.md:140), [docs/EXECUTION_PLAN.md](/Users/lynx/projects/prism/docs/EXECUTION_PLAN.md:182)

The plan tracks `selected rows`, but it does not define what happens when polling inserts a newer run or reorders the list while the user is focused on an item.

Why this is risky:

- a dashboard that reorders under the cursor becomes frustrating fast
- drill-down can accidentally open the wrong run if selection is index-based
- live polling without selection anchoring creates “UI jump” behavior that users interpret as instability

Recommendation:

- define selection by stable entity ID, not row index
- preserve the selected entity across refreshes when it still exists
- if the selected entity disappears, fall back predictably to nearest neighbor
- do not steal focus when new runs arrive

### 2. Medium: The plan still lacks a stated support envelope for repo count versus polling interval

Reference: [docs/EXECUTION_PLAN.md](/Users/lynx/projects/prism/docs/EXECUTION_PLAN.md:166), [docs/EXECUTION_PLAN.md](/Users/lynx/projects/prism/docs/EXECUTION_PLAN.md:433)

The plan has adaptive polling, but it never states what scale Prism is actually targeting in v1.

Why this matters:

- “multi-repo” is too vague
- one user may mean 3 repos, another may mean 40
- without a tested envelope, performance and rate-limit behavior will be judged against undefined expectations

Recommendation:

- define a v1 tested support envelope, for example:
  - comfortably supports 5-10 repos at 10s polling
  - degrades gracefully beyond that
- add a warning path when configured repo count materially exceeds the tested envelope

### 3. Medium: Progress bars still risk communicating false precision

Reference: [docs/EXECUTION_PLAN.md](/Users/lynx/projects/prism/docs/EXECUTION_PLAN.md:211)

The plan computes progress from steps and jobs, but does not explicitly say when progress must fall back to indeterminate state.

Why this is risky:

- GitHub job step lists include setup/cleanup/post steps that distort naive progress percentages
- skipped steps and matrix expansion can make “80% done” misleading
- a wrong progress bar is worse than no progress bar

Recommendation:

- define trusted-progress rules in the execution plan:
  - use determinate bars only when total/completed counts are clearly derivable
  - otherwise show running/indeterminate state
- test this against at least one workflow with post steps and one with skipped steps

### 4. Medium: Snapshot and render testing are still introduced too late

Reference: [docs/EXECUTION_PLAN.md](/Users/lynx/projects/prism/docs/EXECUTION_PLAN.md:360)

The plan moves fixtures earlier, which is good, but snapshot testing is still deferred until Phase 8.

Why this matters:

- UI layout regressions usually begin during the first working TUI stages
- if snapshots arrive late, visual debt accumulates
- progress/detail layouts are precisely the kind of output that benefits from early snapshots

Recommendation:

- add a minimal snapshot harness in Phase 1 or 3
- Phase 8 can deepen coverage, but initial render baselines should exist earlier

### 5. Low: Terminal restoration still is not called out as an explicit exit criterion

Reference: [docs/EXECUTION_PLAN.md](/Users/lynx/projects/prism/docs/EXECUTION_PLAN.md:70)

The plan mentions terminal lifecycle, but not panic/signal-safe restoration as a hard requirement.

Why this matters:

- broken terminal state ruins trust instantly
- this is a classic TUI failure mode

Recommendation:

- add explicit panic/signal-safe terminal restoration to Phase 1 exit criteria

## Required Adjustments Before Round 3

1. Add identity-stable selection and refresh anchoring rules.
2. Define a tested repo-count support envelope for v1.
3. Add indeterminate-progress fallback rules to Phase 4.
4. Move a minimal snapshot baseline earlier than Phase 8.
5. Make terminal restoration a hard exit criterion in the app shell phase.

## Interim Verdict

The plan is materially better than round 1. Remaining risk is now mostly about operator trust under live updates and whether the app looks stable while data changes underneath it.
