# V2 GPU Runtime

## Runtime Intent

Execute typed node graphs in real time with deterministic scheduling, zero software-adapter fallback, and high-throughput GPU pipelines for both interactive viewing and export.

## Adapter Policy

- Hardware GPU is mandatory.
- Software adapters (llvmpipe/swiftshader/WARP) fail fast with explicit errors.
- Baseline target hardware tier: RTX 2060-class or better.

## Evaluation Strategy

### Pull Scheduling

Each frame (or export step), runtime starts from requested sinks:

- interactive: `io.window_out`
- export: image-sequence and H.264 outputs

Only required upstream nodes are executed.

### Dirty Propagation

A node re-runs when:

- any input output-version changed, or
- its parameter hash changed, or
- shader pipeline changed.

Otherwise cached outputs are reused.

### Determinism

- deterministic topological order
- deterministic pass ordering and bind layout
- deterministic hash/version updates

## FrameClock Pipeline

1. Tick `ctl.time` (`t`, `dt`, `frame_index`).
2. Build required node set for current sinks.
3. Topologically order render plan.
4. Acquire transient render targets from `TexturePool`.
5. Execute render/compute passes.
6. Submit command buffers.
7. Present swapchain frame for viewer outputs.

Goal is one primary submission path per frame.

## EventClock Pipeline

Runs on explicit events, not each frame:

- file loads/saves
- parameter edits
- trigger actions

Event results update cached resources/versions consumed by the next frame pull.

## Shader Node Behavior

`tex.shader` contract:

- compile WGSL on source change
- if compile succeeds: swap in new pipeline
- if compile fails: keep previous valid pipeline active
- publish diagnostics for UI display

This keeps playback stable during live editing.

## Resource Pooling

`TexturePool` reuse key:

- width
- height
- format
- usage flags

Transient targets are reused across passes. Persistent targets are reserved for explicit stateful nodes (for example, planned feedback nodes).

## Export Runtime (V1 Scope)

Supported outputs:

- H.264
- image sequences

Policy:

- keep export on GPU path as far as possible
- prioritize throughput in export mode while preserving output correctness
- maintain responsive viewer path in interactive mode

Windows rollout order for H.264 backends:

1. NVENC
2. AMF

## Known Limits (V1)

- No arbitrary graph cycles.
- No audio/stream domain.
- No geometry domain.
- No implicit `Struct -> GPU` conversion.
