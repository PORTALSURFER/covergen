# MEMORY.md

Last Updated: 2026-02-25 12:20:16 UTC

## Current Mission
The project is running V2-only. Current work is focused on local CI/perf-baseline reliability, GPU-required runtime stability, and responsive GUI node-editor iteration.

## Current State
- `covergen` is V2-only and launches the GUI preview by default.
- Runtime and benchmark paths require a hardware GPU; software adapters/CPU fallback are rejected.
- Local CI is authoritative (`scripts/ci_local.sh` / `scripts/ci_local.ps1`).
- Housekeeping preflight now runs through `scripts/run_agent_request.sh`.
- `scripts/ci_local.sh` supports no-arg execution and defaults to `validate laptop_integrated`.
- Rust-gpu shader artifacts are validated/built through the existing `scripts/shaders/*` flows.

## Active Queue
Immediate ordered tasks are in `docs/plans/active/todo.md`.

## Current Risks
- `desktop_mid` tier thresholds remain deferred until desktop hardware is available.
- Warning volume in CI is still high and can mask meaningful regressions.

## Working Assumptions
- V2 remains the only supported runtime path.
- Local hardware-tier validation is the release gate.
- Handoff docs (`AGENTS.md`, `MEMORY.md`, `docs/plans/active/todo.md`) must stay synchronized.
