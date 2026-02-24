# V2 rust-gpu Shader Backend

The runtime loads all shader programs from rust-gpu SPIR-V artifacts.
WGSL fallback paths have been removed.

## Programs

Expected SPIR-V file names:

- `fractal_main.spv`
- `graph_ops.spv`
- `graph_decode.spv`
- `retained_post.spv`

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
