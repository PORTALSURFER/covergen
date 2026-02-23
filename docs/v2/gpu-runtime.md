# V2 GPU Runtime

## Execution Model

The V2 runtime executes compiled layer steps using `GpuLayerRenderer` retained
APIs:

1. `begin_retained_image()`
2. `submit_retained_layer(...)` for each compiled step
3. `collect_retained_image(...)` once at image end

This avoids per-layer host readbacks.

## Animation Mode

V2 supports clip rendering for social-video output:

- `--animate --seconds <n> --fps <n>` enables frame sequence rendering.
- `--reels` sets `1080x1920` and enables animation automatically.

For each frame, layer parameters are gently modulated (center offsets, zoom,
mix, warp, contrast, opacity) using deterministic sinusoids. This produces slow
morphing over the full clip duration.

Frame flow:

1. Render all layers via retained GPU path.
2. Single readback for final luma.
3. Write PNG frame.
4. Assemble MP4 with `ffmpeg` (`libx264`, `yuv420p`, `+faststart`).

## Adapter Policy

V2 targets hardware GPU execution. If a software adapter is selected
(`llvmpipe`, `swiftshader`, WARP, etc.), runtime fails fast with a clear error.

Animation mode additionally requires `ffmpeg` in `PATH` for MP4 assembly.

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
