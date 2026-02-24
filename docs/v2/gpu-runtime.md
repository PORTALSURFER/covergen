# V2 GPU Runtime

## Execution Model

The V2 runtime executes graph-native presets through retained GPU buffers for
all compiled node kinds:

1. `begin_retained_image()` clears retained accumulation state.
2. Graph nodes (`GenerateLayer`, `SourceNoise`, `Mask`, `Blend`, `ToneMap`,
   `WarpTransform`) run on aliased GPU output slots.
3. Final compositor stage resolves `Output` bindings:
   - `Primary` selects the base luma slot.
   - `Tap` outputs are sorted by slot and composited into the base with a
     deterministic GPU blend policy.
4. Composited result is staged into retained accumulation.
5. `collect_retained_output_gray(...)` runs GPU finalize passes and performs
   one image-end readback.

This avoids per-node host readbacks in graph-native execution.

Shader modules are loaded through `src/shaders.rs` and run from:

- rust-gpu SPIR-V artifacts only (strict mode)

## Animation Mode

V2 supports clip rendering for social-video output:

- `--animate --seconds <n> --fps <n>` enables frame sequence rendering.
- `--reels` sets `1080x1920` and enables animation automatically.
- `--motion <gentle|normal|wild>` controls temporal modulation intensity.

For each frame, layer parameters are gently modulated (center offsets, zoom,
mix, warp, contrast, opacity) using deterministic sinusoids. This produces slow
morphing over the full clip duration.

Motion profile behavior:

- `gentle`: low modulation amplitude, stable per-clip seed (minimum flicker)
- `normal`: moderate modulation amplitude, stable per-clip seed
- `wild`: full modulation amplitude with per-frame seed jitter

On top of DSL/curve temporal expressions, runtime applies profile constraints:

- modulation envelope clamp
- per-frame slew-rate cap

This reduces abrupt frame-to-frame parameter jumps without removing the
underlying modulation signal.

Frame flow:

1. Render all layers via retained GPU path.
2. Single readback for final luma.
3. Write PNG frame.
4. Assemble MP4 with `ffmpeg` (`libx264`, `yuv420p`, `+faststart`).

## Still Candidate Selection Mode

For still-image runs, V2 can explore low-resolution candidates and keep only
top-scoring seeds for final full-resolution rendering:

- `--explore-candidates <n>` enables generate-score-select mode.
- `--explore-size <n>` sets the max low-res exploration dimension.
- Final output count remains `--count`; runtime renders the top `count` seeds.

Score combines:

- composition quality (contrast/edge/exposure balance)
- novelty against previously explored candidates
- temporal stability under a small modulation probe

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

## Tap Output Artifact Strategy

- Runtime encodes one artifact per still/frame (the composited primary output).
- Tap outputs are still first-class graph surfaces and now directly feed the
  explicit GPU compositor stage before finalization.
- Bench and regression suites validate that benchmark/snapshot graphs compile
  with one primary output and at least one tap output.

## Determinism

- Preset generation is deterministic from CLI seed.
- Per-image seed offsets are deterministic.
- Layer uniforms are deterministic per node and image index.
