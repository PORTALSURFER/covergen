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
  COVERGEN_ALLOW_MISSING_GPU   default: 0
                               when set to 1, preserves no-arg skip behavior
                               for missing GPU/display even when CI is set
EOF
}

is_truthy() {
  case "${1,,}" in
    1|true|yes|on) return 0 ;;
    *) return 1 ;;
  esac
}

is_linux_host() {
  [[ "$(uname -s 2>/dev/null)" == "Linux" ]]
}

missing_linux_dri() {
  is_linux_host && [[ ! -d "/dev/dri" ]]
}

missing_linux_display() {
  is_linux_host && [[ -z "${DISPLAY:-}" && -z "${WAYLAND_DISPLAY:-}" ]]
}

run_common_ci_steps() {
  local steps_file="${repo_root}/scripts/lib/ci_local_steps.tsv"
  if [[ ! -f "${steps_file}" ]]; then
    echo "missing shared CI steps file: ${steps_file}" >&2
    exit 1
  fi
  while IFS='|' read -r label bash_command _pwsh_command; do
    [[ -z "${label}" || "${label}" =~ ^[[:space:]]*# ]] && continue
    echo "[ci_local] ${label}"
    eval "${bash_command}"
  done < "${steps_file}"
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

ci_mode=0
if is_truthy "${CI:-0}"; then
  ci_mode=1
fi
allow_missing_gpu_opt_in=0
if is_truthy "${COVERGEN_ALLOW_MISSING_GPU:-0}"; then
  allow_missing_gpu_opt_in=1
fi
if [[ "${ci_mode}" -eq 1 ]]; then
  if [[ "${allow_missing_gpu_opt_in}" -eq 1 ]]; then
    echo "[ci_local] warning: CI mode detected but COVERGEN_ALLOW_MISSING_GPU=1 is set; missing GPU/display skips remain enabled"
  else
    if [[ "${allow_missing_gpu}" -eq 1 ]]; then
      echo "[ci_local] CI mode detected; disabling no-arg missing GPU/display skip behavior"
    fi
    allow_missing_gpu=0
  fi
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

run_common_ci_steps

echo "[ci_local] benchmark thresholds (${mode}) for tier=${tier}"
if [[ "${allow_missing_gpu}" -eq 1 ]] && missing_linux_dri; then
  echo "[ci_local] warning: skipping tier benchmark threshold enforcement in no-arg mode because no /dev/dri hardware GPU device was detected"
  echo "[ci_local] warning: run 'scripts/ci_local.sh validate <tier>' on a hardware tier host for authoritative benchmark gating"
else
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
fi

echo "[ci_local] gui interaction thresholds (${mode}) for tier=${tier}"
if [[ "${allow_missing_gpu}" -eq 1 ]] && (missing_linux_dri || missing_linux_display); then
  if missing_linux_dri; then
    echo "[ci_local] warning: skipping gui interaction threshold enforcement in no-arg mode because no /dev/dri hardware GPU device was detected"
  fi
  if missing_linux_display; then
    echo "[ci_local] warning: skipping gui interaction threshold enforcement in no-arg mode because no desktop display was detected"
  fi
  echo "[ci_local] warning: run 'scripts/ci_local.sh validate <tier>' on a hardware tier host with desktop display for authoritative gui threshold gating"
else
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
fi

echo "[ci_local] completed mode=${mode} tier=${tier}"
