# V2 rust-gpu Shader Backend

The runtime loads all shader programs from rust-gpu SPIR-V artifacts.
WGSL fallback paths are removed.

## Programs

Expected SPIR-V file names:

- `fractal_main.spv`
- `graph_ops.spv`
- `graph_decode.spv`
- `retained_post.spv`

Rust shader source crates:

- `shaders/fractal_main`
- `shaders/graph_ops`
- `shaders/graph_decode`
- `shaders/retained_post`

## Runtime Behavior

Shader loading is strict:

- if artifacts are present and valid, runtime proceeds
- if artifacts are missing/invalid, runtime fails fast with an actionable error

Use custom artifact directory if needed:

```bash
export COVERGEN_RUST_GPU_SPIRV_DIR=target/rust-gpu
```

If `COVERGEN_RUST_GPU_SPIRV_DIR` is unset, runtime defaults to
`target/rust-gpu`.

## Artifact Validation

Validate required SPIR-V files and magic bytes:

```bash
scripts/shaders/validate_rust_gpu_artifacts.sh target/rust-gpu
```

Build rust-gpu artifacts and validate:

```bash
scripts/shaders/build_rust_gpu_artifacts.sh target/rust-gpu
```

PowerShell equivalents:

```powershell
pwsh -File scripts/shaders/validate_rust_gpu_artifacts.ps1 -Root target/rust-gpu
pwsh -File scripts/shaders/build_rust_gpu_artifacts.ps1 -ArtifactsDir target/rust-gpu
```

## Toolchain Requirements

By default, build scripts run:

`cargo +nightly-2023-05-27 run --quiet --manifest-path shaders/build_spirv/Cargo.toml`.

Required components for the selected toolchain:

- `rust-src`
- `rustc-dev`
- `llvm-tools-preview`

Install example:

```bash
rustup toolchain install nightly-2023-05-27 -c rust-src -c rustc-dev -c llvm-tools-preview
```

Override toolchain when needed:

```bash
export COVERGEN_RUST_GPU_TOOLCHAIN=nightly-2023-05-27
```

Windows:

```powershell
$env:COVERGEN_RUST_GPU_TOOLCHAIN = "nightly-2023-05-27"
```

## Integration Notes

- `src/shaders.rs` is the single source of truth for shader program loading.
- `shaders/build_spirv` compiles all runtime shader artifacts from rust-gpu crates.
- All GPU pipelines route through `src/shaders.rs`:
  - main fractal generation
  - graph ops/decode
  - retained post passes
