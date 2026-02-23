# covergen

GPU-driven procedural cover generation.

## Modes

- `covergen` runs the legacy V1 pipeline (`src/engine.rs`).
- `covergen v2 ...` runs the node-graph V2 runtime (`src/v2/*`).

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
- `--reels` (forces 1080x1920 and enables animation)
- `--keep-frames` (preserve temporary PNG frames after MP4 encode)

## Benchmark Suite

Run V1 vs V2 benchmark + telemetry report:

```bash
cargo run -- bench
```

Report output:

- `target/bench/benchmark_report.md`

## V2 Design Docs

- `docs/v2/architecture.md`
- `docs/v2/graph-spec.md`
- `docs/v2/gpu-runtime.md`
- `docs/v2/preset-authoring.md`
- `docs/v2/migration.md`
