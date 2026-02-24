# V2 Architecture

## Goals

- Programmatic node-graph generation (no GUI).
- GPU-first execution with retained buffers.
- One readback per image at output boundary.

## Module Layout

- `src/graph.rs`: typed graph IR and validation.
- `src/compiler.rs`: topological lowering to runtime plan.
- `src/presets/mod.rs`: deterministic graph generators from seed.
- `src/runtime.rs`: GPU executor and output encoding.
- `src/runtime_config.rs`: runtime argument parsing.
- `src/main.rs`: orchestration entrypoint.
- `src/shaders.rs`: strict rust-gpu SPIR-V shader loader.

## Pipeline

1. Parse V2 CLI config.
2. Build graph from preset generator.
3. Validate and compile graph.
4. Execute compiled layer steps on GPU retained path.
5. Read back once to host memory.
6. Final normalization/downsample/PNG encode.

Animation path:

1. Compile once per clip.
2. For each frame, evaluate graph-time modulation on supported node params
   using deterministic low-frequency functions.
3. Execute retained GPU graph and read back once per frame.
4. Encode PNG sequence and assemble MP4 with ffmpeg.

## Current Runtime Limits

- Graphs must be acyclic and type-correct per node port contracts.
- Runtime supports DAG fan-out/fan-in across all current node kinds
  (`GenerateLayer`, `SourceNoise`, `Mask`, `Blend`, `ToneMap`, `WarpTransform`,
  `Output`).
- Graph must define exactly one primary output and may define additional tap
  outputs with unique slots.
- Default encode/finalization path reads back and encodes the primary output.
- V2 requires a hardware GPU adapter; software adapters are rejected.
