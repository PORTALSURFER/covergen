# MEMORY.md

Last Updated: 2026-02-24 07:37:36 UTC

## Current Mission
The team is operating on a V2-only runtime path, hardening GPU/perf guardrails, and closing remaining migration follow-through work.

## Current State
- V2 is the only user-facing CLI runtime path (`covergen` and `covergen v2 ...`).
- `covergen v1` was removed from CLI dispatch on 2026-02-24 and now returns a deprecation error.
- V1 rendering code remains in-repo only for internal benchmark/comparison workflows.
- V2 includes:
  - graph IR + compiler (`src/v2/graph.rs`, `src/v2/compiler.rs`)
  - deterministic presets (`src/v2/presets/*`)
  - GPU-retained runtime (`src/v2/runtime.rs`)
  - 30s reels animation mode with gentle parameter modulation (`src/v2/animation.rs`)
- Benchmark + visual regression CI gates run from `.github/workflows/perf-gates.yml`.
- CI software-tier benchmark thresholds are locked at `.github/bench/ci_software.thresholds.ini`.

## Active Queue
The immediate ordered queue is maintained in `docs/plans/active/todo.md`.

## Risks and Gaps (Current)
- Target-hardware benchmark thresholds outside CI software tier are not yet locked.
- Visual regression coverage should continue expanding for larger outputs and additional preset families.
- Multi-output/tap graph contracts need dedicated benchmark + artifact policy coverage.

## Working Assumptions
- V2 is the long-term and only supported user runtime path.
- Bench-only V1 comparisons are internal diagnostics, not a supported user mode.
- Documentation-driven handoff is required for stateless agent wake-up.
