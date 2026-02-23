# MEMORY.md

Last Updated: 2026-02-23 22:27:01 UTC

## Current Mission
The team is operating on the V2-default runtime, validating cutover guardrails, and running the announced V1 deprecation window while completing remaining GPU/perf hardening.

## Current State
- V2 is the default CLI path (`covergen` and `covergen v2 ...`).
- V1 is available only via explicit legacy mode (`covergen v1`) during the deprecation window (2026-02-23 to 2026-05-24).
- V2 includes:
  - graph IR + compiler (`src/v2/graph.rs`, `src/v2/compiler.rs`)
  - deterministic presets (`src/v2/presets.rs`)
  - GPU-retained runtime (`src/v2/runtime.rs`)
  - 30s reels animation mode with gentle parameter modulation (`src/v2/animation.rs`)
- Benchmark + visual regression CI gates now run from `.github/workflows/perf-gates.yml`.
- CI software-tier benchmark thresholds are locked at `.github/bench/ci_software.thresholds.ini`.

## Active Queue
The immediate ordered queue is maintained in `docs/plans/active/todo.md`.

## Risks and Gaps (Current)
- Target-hardware benchmark thresholds outside the CI software tier are not yet locked.
- Visual regression coverage should continue expanding for larger outputs and additional preset families.
- V1 removal criteria still depend on deprecation-window stability.

## Working Assumptions
- V2 is the long-term default target.
- V1 remains available only during the active deprecation window.
- Documentation-driven handoff is required for stateless agent wake-up.
