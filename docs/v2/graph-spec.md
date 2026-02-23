# V2 Graph Specification

## Core Types

- `NodeId`: stable node identifier.
- `PortType`: currently `LumaTexture`.
- `NodeKind`:
  - `GenerateLayer(GenerateLayerNode)`
  - `Output`
- `EdgeSpec`: typed directed edge between node ports.
- `GpuGraph`: immutable validated graph payload.

## GenerateLayerNode Fields

`GenerateLayerNode` captures shader and blend parameters:

- Fractal controls (`symmetry`, `iterations`, `fill_scale`, `fractal_zoom`, etc.)
- Style controls (`art_style`, `art_style_secondary`, `art_style_mix`)
- Warp controls (`bend_strength`, `warp_strength`, `warp_frequency`)
- Layering controls (`blend_mode`, `opacity`, `contrast`)

## Validation Rules

- Graph must have non-zero dimensions.
- Graph must contain nodes.
- At least one `Output` node is required.
- `Output` nodes must have exactly one incoming luma edge.
- `GenerateLayer` nodes may have at most one incoming edge.
- Edge source/target node IDs must exist.
- Edge port types must match node port contracts.
- Graph must be acyclic.

## Compilation Constraints

The current runtime compiler additionally enforces:

- Exactly one `Output` node at execution time.
- `GenerateLayer` nodes with at most one outgoing edge.
