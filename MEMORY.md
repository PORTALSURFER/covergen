# MEMORY.md

Last Updated: 2026-02-25 12:50:24 UTC

## Current Mission
Current work is focused on building a TouchDesigner-style GUI and node-graph generative art generator with top-tier real-time and export performance.

## Current State
- `covergen` is V2-only and launches the GUI preview by default.
- Platform scope is Windows-first for this phase; cross-platform portability is deferred.
- Runtime and benchmark paths require a hardware GPU; software adapters/CPU fallback are rejected.
- Core feature scope follows TouchDesigner-style capabilities and will be expanded iteratively.
- Interactive TOP-viewer performance target is a minimum of 60 FPS at 1080p, with higher idle headroom as a design target.
- Export scope is currently H.264 and image sequences.
- Export architecture target is fully GPU-accelerated workflows, including zero-readback paths where feasible.
- Current priority order is core stability, performance, and core features before extensibility.
- Housekeeping preflight now runs through `scripts/run_agent_request.sh`.
- `scripts/ci_local.sh` supports no-arg execution and defaults to `validate laptop_integrated`.
- Rust-gpu shader artifacts are validated/built through the existing `scripts/shaders/*` flows.

## Active Queue
Immediate ordered tasks are in `docs/plans/active/todo.md`.

## Current Risks
- Node-editor and TOP-viewer responsiveness can regress under larger graph sizes.
- GPU export pipeline complexity can delay stable H.264 throughput targets.
- Warning volume in checks is still high and can mask meaningful regressions.

## Working Assumptions
- Windows is the only supported runtime platform for the current delivery phase.
- Systems without hardware GPU support are out of scope.
- Interactive playback prioritizes sustained frame pacing; export mode can prioritize throughput.
- Handoff docs (`AGENTS.md`, `MEMORY.md`, `docs/plans/active/todo.md`) must stay synchronized.
