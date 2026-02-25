#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
# shellcheck source=../lib/rust_tooling.sh
source "${repo_root}/scripts/lib/rust_tooling.sh"
ensure_rust_command cargo

usage() {
  cat <<'EOF'
Usage:
  scripts/bench/tier_gate.sh lock <desktop_mid|laptop_integrated>
  scripts/bench/tier_gate.sh validate <desktop_mid|laptop_integrated>

Environment overrides:
  SAMPLES            default: 8
  ANIMATION_SAMPLES  default: 4
  SIZE               default: 1024
  BENCH_SECONDS      default: 6
  FPS                default: 24
  PRESET             default: mask-atlas
  PROFILE            default: performance
  OUTPUT_ROOT        default: target/bench
  COVERGEN_RUST_GPU_SPIRV_DIR
                     default: target/rust-gpu
EOF
}

if [[ $# -ne 2 ]]; then
  usage
  exit 1
fi

mode="$1"
tier="$2"

case "${mode}" in
  lock|validate) ;;
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

samples="${SAMPLES:-8}"
animation_samples="${ANIMATION_SAMPLES:-4}"
size="${SIZE:-1024}"
seconds="${BENCH_SECONDS:-${COVERGEN_SECONDS:-6}}"
fps="${FPS:-24}"
preset="${PRESET:-mask-atlas}"
profile="${PROFILE:-performance}"
output_root="${OUTPUT_ROOT:-target/bench}"
shader_root="${COVERGEN_RUST_GPU_SPIRV_DIR:-target/rust-gpu}"

threshold_file="docs/v2/benchmarks/${tier}.thresholds.ini"
output_dir="${output_root}/${tier}"

if [[ "${mode}" == "lock" ]]; then
  threshold_arg=(--lock-thresholds "${threshold_file}")
else
  threshold_arg=(--thresholds "${threshold_file}")
fi

echo "[bench] ${mode} tier=${tier} output=${output_dir} thresholds=${threshold_file}"
scripts/shaders/validate_rust_gpu_artifacts.sh "${shader_root}"
cargo run --quiet --bin covergen -- bench \
  --tier "${tier}" \
  --samples "${samples}" \
  --animation-samples "${animation_samples}" \
  --size "${size}" \
  --seconds "${seconds}" \
  --fps "${fps}" \
  --preset "${preset}" \
  --profile "${profile}" \
  --output-dir "${output_dir}" \
  --require-v2-scenarios \
  "${threshold_arg[@]}"
