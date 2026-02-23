# V2 GPU Runtime

## Execution Model

The V2 runtime executes compiled layer steps using `GpuLayerRenderer` retained
APIs:

1. `begin_retained_image()`
2. `submit_retained_layer(...)` for each compiled step
3. `collect_retained_image(...)` once at image end

This avoids per-layer host readbacks.

## Adapter Policy

V2 targets hardware GPU execution. If a software adapter is selected
(`llvmpipe`, `swiftshader`, WARP, etc.), runtime fails fast with a clear error.

## Post Boundary

After one readback, host-side finishing is applied:

- contrast adjustment
- percentile stretch
- optional downsampling (AA > 1)
- PNG encoding under size cap

## Determinism

- Preset generation is deterministic from CLI seed.
- Per-image seed offsets are deterministic.
- Layer uniforms are deterministic per node and image index.
