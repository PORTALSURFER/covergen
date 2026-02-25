# V2 Graph Specification

## 1. Scope

This specification defines the graph contracts for the engine-centric V1 model in [`engine-v1-playground.md`](./engine-v1-playground.md).

## 2. Core Types

- `NodeId`: stable node identifier.
- `PortId`: stable local port identifier.
- `ResourceKind`:
  - `Texture2D`
  - `Struct`
  - `ParamBlock`
- `ExecutionKind`:
  - `Render`
  - `Compute`
  - `Cpu`
  - `Io`
  - `Control`
- `ClockDomain`:
  - `FrameClock`
  - `EventClock`
- `NodeSpec`: registry entry describing ports, params, execution, and clock behavior.
- `EdgeSpec`: directed typed connection between source and destination ports.
- `GraphSpec`: immutable validated graph payload.

## 3. Graph-Level Contract

- Graph must be acyclic in V1.
- At least one output sink node must exist.
- `io.window_out` is optional for headless/export-only runs, but required for interactive viewer output.
- Each destination input port accepts at most one incoming edge.
- Source and destination `ResourceKind` must match exactly.
- No implicit conversions are allowed.

## 4. Explicit Conversion Contract

The only V1 bridge from control data to shader uniforms is:

`Struct -> ParamBlock` through `data.params`.

Any shader node requiring uniforms must receive a `ParamBlock` input explicitly.

## 5. Node Registry (V1)

Stable IDs and typed signatures:

### IO

- `io.window_out`
  - Inputs: `input0(Texture2D)`
  - Outputs: none
  - `ExecutionKind`: `Io`
  - `ClockDomain`: `FrameClock`
- `io.image_load`
  - Inputs: none
  - Outputs: `out(Texture2D)`
  - `ExecutionKind`: `Io`
  - `ClockDomain`: `EventClock`
  - Params: `filepath`
- `io.image_save`
  - Inputs: `input0(Texture2D)`
  - Outputs: none
  - `ExecutionKind`: `Io`
  - `ClockDomain`: `EventClock`
  - Params: `filepath`, `trigger`

### Texture Sources

- `tex.solid`
  - Inputs: none
  - Outputs: `out(Texture2D)`
  - `ExecutionKind`: `Render`
  - `ClockDomain`: `FrameClock`
- `tex.noise`
  - Inputs: none
  - Outputs: `out(Texture2D)`
  - `ExecutionKind`: `Render`
  - `ClockDomain`: `FrameClock`

### Texture Transforms

- `tex.transform_2d`
  - Inputs: `input0(Texture2D)`
  - Outputs: `out(Texture2D)`
  - `ExecutionKind`: `Render`
  - `ClockDomain`: `FrameClock`
- `tex.blur_gauss`
  - Inputs: `input0(Texture2D)`
  - Outputs: `out(Texture2D)`
  - `ExecutionKind`: `Render`
  - `ClockDomain`: `FrameClock`
  - Note: separable 2-pass render implementation.
- `tex.levels`
  - Inputs: `input0(Texture2D)`
  - Outputs: `out(Texture2D)`
  - `ExecutionKind`: `Render`
  - `ClockDomain`: `FrameClock`
- `tex.hsv`
  - Inputs: `input0(Texture2D)`
  - Outputs: `out(Texture2D)`
  - `ExecutionKind`: `Render`
  - `ClockDomain`: `FrameClock`

### Compositing

- `tex.mix`
  - Inputs: `a(Texture2D)`, `b(Texture2D)`
  - Outputs: `out(Texture2D)`
  - `ExecutionKind`: `Render`
  - `ClockDomain`: `FrameClock`
- `tex.mask`
  - Inputs: `base(Texture2D)`, `layer(Texture2D)`, `mask(Texture2D)`
  - Outputs: `out(Texture2D)`
  - `ExecutionKind`: `Render`
  - `ClockDomain`: `FrameClock`
- `tex.compose_over`
  - Inputs: `under(Texture2D)`, `over(Texture2D)`
  - Outputs: `out(Texture2D)`
  - `ExecutionKind`: `Render`
  - `ClockDomain`: `FrameClock`

### Shader Core

- `tex.shader`
  - Inputs: `input_textures[0..N](Texture2D)`, optional `params(ParamBlock)`
  - Outputs: `out(Texture2D)`
  - `ExecutionKind`: `Render`
  - `ClockDomain`: `FrameClock`
  - Failure behavior: retain last valid pipeline and mark node as error-state.

### Control/Data

- `ctl.time`
  - Inputs: none
  - Outputs: `out(Struct)`
  - `ExecutionKind`: `Control`
  - `ClockDomain`: `FrameClock`
- `data.params`
  - Inputs: optional `in(Struct)` for edits/overrides
  - Outputs: `out(ParamBlock)`
  - `ExecutionKind`: `Cpu`
  - `ClockDomain`: `EventClock`
- `ctl.switch`
  - Inputs: `a(Texture2D)`, `b(Texture2D)`, `select(Struct|bool field)`
  - Outputs: `out(Texture2D)`
  - `ExecutionKind`: `Render`
  - `ClockDomain`: `FrameClock`
- `ctl.latch`
  - Inputs: `value(Struct)`, `trigger(Struct|bool field)`
  - Outputs: `out(Struct)`
  - `ExecutionKind`: `Control`
  - `ClockDomain`: `FrameClock`

## 6. Validation Rules

`GraphBuilder::build()` (or equivalent validator) must enforce:

- non-zero graph dimensions for texture-domain graphs
- non-empty node set
- unique `NodeId`s
- existing source and destination nodes for each edge
- source output and destination input compatibility by exact `ResourceKind`
- one inbound edge max per destination input port
- required inputs connected for each node spec
- acyclic graph

## 7. Compilation Rules

Compilation must produce:

- deterministic topological node order
- stable, per-node input ordering
- executable plan entries carrying:
  - `NodeId`
  - `ExecutionKind`
  - `ClockDomain`
  - input/output resource handles
- explicit output binding map for viewer/export sinks

## 8. Caching and Re-execution

Per output resource:

- maintain version counter
- maintain parameter hash per node

Node re-executes when:

- any input version changes
- parameter hash changes
- pipeline invalidation occurs (for shader recompiles)

Otherwise reuse cached outputs.
