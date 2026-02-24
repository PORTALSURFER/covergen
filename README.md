# covergen

GPU-driven procedural cover generation.

## Modes

- `covergen` runs the V2 node-graph runtime (`src/v2/*`) by default.
- `covergen v2 ...` runs V2 explicitly.
- `covergen bench ...` runs benchmark + telemetry workflows.

V1 CLI mode (`covergen v1`) was removed on **February 24, 2026**. Migration and cutover status are documented in `docs/v2/migration.md`.

## V2 Quick Start

```bash
cargo run -- v2 --size 1024 --count 4 --seed 12345 --preset hybrid-stack --profile quality
```

Instagram Reels animation (30 seconds, vertical 1080x1920, gentle modulation):

```bash
cargo run -- v2 --reels --animate --seconds 30 --fps 30 --seed 12345 --output reel.mp4
```

Useful V2 flags:

- `--size <u32>` or `--width/--height`
- `--seed <u32>`
- `--count <u32>`
- `--layers <u32>`
- `--preset <hybrid-stack|field-weave|node-weave|mask-atlas|warp-grid>`
- `--profile <quality|performance>`
- `--antialias <1..=4>`
- `--output <path>`
- `--animate --seconds <u32> --fps <u32>`
- `--motion <gentle|normal|wild>` (animation intensity profile; default `normal`)
- `--reels` (forces 1080x1920 and enables animation)
- `--keep-frames` (preserve temporary PNG frames after MP4 encode)

## Benchmark Suite

Run V1 vs V2 benchmark + telemetry report:

```bash
cargo run -- bench
```

Report output:

- `target/bench/benchmark_report.md`
- `target/bench/benchmark_metrics.ini`

Tiered baseline + threshold lock workflow:

```bash
# 1) On each target hardware tier, capture baseline metrics and lock thresholds
scripts/bench/tier_gate.sh lock desktop_mid

# 2) Validate future runs against the locked thresholds
scripts/bench/tier_gate.sh validate desktop_mid
```

## V2 Design Docs

- `docs/v2/architecture.md`
- `docs/v2/graph-spec.md`
- `docs/v2/gpu-runtime.md`
- `docs/v2/preset-authoring.md`
- `docs/v2/migration.md`
- `docs/v2/benchmarks/README.md`
- `docs/v2/rust-gpu.md`

## Shader Backend

The default shader backend is rust-gpu SPIR-V in auto mode: it will fall back
to WGSL if SPIR-V artifacts are missing.

To point at rust-gpu artifacts explicitly:

```bash
export COVERGEN_RUST_GPU_SPIRV_DIR=target/rust-gpu
```

To require strict rust-gpu (no fallback):

```bash
export COVERGEN_SHADER_BACKEND=rust-gpu
```

To force legacy WGSL shaders:

```bash
export COVERGEN_SHADER_BACKEND=wgsl
```
