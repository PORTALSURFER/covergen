#!/usr/bin/env bash
set -euo pipefail

root="${1:-${COVERGEN_RUST_GPU_SPIRV_DIR:-target/rust-gpu}}"

echo "[shader] building SPIR-V artifacts into ${root}"
export COVERGEN_RUST_GPU_SPIRV_DIR="${root}"
cargo run --quiet --bin build_spirv
scripts/shaders/validate_rust_gpu_artifacts.sh "${root}"
