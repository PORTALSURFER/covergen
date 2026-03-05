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
  SAMPLES            default: lock=8, validate=3
  ANIMATION_SAMPLES  default: lock=4, validate=1
  SIZE               default: lock=1024, validate=512
  BENCH_SECONDS      default: lock=6, validate=1
  FPS                default: 24
  PRESET             default: mask-atlas
  PROFILE            default: performance
  OUTPUT_ROOT        default: target/bench
  REQUIRE_LOCKED_THRESHOLDS
                    default: 0 (set to 1 to fail validate mode on placeholder thresholds)
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

if [[ "${mode}" == "lock" ]]; then
  default_samples=8
  default_animation_samples=4
  default_size=1024
  default_seconds=6
else
  default_samples=3
  default_animation_samples=1
  default_size=512
  default_seconds=1
fi

samples="${SAMPLES:-${default_samples}}"
animation_samples="${ANIMATION_SAMPLES:-${default_animation_samples}}"
size="${SIZE:-${default_size}}"
seconds="${BENCH_SECONDS:-${COVERGEN_SECONDS:-${default_seconds}}}"
fps="${FPS:-24}"
preset="${PRESET:-mask-atlas}"
profile="${PROFILE:-performance}"
output_root="${OUTPUT_ROOT:-target/bench}"
shader_root="${COVERGEN_RUST_GPU_SPIRV_DIR:-target/rust-gpu}"

threshold_file="docs/v2/benchmarks/${tier}.thresholds.ini"
output_dir="${output_root}/${tier}"
require_locked_thresholds="${REQUIRE_LOCKED_THRESHOLDS:-0}"

check_placeholder_thresholds() {
  local file="$1"
  if grep -Eq "0\\.001000|1000000\\.000000" "${file}"; then
    cat >&2 <<EOF
[bench] locked-threshold check failed for ${file}
Detected placeholder threshold values.
Run: scripts/ci_local.sh lock ${tier}
EOF
    return 1
  fi
}

if [[ "${mode}" == "lock" ]]; then
  threshold_arg=(--lock-thresholds "${threshold_file}")
else
  threshold_arg=(--thresholds "${threshold_file}")
  if [[ "${require_locked_thresholds}" == "1" ]]; then
    check_placeholder_thresholds "${threshold_file}"
  fi
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
