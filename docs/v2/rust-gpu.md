# V2 rust-gpu Shader Backend

The runtime now supports loading all shader programs from rust-gpu SPIR-V
artifacts instead of embedded WGSL text.

## Programs

Expected SPIR-V file names:

- `fractal_main.spv`
- `graph_ops.spv`
- `graph_decode.spv`
- `retained_post.spv`

## Runtime Switch

Default backend is rust-gpu SPIR-V in auto mode.
If SPIR-V files are missing, runtime falls back to WGSL with a warning.

Use custom artifact directory if needed:

```bash
export COVERGEN_RUST_GPU_SPIRV_DIR=target/rust-gpu
```

If `COVERGEN_RUST_GPU_SPIRV_DIR` is unset, runtime defaults to
`target/rust-gpu`.

Force legacy WGSL backend only when needed:

```bash
export COVERGEN_SHADER_BACKEND=wgsl
```

Require strict rust-gpu (disable fallback):

```bash
export COVERGEN_SHADER_BACKEND=rust-gpu
```

## Artifact Validation

Validate that all required SPIR-V files exist and have correct magic:

```bash
scripts/shaders/validate_rust_gpu_artifacts.sh target/rust-gpu
```

## Integration Notes

- `src/shaders.rs` is the single source of truth for shader program loading.
- All GPU pipelines now route through that module:
  - main fractal generation
  - graph ops/decode
  - retained post passes
