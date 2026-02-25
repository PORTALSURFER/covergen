# MEMORY.md

Last Updated: 2026-02-25 12:43:08 UTC

## Current Mission
Current work is focused on responsive GUI node-editor iteration and keeping handoff documentation synchronized.

## Current State
- `covergen` is V2-only and launches the GUI preview by default.
- Runtime and benchmark paths require a hardware GPU; software adapters/CPU fallback are rejected.
- Housekeeping preflight now runs through `scripts/run_agent_request.sh`.
- `scripts/ci_local.sh` supports no-arg execution and defaults to `validate laptop_integrated`.
- Rust-gpu shader artifacts are validated/built through the existing `scripts/shaders/*` flows.

## Active Queue
Immediate ordered tasks are in `docs/plans/active/todo.md`.

## Current Risks
- Node-editor interactions can regress under larger graph sizes.
- Warning volume in checks is still high and can mask meaningful regressions.

## Working Assumptions
- GUI responsiveness improvements should preserve existing visual behavior.
- Handoff docs (`AGENTS.md`, `MEMORY.md`, `docs/plans/active/todo.md`) must stay synchronized.
