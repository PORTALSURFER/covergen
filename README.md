# covergen

GPU-driven procedural cover generation.

## Modes

- `covergen` runs the legacy V1 pipeline (`src/engine.rs`).
- `covergen v2 ...` runs the node-graph V2 runtime (`src/v2/*`).

## V2 Quick Start

```bash
cargo run -- v2 --size 1024 --count 4 --seed 12345 --preset hybrid-stack --profile quality
```

Useful V2 flags:

- `--size <u32>` or `--width/--height`
- `--seed <u32>`
- `--count <u32>`
- `--layers <u32>`
- `--preset <hybrid-stack|field-weave>`
- `--profile <quality|performance>`
- `--antialias <1..=4>`
- `--output <path>`

## V2 Design Docs

- `docs/v2/architecture.md`
- `docs/v2/graph-spec.md`
- `docs/v2/gpu-runtime.md`
- `docs/v2/preset-authoring.md`
- `docs/v2/migration.md`
