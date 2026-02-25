# V2 Architecture

## Purpose

Define the engine-centric architecture for a Windows-first, GPU-required node editor focused on real-time shader/video workflows.

The canonical model is documented in [`engine-v1-playground.md`](./engine-v1-playground.md). This document maps that model to project implementation boundaries.

## Product Constraints

- Windows-first runtime support.
- No CPU/software adapter fallback.
- Real-time first: user-selected target FPS (60 FPS baseline), with meaningful idle headroom.
- Export scope in this phase: H.264 and image sequences.

## Architectural Axes

The system uses three explicit classifications:

- `ResourceKind`: typed payloads that flow over ports (`Texture2D`, `Struct` in V1).
- `ExecutionKind`: node runtime behavior (`Render`, `Compute`, `Cpu`, `Io`, `Control`).
- `ClockDomain`: scheduling cadence (`FrameClock`, `EventClock`).

This replaces family-style operator taxonomy.

## Planes

### GPU Data Plane

- Texture allocation/reuse.
- Render/compute pass execution.
- Shader compilation/pipeline management.
- Swapchain presentation.

### CPU Control Plane

- Graph editing/validation.
- Dirty/version propagation.
- Scheduling and topological planning.
- Parameter/event updates.

Bridges between planes are explicit (`Struct -> ParamBlock`).

## Runtime Pipeline

1. Determine requested outputs (`io.window_out`, export sinks).
2. Build pull-set of required upstream nodes.
3. Resolve deterministic topological order.
4. Execute nodes by dependency and `ExecutionKind`.
5. Reuse transient textures via pool.
6. Submit command buffer(s), then present/export.

## Error and Stability Behavior

- Shader compile errors never crash frame execution.
- `tex.shader` keeps last known-good pipeline on compile failure.
- Error state is surfaced in UI diagnostics.
- Adapter policy is strict GPU-only fail-fast.

## Migration Direction

The repository still contains legacy graph/runtime naming from prior presets. Migration now prioritizes:

1. Type-system migration from legacy port families to `ResourceKind`.
2. Scheduler migration to explicit `ExecutionKind + ClockDomain`.
3. Node registry migration to namespace IDs (`io.*`, `tex.*`, `ctl.*`, `data.*`).

See [`graph-spec.md`](./graph-spec.md) and [`gpu-runtime.md`](./gpu-runtime.md) for concrete contracts.
