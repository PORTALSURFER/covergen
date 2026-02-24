# covergen

GPU-driven procedural cover generation.

## Hardware Requirement

`covergen` requires a hardware GPU (integrated or discrete) for all runtime and benchmark commands.
Software adapters (for example llvmpipe/swiftshader/WARP) and CPU fallback are disabled; if no
hardware GPU is available, the process exits with an error.

## Modes

- `covergen` runs the node-graph runtime (`src/*`).
- `covergen bench ...` runs benchmark + telemetry workflows.

V1 runtime support was removed on **February 24, 2026**. Migration and cutover status are documented in `docs/v2/migration.md`.

## V2 Quick Start

```bash
cargo run --bin covergen -- --size 1024 --count 4 --seed 12345 --preset hybrid-stack --profile quality
```

Instagram Reels animation (30 seconds, vertical 1080x1920, gentle modulation):

```bash
cargo run --bin covergen -- --reels --animate --seconds 30 --fps 30 --seed 12345 --output reel.mp4
```

Useful V2 flags:

- `--size <u32>` or `--width/--height`
- `--seed <u32>`
  - If omitted, V2 generates a fresh seed for each run.
- `--count <u32>`
- `--layers <u32>`
- `--preset <hybrid-stack|field-weave|node-weave|mask-atlas|warp-grid|random-grammar|td-primitive-stage|td-random-network|td-cascade-lab|td-feedback-atlas|td-hyperweave|td-patchwork|td-router|td-signal-lab|td-orbit-forge|td-modular-network|td-multi-stage>`
- `--profile <quality|performance>`
- `--antialias <1..=4>`
- `--output <path>`
- `--animate --seconds <u32> --fps <u32>`
- `--motion <gentle|normal|wild>` (animation intensity profile; default `normal`)
- `--reels` (forces 1080x1920 and enables animation)
- `--keep-frames` (preserve temporary PNG frames after MP4 encode)

Operator-family model (TouchDesigner-style):

- `TOP`: image/texture operators (includes camera render from SOP primitives)
- `CHOP`: channel operators (`chop-lfo`, `chop-math`, `chop-remap`)
- `SOP`: geometry operators (`sop-circle`, `sop-sphere`)
- `Output`: terminal output contracts

## Benchmark Suite

Run V2 benchmark + telemetry report:

```bash
cargo run --bin covergen -- bench
```

Report output:

- `target/bench/benchmark_report.md`
- `target/bench/benchmark_metrics.ini`

Tiered baseline + threshold lock workflow:

```bash
# 1) On each target hardware tier, capture baseline metrics and lock thresholds
scripts/ci_local.sh lock desktop_mid

# 2) Validate future runs against the locked thresholds
scripts/ci_local.sh validate desktop_mid
```

PowerShell equivalents:

```powershell
pwsh -File scripts/bench/tier_gate.ps1 lock desktop_mid
pwsh -File scripts/bench/tier_gate.ps1 lock laptop_integrated
```

## Local CI (Authoritative)

Project gating is local-first. GitHub Actions are not the source of truth for
cutover decisions.

Run full local CI on each hardware tier host:

```bash
scripts/ci_local.sh validate desktop_mid
scripts/ci_local.sh validate laptop_integrated
```

PowerShell equivalents:

```powershell
pwsh -File scripts/ci_local.ps1 validate desktop_mid
pwsh -File scripts/ci_local.ps1 validate laptop_integrated
```

When refreshing thresholds from measured baselines:

```bash
scripts/ci_local.sh lock desktop_mid
scripts/ci_local.sh lock laptop_integrated
```

PowerShell lock equivalents:

```powershell
pwsh -File scripts/ci_local.ps1 lock desktop_mid
pwsh -File scripts/ci_local.ps1 lock laptop_integrated
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

V2 shader loading is strict rust-gpu SPIR-V only.
If artifacts are missing, runtime fails fast.

To point at rust-gpu artifacts explicitly:

```bash
export COVERGEN_RUST_GPU_SPIRV_DIR=target/rust-gpu
```

Windows/PowerShell instrumentation for build + validation:

```powershell
pwsh -File scripts/shaders/build_rust_gpu_artifacts.ps1 `
  -ArtifactsDir target/rust-gpu `
```

Default shader build uses:
`cargo +nightly-2023-05-27 run --quiet --manifest-path shaders/build_spirv/Cargo.toml`.
Install rust-gpu toolchain prerequisites first:

```bash
rustup toolchain install nightly-2023-05-27 -c rust-src -c rustc-dev -c llvm-tools-preview
```

Bash/Linux build + validation:

```bash
scripts/shaders/build_rust_gpu_artifacts.sh target/rust-gpu
```
