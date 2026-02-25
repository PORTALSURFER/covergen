# MEMORY.md

Last Updated: 2026-02-25 15:28:00 UTC

## Current Mission
Current work is focused on building a Windows-first, GPU-only shader/video playground with a high-performance node graph editor, real-time output, and fast export.

## Current State
- `covergen` is V2-only and launches the GUI preview by default.
- Platform scope is Windows-first for this phase; cross-platform portability is deferred.
- Runtime and benchmark paths require a hardware GPU; software adapters/CPU fallback are rejected.
- Baseline performance tier is NVIDIA GeForce RTX 2060; target users are high-tier gaming GPUs or better.
- Canonical design contract is `docs/v2/engine-v1-playground.md`.
- Core feature scope tracks leading node-based real-time tools at a capability level while maintaining legal separation through original architecture and naming.
- Engine model is now explicitly based on `ResourceKind + ExecutionKind + ClockDomain`.
- System behavior is always real-time with a user-selected target frame rate (typically 60 FPS).
- Interactive TOP-viewer target is at least 60 FPS at 1080p, with meaningful idle headroom above target in low-complexity scenes.
- Idle headroom gate (RTX 2060, 1080p, empty/light scene): primary `p95 frametime <= 10 ms`, secondary `average FPS >= 90`.
- Node-network complexity should be the dominant bottleneck; typical deep graphs must remain responsive and keep stable frame pacing.
- Export scope is currently H.264 and image sequences.
- Export architecture target is fully GPU-accelerated workflows, including zero-readback paths where feasible.
- Windows H.264 implementation order is NVENC first, then AMF.
- Current priority order is core stability, performance, and core features before extensibility.
- Housekeeping preflight now runs through `scripts/run_agent_request.sh`.
- `scripts/ci_local.sh` supports no-arg execution and defaults to `validate laptop_integrated`.
- Rust-gpu shader artifacts are validated/built through the existing `scripts/shaders/*` flows.

## Active Queue
Immediate ordered tasks are in `docs/plans/active/todo.md`.

## Current Risks
- Node-editor and TOP-viewer responsiveness can regress under larger graph sizes and violate frame-pacing targets.
- GPU export pipeline complexity can delay stable H.264 throughput targets.
- Warning volume in checks is still high and can mask meaningful regressions.

## Working Assumptions
- Windows is the only supported runtime platform for the current delivery phase.
- Systems without hardware GPU support are out of scope.
- Interactive playback prioritizes sustained frame pacing around user-selected target FPS; export mode can prioritize throughput.
- Product behavior and performance priorities should track best-in-class real-time node-workflow expectations while preserving legal and architectural separation.
- Handoff docs (`AGENTS.md`, `MEMORY.md`, `docs/plans/active/todo.md`) must stay synchronized.
