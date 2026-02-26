#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=lib/rust_tooling.sh
source "${repo_root}/scripts/lib/rust_tooling.sh"
ensure_rust_command cargo

usage() {
  cat <<'EOF'
Usage:
  scripts/ci_local.sh <validate|lock> <desktop_mid|laptop_integrated>
  scripts/ci_local.sh

Description:
  Runs the full local CI gate for one hardware tier host.
  - validate: checks against existing locked thresholds
  - lock: regenerates and locks thresholds from this host's measurements
  - validates deterministic GUI interaction trace thresholds
  - with no args: defaults to validate laptop_integrated

Environment overrides:
  COVERGEN_RUST_GPU_SPIRV_DIR  default: target/rust-gpu
EOF
}

allow_missing_gpu=0
if [[ $# -eq 0 ]]; then
  mode="validate"
  tier="laptop_integrated"
  allow_missing_gpu=1
  echo "[ci_local] no args provided; defaulting to mode=${mode} tier=${tier}"
elif [[ $# -eq 2 ]]; then
  mode="$1"
  tier="$2"
else
  usage
  exit 1
fi

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

shader_root="${COVERGEN_RUST_GPU_SPIRV_DIR:-target/rust-gpu}"
echo "[ci_local] ensuring rust-gpu artifacts in ${shader_root}"
if scripts/shaders/validate_rust_gpu_artifacts.sh "${shader_root}"; then
  echo "[ci_local] rust-gpu artifacts already valid"
else
  echo "[ci_local] rust-gpu artifacts missing/invalid, building"
  scripts/shaders/build_rust_gpu_artifacts.sh "${shader_root}"
fi

echo "[ci_local] rustfmt check"
cargo fmt --check

echo "[ci_local] clippy (warnings denied; private-doc lint reported separately)"
cargo clippy --all-targets --all-features -- -D warnings -A clippy::missing_docs_in_private_items

echo "[ci_local] full test suite"
cargo test -q

echo "[ci_local] still snapshot regression"
cargo test v2_still_fixed_seed_snapshots_match

echo "[ci_local] animation snapshot regression"
cargo test v2_animation_fixed_seed_sampled_frames_match

echo "[ci_local] animation movie-quality regression"
cargo test v2_animation_movie_quality_metrics_within_bounds

echo "[ci_local] gpu still confidence regression"
cargo test v2_gpu_still_fixed_seed_is_deterministic_when_hardware_available

echo "[ci_local] gpu animation confidence regression"
cargo test v2_gpu_animation_sampled_frames_change_when_hardware_available

echo "[ci_local] benchmark thresholds (${mode}) for tier=${tier}"
bench_log="$(mktemp)"
if scripts/bench/tier_gate.sh "${mode}" "${tier}" 2>&1 | tee "${bench_log}"; then
  rm -f "${bench_log}"
else
  if [[ "${allow_missing_gpu}" -eq 1 ]] && rg -qi "requires a hardware GPU|software adapter" "${bench_log}"; then
    echo "[ci_local] warning: skipping tier benchmark threshold enforcement in no-arg mode because no hardware GPU was detected"
    echo "[ci_local] warning: run 'scripts/ci_local.sh validate <tier>' on a hardware tier host for authoritative benchmark gating"
    rm -f "${bench_log}"
  else
    rm -f "${bench_log}"
    exit 1
  fi
fi

echo "[ci_local] gui interaction thresholds (${mode}) for tier=${tier}"
gui_log="$(mktemp)"
if scripts/gui/tier_gate.sh "${mode}" "${tier}" 2>&1 | tee "${gui_log}"; then
  rm -f "${gui_log}"
else
  if [[ "${allow_missing_gpu}" -eq 1 ]] && rg -qi "requires a hardware GPU|software adapter|WAYLAND_DISPLAY|nor DISPLAY is set" "${gui_log}"; then
    echo "[ci_local] warning: skipping gui interaction threshold enforcement in no-arg mode because no hardware GPU or display was detected"
    echo "[ci_local] warning: run 'scripts/ci_local.sh validate <tier>' on a hardware tier host with desktop display for authoritative gui threshold gating"
    rm -f "${gui_log}"
  else
    rm -f "${gui_log}"
    exit 1
  fi
fi

echo "[ci_local] completed mode=${mode} tier=${tier}"
