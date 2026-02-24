# V2 Graph Specification

## Core Types

- `NodeId`: stable node identifier.
- `PortType`:
  - `LumaTexture`
  - `MaskTexture`
  - `ChannelScalar`
  - `SopPrimitive`
- `NodeKind`:
  - `GenerateLayer(GenerateLayerNode)`
  - `SourceNoise(SourceNoiseNode)`
  - `Mask(MaskNode)`
  - `Blend(BlendNode)`
  - `ToneMap(ToneMapNode)`
  - `WarpTransform(WarpTransformNode)`
  - `ChopLfo(ChopLfoNode)`
  - `ChopMath(ChopMathNode)`
  - `ChopRemap(ChopRemapNode)`
  - `SopCircle(SopCircleNode)`
  - `SopSphere(SopSphereNode)`
  - `TopCameraRender(TopCameraRenderNode)`
  - `Output(OutputNode)`
- `OperatorFamily`:
  - `Top`: texture/image operators
  - `Chop`: channel/stream operators
  - `Sop`: geometry operators
  - `Output`: terminal output operators
- `EdgeSpec`: typed directed edge between node ports.
- `GpuGraph`: immutable validated graph payload.

## Output Contract

`OutputNode` defines output semantics:

- `role`:
  - `Primary`: default final image target used by runtime encode/finalize.
  - `Tap`: additional output product or module boundary output.
- `slot`: stable output slot index (`u8`) for addressing outputs.

Graph contract requires:

- At least one `Output` node is required.
- Exactly one `Primary` output is required.
- Output slots must be unique across all outputs.

## Node Port Contracts

Current built-in nodes span `Top`, `Chop`, `Sop`, and `Output` families.

- `GenerateLayer`:
  - inputs: `0..=1` (`slot 0: LumaTexture`)
  - output: `LumaTexture`
- `SourceNoise`:
  - inputs: `0`
  - output: `LumaTexture` or `MaskTexture` (`output_port`)
- `Mask`:
  - inputs: exactly `1` (`slot 0: LumaTexture`)
  - output: `MaskTexture`
- `Blend`:
  - inputs: `2..=3`
  - `slot 0: LumaTexture` (base)
  - `slot 1: LumaTexture` (top)
  - `slot 2: MaskTexture` (optional mask)
  - output: `LumaTexture`
- `ToneMap`:
  - inputs: `1..=2`
  - `slot 0: LumaTexture`
  - `slot 1: ChannelScalar` (optional contrast modulation)
  - output: `LumaTexture`
- `WarpTransform`:
  - inputs: `1..=2`
  - `slot 0: LumaTexture`
  - `slot 1: ChannelScalar` (optional strength modulation)
  - output: `LumaTexture`
- `ChopLfo`:
  - inputs: `0`
  - output: `ChannelScalar`
- `ChopMath`:
  - inputs: `1..=2`
  - `slot 0: ChannelScalar` (required)
  - `slot 1: ChannelScalar` (optional)
  - output: `ChannelScalar`
- `ChopRemap`:
  - inputs: exactly `1` (`slot 0: ChannelScalar`)
  - output: `ChannelScalar`
- `SopCircle` / `SopSphere`:
  - inputs: `0`
  - output: `SopPrimitive`
- `TopCameraRender`:
  - inputs: `1..=2`
  - `slot 0: SopPrimitive` (required)
  - `slot 1: ChannelScalar` (optional camera modulation)
  - output: `LumaTexture`
- `Output`:
  - inputs: exactly `1` (`slot 0: LumaTexture`)
  - output: none

## Temporal Modulation Contract

Node temporal channels are optional and evaluated once per frame through
`GraphTimeInput` (`t` normalized clip position and global intensity `i`).

Each temporal channel accepts either:

- `TemporalCurve` (legacy sine-curve modulation)
- `TemporalModulation::Expr` (expression DSL program)

Expression DSL:

- Variables: `t`, `i`
- Constants: `pi`, `tau`
- Operators: `+`, `-`, `*`, `/`
- Functions: `sin`, `cos`, `abs`, `fract`, `tri`, `saw`, `min`, `max`, `clamp`

Example expression:

`0.08 * sin((t * 0.9 + 0.2) * tau) * i`

## Validation Rules

`GraphBuilder::build()` validates:

- Graph dimensions must be non-zero.
- Graph must contain at least one node.
- Node IDs must be unique.
- At least one output node exists.
- Exactly one primary output exists.
- Output slots are unique.
- Each edge source/target node must exist.
- Source and target port types must match node contracts.
- A target input slot can have at most one incoming edge.
- Every node input count must satisfy its min/max input range.
- Required low-index input slots (`0..min_inputs-1`) must all be connected.
- Graph must be acyclic.

## Compilation Rules

`compile_graph()` performs:

- Topological ordering of validated DAG nodes.
- Input ordering by destination slot (`to_input`) per step.
- Output binding extraction for every output node.
- Primary output node detection from output bindings.
- GPU resource lifetime planning (alias slots + release schedule).

Runtime uses output bindings for metadata and stages only the primary output
for default image finalization.
