# MEMORY.md

Last Updated: 2026-02-23 14:16:11 UTC

## Current Mission
The team is stabilizing V2 as the future default rendering path: programmatic node-graph generation, GPU-retained execution, and animation support for reels.

## Current State
- V1 remains the default CLI path (`covergen`).
- V2 is available via `covergen v2 ...` with:
  - graph IR + compiler (`src/v2/graph.rs`, `src/v2/compiler.rs`)
  - deterministic presets (`src/v2/presets.rs`)
  - GPU-retained runtime (`src/v2/runtime.rs`)
  - 30s reels animation mode with gentle parameter modulation (`src/v2/animation.rs`)
- V2 docs exist under `docs/v2/`.

## Active Queue
The immediate ordered queue is maintained in `docs/plans/active/todo.md`.

## Risks and Gaps (Current)
- Runtime still treats graph execution as mostly linear in practice.
- Several post-process operations still rely on host-side steps.
- Benchmark and regression gates for V2 cutover are not fully established yet.

## Working Assumptions
- V2 is the long-term default target.
- V1 remains available until V2 performance/quality gates are met.
- Documentation-driven handoff is required for stateless agent wake-up.
