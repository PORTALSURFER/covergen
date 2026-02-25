# Engine Model V1: Shader/Video Playground

## 1. Purpose

Build a real-time, node-based system for GPU shader experimentation and video compositing.

The design target is a high-performance graph editor and viewer workflow comparable to leading node tools while staying legally distinct through original naming, architecture, and UX language.

### Goals

- Fast shader iteration with live WGSL editing.
- Strict resource typing across all ports.
- Deterministic frame evaluation.
- Minimal but high-leverage V1 node set.
- Clean separation between GPU data plane and CPU control plane.
- Windows-first runtime, GPU-required operation, and export support for H.264 plus image sequences.

### Non-goals (V1)

- Arbitrary graph cycles.
- Audio/sample-accurate processing.
- Geometry/mesh domains.
- Complex multi-view scene graphs.

## 2. Core Model

The engine is defined by two orthogonal axes:

- `ResourceKind`: what flows through a port.
- `ExecutionKind`: how a node runs.

Clocking is modeled separately by `ClockDomain`.

## 3. Resource Kinds

### 3.1 `Texture2D`

Represents one GPU 2D texture.

Descriptor fields:

- `width: u32`
- `height: u32`
- `format: TextureFormat` (`RGBA8Unorm`, `RGBA16Float` in V1)
- `usage: TextureUsageFlags`

Primary uses:

- Images and video frames.
- Intermediate render targets.
- Shader outputs.
- Masks.

### 3.2 `Struct`

CPU-side structured data for control and parameter flow.

Primary uses:

- Parameters.
- Tables.
- Events and trigger payloads.
- Parsed JSON/control signals.

`Struct` never binds to GPU implicitly. Any GPU use must pass through an explicit `data.params -> ParamBlock` bridge node.

### Deferred Domains

- `Buffer`
- `Stream`

## 4. Clock Domains

### 4.1 `FrameClock`

Ticks once per render frame.

Used by:

- Texture transforms.
- Shader nodes.
- Compositing and presentation.
- Time-based effects.

### 4.2 `EventClock`

Ticks on explicit non-frame events:

- UI edits.
- File loads.
- Network messages.
- Trigger actions.

Used by:

- Data parsing.
- Parameter updates.
- Save/export trigger nodes.

## 5. Execution Kinds

### 5.1 `Render`

A render pass that produces `Texture2D`.

### 5.2 `Compute` (optional in V1)

A compute pass that produces `Texture2D`.

### 5.3 `Cpu`

Pure CPU execution that produces `Struct` or upload-ready parameter payloads.

### 5.4 `Io`

Boundary nodes:

- image/video inputs
- swapchain output
- file export

### 5.5 `Control`

Scheduling and routing logic nodes.

## 6. Evaluation Model

### 6.1 Pull-Based Evaluation

Evaluation starts at requested outputs (for example, `io.window_out` or export targets) and executes only required upstream nodes.

### 6.2 Dirty Propagation

Every node output is versioned. Re-execute a node when:

- any input version changes, or
- parameter hash changes.

Otherwise reuse cached output.

### 6.3 No Implicit Conversions

`Struct` does not directly feed shader bindings. Explicit conversion is required:

- `data.params -> ParamBlock`
- `ParamBlock` is then bound by shader-capable nodes.

### 6.4 Cycles

Arbitrary cycles are out of scope in V1. Feedback is introduced only as an explicit delayed node in V1.1 (`tex.feedback`).

## 7. Node Registry (V1)

Stable IDs use namespace-style naming.

### 7.1 IO

- `io.window_out`: input `Texture2D`, `Io + FrameClock`, presents to swapchain.
- `io.image_load`: output `Texture2D`, `Io + EventClock`, params `filepath`, load-and-cache behavior.
- `io.image_save`: input `Texture2D`, `Io + EventClock`, params `filepath`, `trigger`.

### 7.2 Texture Sources

- `tex.solid`: output `Texture2D`, params `color(vec4)`, `resolution(vec2)`.
- `tex.noise`: output `Texture2D`, params `scale`, `seed`, `resolution`.

### 7.3 Core Transforms

- `tex.transform_2d`: in/out `Texture2D`, params `translate`, `rotate`, `scale`, `pivot`.
- `tex.blur_gauss`: in/out `Texture2D`, separable 2-pass render, params `radius`, `sigma`.
- `tex.levels`: in/out `Texture2D`, params `in_black`, `in_white`, `gamma`, `out_black`, `out_white`.
- `tex.hsv`: in/out `Texture2D`, params `hue_shift`, `saturation`, `value`.

### 7.4 Compositing

- `tex.mix`: inputs `A`, `B`, output `Texture2D`, params `blend`, `mode(normal|add|multiply|screen)`.
- `tex.mask`: inputs `base`, `layer`, `mask`, output `Texture2D`, params `mask_channel`, `invert`.
- `tex.compose_over`: inputs `under`, `over`, output `Texture2D`, param `premultiplied`.

### 7.5 Shader Core Node

- `tex.shader`:
  - Inputs: `0..N Texture2D`, optional `ParamBlock`.
  - Output: `Texture2D`.
  - Params: `wgsl_source`, `output_policy(match_input0|explicit|match_window)`.
  - Behavior:
    - compile WGSL on source change
    - keep last valid pipeline on compile failure
    - surface compile errors in UI

### 7.6 Control

- `ctl.time`: output `Struct`, fields `t`, `dt`, `frame_index`.
- `data.params`: output `ParamBlock`, editable parameter block (`EventClock`).
- `ctl.switch`: inputs `A`, `B`, `select(bool)`, output `Texture2D`.
- `ctl.latch`: inputs `Struct`, `trigger`, output `Struct`.

## 8. ParamBlock Contract

`ParamBlock` is an explicit GPU-safe layout with stable ABI.

Allowed field classes in V1:

- `f32`
- `vec2/vec3/vec4`
- `bool`
- `u32`
- small fixed-size arrays

Conversion from `Struct` to `ParamBlock` validates field compatibility and enforces deterministic layout.

WGSL binding shape example:

```wgsl
struct Params {
    time: f32,
    value: f32,
    color: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> params: Params;
```

## 9. Example Graphs

### Image Grading

`io.image_load -> tex.levels -> tex.hsv -> io.window_out`

### Shader Playground

`tex.noise -> tex.shader -> io.window_out`

`ctl.time -> data.params -> tex.shader`

### Masked Composite

`io.image_load(A) + io.image_load(B) + tex.noise(mask) -> tex.mask -> io.window_out`

## 10. Frame Pipeline

Per frame:

1. Update `ctl.time`.
2. Collect requested outputs.
3. Topologically sort required nodes.
4. Execute render/compute/cpu/io steps by dependency order.
5. Reuse transient targets via `TexturePool`.
6. Submit one command buffer for render graph work.
7. Present swapchain output.

## 11. Resource Pooling

`TexturePool` key:

- width
- height
- format
- usage flags

Transient render targets are aggressively reused. Persistent allocations are reserved for explicit feedback/persistent nodes.

## 12. Why This Design

- No borrowed family branding.
- Explicit memory model.
- Deterministic scheduling.
- Strong type contracts.
- Clear legal and architectural separation from existing tools.

## 13. Planned Extensions

### V1.1

- `tex.feedback` (1-frame delay)
- broader compute-node support
- first-pass video capture/export plumbing

### V2

- `Buffer` domain
- `Stream` domain (audio/control signals)
- multi-target render passes
- genlock-aware frame clocking
