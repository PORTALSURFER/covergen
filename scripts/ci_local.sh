#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  scripts/ci_local.sh <validate|lock> <desktop_mid|laptop_integrated>

Description:
  Runs the full local CI gate for one hardware tier host.
  - validate: checks against existing locked thresholds
  - lock: regenerates and locks thresholds from this host's measurements

Environment overrides:
  COVERGEN_RUST_GPU_SPIRV_DIR  default: target/rust-gpu
EOF
}

if [[ $# -ne 2 ]]; then
  usage
  exit 1
fi

mode="$1"
tier="$2"

case "${mode}" in
  validate|lock) ;;
  *)
    echo "unknown mode: ${mode}" >&2
    usage
    exit 1
    ;;
esac

case "${tier}" in
  desktop_mid|laptop_integrated) ;;
  *)
    echo "unknown tier: ${tier}" >&2
    usage
    exit 1
    ;;
esac

echo "[ci_local] validating rust-gpu artifacts"
scripts/shaders/validate_rust_gpu_artifacts.sh "${COVERGEN_RUST_GPU_SPIRV_DIR:-target/rust-gpu}"

echo "[ci_local] rustfmt check"
cargo fmt --check

echo "[ci_local] full test suite"
cargo test -q

echo "[ci_local] still snapshot regression"
cargo test v2_still_fixed_seed_snapshots_match

echo "[ci_local] animation snapshot regression"
cargo test v2_animation_fixed_seed_sampled_frames_match

echo "[ci_local] gpu still confidence regression"
cargo test v2_gpu_still_fixed_seed_is_deterministic_when_hardware_available

echo "[ci_local] gpu animation confidence regression"
cargo test v2_gpu_animation_sampled_frames_change_when_hardware_available

echo "[ci_local] benchmark thresholds (${mode}) for tier=${tier}"
scripts/bench/tier_gate.sh "${mode}" "${tier}"

echo "[ci_local] completed mode=${mode} tier=${tier}"
