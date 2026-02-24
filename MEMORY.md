# MEMORY.md

Last Updated: 2026-02-24 10:48:48 UTC

## Current Mission
The team is operating on a V2-only runtime path, hardening GPU/perf guardrails, and closing remaining migration follow-through work.

## Current State
- V2 is the only user-facing CLI runtime path (`covergen` and `covergen v2 ...`).
- V1 runtime code and CLI path are removed from source.
- V2 includes:
  - graph IR + compiler (`src/v2/graph.rs`, `src/v2/compiler.rs`)
  - deterministic presets (`src/v2/presets/*`)
  - GPU-retained runtime (`src/v2/runtime.rs`)
  - 30s reels animation mode with gentle parameter modulation (`src/v2/animation.rs`)
- Hardware GPU is now a hard runtime requirement for both `covergen` and `covergen bench`; software adapters/CPU fallback are rejected with an immediate process error.
- Benchmark + visual regression CI gates run from `.github/workflows/perf-gates.yml`.
- CI software-tier benchmark thresholds are locked at `.github/bench/ci_software.thresholds.ini`.
- Visual regression coverage was expanded (larger still sizes, additional sampled animation frames, and broader GPU-path confidence assertions).
- Benchmark lock flow now refuses to write threshold files when required V2 scenarios are missing.
- Benchmark and regression suites now validate multi-output contracts (exactly one primary + at least one tap) and follow a documented primary-only encode artifact policy.
- Hardware-tier readiness now has a single script entrypoint (`scripts/bench/hardware_gate_readiness.sh`) that checks runner labels, threshold lock state, and manages `COVERGEN_ENABLE_HARDWARE_TIER_GATES`.
- Hardware benchmark and GPU regression CI jobs now validate rust-gpu SPIR-V artifacts before running workload/test commands.

## Active Queue
The immediate ordered queue is maintained in `docs/plans/active/todo.md`.

## Risks and Gaps (Current)
- Target-hardware benchmark thresholds outside CI software tier are not yet locked because self-hosted tier runners are not currently registered (`total_count=0` from Actions runners API at last check).
- Multi-output/tap graph contracts need dedicated benchmark + artifact policy coverage.

## Working Assumptions
- V2 is the long-term and only supported user runtime path.
- Bench and runtime execution paths are V2-only.
- Documentation-driven handoff is required for stateless agent wake-up.
