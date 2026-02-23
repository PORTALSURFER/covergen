# V2 Architecture

## Goals

- Programmatic node-graph generation (no GUI).
- GPU-first execution with retained buffers.
- One readback per image at output boundary.

## Module Layout

- `src/v2/graph.rs`: typed graph IR and validation.
- `src/v2/compiler.rs`: topological lowering to runtime plan.
- `src/v2/presets.rs`: deterministic graph generators from seed.
- `src/v2/runtime.rs`: GPU executor and output encoding.
- `src/v2/cli.rs`: V2-specific argument parsing.
- `src/v2/mod.rs`: orchestration entrypoint.

## Pipeline

1. Parse V2 CLI config.
2. Build graph from preset generator.
3. Validate and compile graph.
4. Execute compiled layer steps on GPU retained path.
5. Read back once to host memory.
6. Final normalization/downsample/PNG encode.

## Current Runtime Limits

- Current executor supports linear or near-linear graphs where each `GenerateLayer`
  has at most one downstream edge.
- One `Output` node is supported.
- Composition is encoded per layer using blend mode, opacity, and contrast.

These constraints are deliberate to keep execution deterministic while the node
library and scheduler evolve.
