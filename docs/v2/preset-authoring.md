# V2 Preset Authoring (No GUI)

## Approach

Presets are Rust functions that construct graphs with `GraphBuilder`.

Entry point:

- `src/v2/presets.rs::build_preset_graph`

Current presets:

- `hybrid-stack`
- `field-weave`

## Pattern

1. Create builder with target render size and seed.
2. Generate N `GenerateLayerNode` values from deterministic RNG.
3. Connect layers in sequence.
4. Add `Output` and connect final layer to it.
5. Return `builder.build()`.

## Adding a Preset

1. Add a new builder function in `src/v2/presets.rs`.
2. Register it in `build_preset_graph` match.
3. Keep generation deterministic for fixed seed.
4. Keep graph validation-compatible (acyclic and typed).

## Best Practices

- Keep node params clamped to shader-safe ranges.
- Vary blend mode/opacity gradually across layer depth.
- Use profile (`quality`/`performance`) to scale iterations and complexity.
- For animation-friendly presets, avoid abrupt discrete parameter jumps;
  prefer ranges that remain visually stable under slow modulation.
