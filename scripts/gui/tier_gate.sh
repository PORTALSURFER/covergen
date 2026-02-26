#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
# shellcheck source=../lib/rust_tooling.sh
source "${repo_root}/scripts/lib/rust_tooling.sh"
ensure_rust_command cargo

usage() {
  cat <<'EOF'
Usage:
  scripts/gui/tier_gate.sh lock <desktop_mid|laptop_integrated>
  scripts/gui/tier_gate.sh validate <desktop_mid|laptop_integrated>

Environment overrides:
  GUI_TRACE_FRAMES                default: 420
  GUI_WARMUP_FRAMES               default: 60
  GUI_TARGET_FPS                  default: 60
  GUI_SIZE                        default: 1024
  GUI_SEED                        default: 1337
  GUI_MS_THRESHOLD_MARGIN         default: 1.20
  GUI_HIT_THRESHOLD_MARGIN        default: 1.20
  OUTPUT_ROOT                     default: target/bench
  COVERGEN_RUST_GPU_SPIRV_DIR     default: target/rust-gpu
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

trace_frames="${GUI_TRACE_FRAMES:-420}"
warmup_frames="${GUI_WARMUP_FRAMES:-60}"
target_fps="${GUI_TARGET_FPS:-60}"
size="${GUI_SIZE:-1024}"
seed="${GUI_SEED:-1337}"
ms_margin="${GUI_MS_THRESHOLD_MARGIN:-1.20}"
hit_margin="${GUI_HIT_THRESHOLD_MARGIN:-1.20}"
output_root="${OUTPUT_ROOT:-target/bench}"
shader_root="${COVERGEN_RUST_GPU_SPIRV_DIR:-target/rust-gpu}"

output_dir="${output_root}/${tier}"
trace_file="${output_dir}/gui_interaction_trace.csv"
metrics_file="${output_dir}/gui_interaction_metrics.ini"
threshold_file="docs/v2/benchmarks/${tier}.gui_interaction.thresholds.ini"

mkdir -p "${output_dir}"

echo "[gui-bench] ${mode} tier=${tier} trace=${trace_file} thresholds=${threshold_file}"
scripts/shaders/validate_rust_gpu_artifacts.sh "${shader_root}"
cargo run --quiet --bin covergen -- gui \
  --width "${size}" \
  --height "${size}" \
  --seed "${seed}" \
  --gui-target-fps "${target_fps}" \
  --gui-vsync off \
  --gui-benchmark-drag \
  --gui-benchmark-frames "${trace_frames}" \
  --gui-perf-trace "${trace_file}"

if [[ ! -s "${trace_file}" ]]; then
  echo "[gui-bench] missing trace file: ${trace_file}" >&2
  exit 1
fi

tmp_dir="$(mktemp -d)"
trap 'rm -rf "${tmp_dir}"' EXIT

update_file="${tmp_dir}/update_ms.txt"
scene_file="${tmp_dir}/scene_ms.txt"
render_file="${tmp_dir}/render_ms.txt"
hit_file="${tmp_dir}/hit_test_scans.txt"

awk -F, -v warmup="${warmup_frames}" 'NR > 1 && $1+0 >= warmup { print $3 }' "${trace_file}" > "${update_file}"
awk -F, -v warmup="${warmup_frames}" 'NR > 1 && $1+0 >= warmup { print $4 }' "${trace_file}" > "${scene_file}"
awk -F, -v warmup="${warmup_frames}" 'NR > 1 && $1+0 >= warmup { print $5 }' "${trace_file}" > "${render_file}"
awk -F, -v warmup="${warmup_frames}" 'NR > 1 && $1+0 >= warmup { print $9 }' "${trace_file}" > "${hit_file}"

sample_count="$(wc -l < "${update_file}")"
if [[ "${sample_count}" -eq 0 ]]; then
  echo "[gui-bench] no samples remained after warmup=${warmup_frames}" >&2
  exit 1
fi

percentile95() {
  local values_file="$1"
  local count index
  count="$(wc -l < "${values_file}")"
  index=$(( (95 * count + 99) / 100 ))
  sort -g "${values_file}" | sed -n "${index}p"
}

format_float() {
  local value="$1"
  awk -v v="${value}" 'BEGIN { printf "%.4f", v }'
}

update_p95="$(percentile95 "${update_file}")"
scene_p95="$(percentile95 "${scene_file}")"
render_p95="$(percentile95 "${render_file}")"
hit_p95="$(percentile95 "${hit_file}")"

cat > "${metrics_file}" <<EOF
# covergen gui interaction metrics
version=1
tier=${tier}
trace_file=${trace_file}
trace_frames=${trace_frames}
warmup_frames=${warmup_frames}
sample_count=${sample_count}
update_ms_p95=$(format_float "${update_p95}")
scene_ms_p95=$(format_float "${scene_p95}")
render_ms_p95=$(format_float "${render_p95}")
hit_test_scans_p95=${hit_p95}
EOF

echo "[gui-bench] wrote metrics: ${metrics_file}"

if [[ "${mode}" == "lock" ]]; then
  update_limit="$(awk -v value="${update_p95}" -v margin="${ms_margin}" 'BEGIN { printf "%.4f", value * margin }')"
  scene_limit="$(awk -v value="${scene_p95}" -v margin="${ms_margin}" 'BEGIN { printf "%.4f", value * margin }')"
  render_limit="$(awk -v value="${render_p95}" -v margin="${ms_margin}" 'BEGIN { printf "%.4f", value * margin }')"
  hit_limit="$(awk -v value="${hit_p95}" -v margin="${hit_margin}" 'BEGIN { printf "%.0f", value * margin }')"
  cat > "${threshold_file}" <<EOF
# covergen gui interaction thresholds
version=1
tier=${tier}
trace_frames=${trace_frames}
warmup_frames=${warmup_frames}
update_ms_p95_max=${update_limit}
scene_ms_p95_max=${scene_limit}
render_ms_p95_max=${render_limit}
hit_test_scans_p95_max=${hit_limit}
EOF
  echo "[gui-bench] locked thresholds: ${threshold_file}"
  exit 0
fi

if [[ ! -f "${threshold_file}" ]]; then
  echo "[gui-bench] missing threshold file: ${threshold_file}" >&2
  echo "[gui-bench] run scripts/gui/tier_gate.sh lock ${tier} on tier hardware first" >&2
  exit 1
fi

read_threshold() {
  local key="$1"
  awk -F= -v key="${key}" '$1 == key { print $2 }' "${threshold_file}" | tail -n 1
}

threshold_tier="$(read_threshold "tier")"
if [[ -z "${threshold_tier}" ]]; then
  echo "[gui-bench] threshold file missing tier key: ${threshold_file}" >&2
  exit 1
fi
if [[ "${threshold_tier}" != "${tier}" ]]; then
  echo "[gui-bench] threshold tier mismatch: expected=${tier} found=${threshold_tier}" >&2
  exit 1
fi

update_limit="$(read_threshold "update_ms_p95_max")"
scene_limit="$(read_threshold "scene_ms_p95_max")"
render_limit="$(read_threshold "render_ms_p95_max")"
hit_limit="$(read_threshold "hit_test_scans_p95_max")"

missing_key=0
for key in update_limit scene_limit render_limit hit_limit; do
  if [[ -z "${!key}" ]]; then
    echo "[gui-bench] threshold file missing required key: ${key}" >&2
    missing_key=1
  fi
done
if [[ "${missing_key}" -ne 0 ]]; then
  exit 1
fi

check_le() {
  local label="$1"
  local actual="$2"
  local limit="$3"
  awk -v actual="${actual}" -v limit="${limit}" 'BEGIN { exit !(actual <= limit) }'
}

violations=()
if ! check_le "update_ms_p95" "${update_p95}" "${update_limit}"; then
  violations+=("update_ms_p95=${update_p95} exceeds ${update_limit}")
fi
if ! check_le "scene_ms_p95" "${scene_p95}" "${scene_limit}"; then
  violations+=("scene_ms_p95=${scene_p95} exceeds ${scene_limit}")
fi
if ! check_le "render_ms_p95" "${render_p95}" "${render_limit}"; then
  violations+=("render_ms_p95=${render_p95} exceeds ${render_limit}")
fi
if ! check_le "hit_test_scans_p95" "${hit_p95}" "${hit_limit}"; then
  violations+=("hit_test_scans_p95=${hit_p95} exceeds ${hit_limit}")
fi

if [[ "${#violations[@]}" -ne 0 ]]; then
  echo "[gui-bench] threshold validation failed (${#violations[@]} violations):" >&2
  for violation in "${violations[@]}"; do
    echo "  - ${violation}" >&2
  done
  exit 1
fi

echo "[gui-bench] threshold validation passed: ${threshold_file}"
