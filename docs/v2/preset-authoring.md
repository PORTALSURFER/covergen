# V2 Preset Authoring (No GUI)

## Approach

Presets are Rust functions that construct graphs with `GraphBuilder`.

Entry point:

- `src/presets/mod.rs::build_preset_graph`

Current presets:

- `hybrid-stack`
- `field-weave`
- `node-weave`
- `mask-atlas`
- `warp-grid`
- `random-grammar`
- `td-primitive-stage`
- `td-random-network`
- `td-cascade-lab`
- `td-feedback-atlas`
- `td-patchwork`
- `td-modular-network`
- `td-multi-stage`

TouchDesigner-focused presets use constrained CHOP/SOP/TOP wiring plus
`SourceNoise`/`Mask` sub-branches so random graphs stay visually varied without
devolving into unstructured flicker.

## Pattern

1. Create builder with target render size and seed.
2. Select node templates by `OperatorFamily` (`Top`/`Chop`/`Sop`/`Output`) as needed.
3. Generate base `GenerateLayerNode` sources from deterministic RNG.
4. Add graph-native operator nodes (`SourceNoise`, `Mask`, `Blend`, `ToneMap`, `WarpTransform`).
5. Build a DAG with fan-in/fan-out branches (avoid pure linear stacks).
6. Add output contract nodes:
   - one `OutputNode::primary()` for default final image encode
   - optional `OutputNode::tap(slot)` for extra products/module boundaries
7. Connect final luma streams to outputs and return `builder.build()`.

## Adding a Preset

1. Add a new builder function in `src/presets/families.rs` or a dedicated module under `src/presets/`.
2. Register it in `src/presets/preset_catalog.rs::register_builtin_presets`.
3. Keep generation deterministic for fixed seed.
4. Keep graph validation-compatible (acyclic and typed).

## Best Practices

- Keep node params clamped to shader-safe ranges.
- Vary blend mode/opacity gradually across layer depth.
- Use profile (`quality`/`performance`) to scale iterations and complexity.
- For animation-friendly presets, avoid abrupt discrete parameter jumps;
  prefer ranges that remain visually stable under slow modulation.
- Prefer temporal expressions for reusable modulation shapes when curve
  parameters become hard to tune. Example:
  - `TemporalModulation::parse("0.08 * sin((t * 0.9 + 0.2) * tau) * i")`
