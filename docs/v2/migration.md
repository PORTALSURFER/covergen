# V1 to V2 Migration Notes

## Status

V2 is implemented as a clean-break path invoked via `covergen v2 ...`.
Legacy V1 remains available as default `covergen`.

## What Changed

- New graph IR and compiler (`src/v2/graph.rs`, `src/v2/compiler.rs`).
- Programmatic preset generation (`src/v2/presets.rs`).
- GPU retained execution with single readback (`src/v2/runtime.rs`).
- V2-specific CLI parsing (`src/v2/cli.rs`).

## What Did Not Change

- Legacy V1 pipeline code remains in place (`src/engine.rs`).
- Existing V1 CLI behavior remains unchanged.

## Next Migration Steps

1. Expand node kinds beyond `GenerateLayer`/`Output`.
2. Add resource lifetime/alias optimization in compiler.
3. Move more postprocess stages fully onto GPU.
4. Add richer graph topology support (branching/merging).
5. Add benchmark suite comparing V1 and V2 latency/throughput.
