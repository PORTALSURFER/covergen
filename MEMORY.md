# MEMORY.md

Last Updated: 2026-02-24 11:31:11 UTC

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
- Benchmark + visual regression gates are local-first via `scripts/ci_local.sh`.
- CI software-tier benchmark thresholds at `.github/bench/ci_software.thresholds.ini` are legacy/non-authoritative.
- Legacy GitHub hardware-gate workflow jobs were removed; hardware validation is local-only on provisioned GPU tier hosts.
- Visual regression coverage was expanded (larger still sizes, additional sampled animation frames, and broader GPU-path confidence assertions).
- Benchmark lock flow now refuses to write threshold files when required V2 scenarios are missing.
- Benchmark and regression suites now validate multi-output contracts (exactly one primary + at least one tap) and follow a documented primary-only encode artifact policy.
- Local CI script (`scripts/ci_local.sh`) now defines the canonical gate sequence: shader validation, fmt/test, visual regression, GPU confidence tests, and tier threshold benchmark validation.
- Windows hosts now have PowerShell rust-gpu artifact instrumentation scripts (`scripts/shaders/build_rust_gpu_artifacts.ps1`, `scripts/shaders/validate_rust_gpu_artifacts.ps1`) for build/validation parity.
- Windows hosts now have a PowerShell tier threshold gate script (`scripts/bench/tier_gate.ps1`) for lock/validate parity with `scripts/bench/tier_gate.sh`.
- Windows hosts now also have PowerShell local CI and handoff capture scripts (`scripts/ci_local.ps1`, `scripts/bench/store_handoff_artifacts.ps1`) for full local-gate and artifact retention flow.
- Shader artifacts now have an in-repo build path via `cargo run --quiet --bin build_spirv` (wrapped by `scripts/shaders/build_rust_gpu_artifacts.sh` and `.ps1`), and local CI auto-builds them when missing.

## Active Queue
The immediate ordered queue is maintained in `docs/plans/active/todo.md`.

## Risks and Gaps (Current)
- Target-hardware benchmark thresholds outside legacy CI software tier are not yet locked because required local tier hosts are not yet provisioned.
- Multi-output/tap graph contracts need dedicated benchmark + artifact policy coverage.

## Working Assumptions
- V2 is the long-term and only supported user runtime path.
- Bench and runtime execution paths are V2-only.
- Documentation-driven handoff is required for stateless agent wake-up.
