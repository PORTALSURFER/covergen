#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${repo_root}"
# shellcheck source=../lib/rust_tooling.sh
source "${repo_root}/scripts/lib/rust_tooling.sh"
ensure_rust_command cargo
ensure_rust_command rustup

root="${1:-${COVERGEN_RUST_GPU_SPIRV_DIR:-target/rust-gpu}}"
toolchain="${COVERGEN_RUST_GPU_TOOLCHAIN:-nightly-2023-05-27}"

if ! rustup toolchain list | awk '{print $1}' | grep -Eq "^${toolchain}(-|$)"; then
  echo "[shader] missing rustup toolchain '${toolchain}'" >&2
  echo "[shader] install it first, e.g.: rustup toolchain install ${toolchain} -c rust-src -c rustc-dev -c llvm-tools-preview" >&2
  exit 1
fi

echo "[shader] building SPIR-V artifacts into ${root} using toolchain ${toolchain}"
export COVERGEN_RUST_GPU_SPIRV_DIR="${root}"
export RUSTGPU_SKIP_TOOLCHAIN_CHECK="${RUSTGPU_SKIP_TOOLCHAIN_CHECK:-1}"

if [[ -n "${COVERGEN_RUST_GPU_BUILD_COMMAND:-}" ]]; then
  echo "[shader] running custom build command"
  eval "${COVERGEN_RUST_GPU_BUILD_COMMAND}"
else
  cargo "+${toolchain}" run --quiet --manifest-path shaders/build_spirv/Cargo.toml
fi

scripts/shaders/validate_rust_gpu_artifacts.sh "${root}"
