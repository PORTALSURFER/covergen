#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
report_dir="${repo_root}/target/ci"
report_file="${report_dir}/structure_drift_report.txt"
thresholds_file_default="${repo_root}/docs/v2/benchmarks/structure_drift.thresholds.ini"
thresholds_file="${COVERGEN_STRUCTURE_DRIFT_THRESHOLDS_FILE:-${thresholds_file_default}}"
gate_mode="${COVERGEN_STRUCTURE_DRIFT_GATE:-off}"

max_file_lines=400
max_oversized_files=40
max_dead_code_suppressions=40
max_too_many_arguments_suppressions=10

mkdir -p "${report_dir}"

if [[ -f "${thresholds_file}" ]]; then
  while IFS='=' read -r raw_key raw_value; do
    key="$(echo "${raw_key}" | tr -d '[:space:]')"
    value="$(echo "${raw_value:-}" | sed -E 's/[;#].*$//' | tr -d '[:space:]')"
    [[ -z "${key}" ]] && continue
    [[ "${key}" == \#* ]] && continue
    [[ -z "${value}" ]] && continue
    case "${key}" in
      max_file_lines) max_file_lines="${value}" ;;
      max_oversized_files) max_oversized_files="${value}" ;;
      max_dead_code_suppressions) max_dead_code_suppressions="${value}" ;;
      max_too_many_arguments_suppressions) max_too_many_arguments_suppressions="${value}" ;;
    esac
  done < "${thresholds_file}"
fi

case "${gate_mode}" in
  off|warn|fail) ;;
  *)
    echo "[structure-drift] invalid COVERGEN_STRUCTURE_DRIFT_GATE='${gate_mode}' (expected off|warn|fail)" >&2
    exit 2
    ;;
esac

oversized_lines=()
while IFS= read -r file; do
  lines="$(wc -l < "${file}")"
  if (( lines > max_file_lines )); then
    oversized_lines+=("$(printf '%6d %s' "${lines}" "${file#${repo_root}/}")")
  fi
done < <(cd "${repo_root}" && rg --files src -g '*.rs' | sort)

mapfile -t dead_code_lines < <(cd "${repo_root}" && rg -n "allow\(dead_code\)" src scripts || true)
mapfile -t too_many_args_lines < <(cd "${repo_root}" && rg -n "allow\(clippy::too_many_arguments\)" src scripts || true)

{
  echo "[structure-drift] generated: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "[structure-drift] gate mode: ${gate_mode}"
  echo "[structure-drift] thresholds file: ${thresholds_file#${repo_root}/}"
  echo "[structure-drift] thresholds: max_file_lines=${max_file_lines}, max_oversized_files=${max_oversized_files}, max_dead_code_suppressions=${max_dead_code_suppressions}, max_too_many_arguments_suppressions=${max_too_many_arguments_suppressions}"
  echo
  echo "[structure-drift] oversized Rust files (>${max_file_lines} LOC): ${#oversized_lines[@]}"
  for line in "${oversized_lines[@]}"; do
    echo "${line}"
  done
  echo
  echo "[structure-drift] dead-code lint suppressions: ${#dead_code_lines[@]}"
  for line in "${dead_code_lines[@]}"; do
    echo "${line}"
  done
  echo
  echo "[structure-drift] too-many-arguments lint suppressions: ${#too_many_args_lines[@]}"
  for line in "${too_many_args_lines[@]}"; do
    echo "${line}"
  done
} > "${report_file}"

cat "${report_file}"
echo "[structure-drift] report written: ${report_file#${repo_root}/}"

violations=()
if (( ${#oversized_lines[@]} > max_oversized_files )); then
  violations+=("oversized file count ${#oversized_lines[@]} exceeds max ${max_oversized_files}")
fi
if (( ${#dead_code_lines[@]} > max_dead_code_suppressions )); then
  violations+=("dead-code suppressions ${#dead_code_lines[@]} exceeds max ${max_dead_code_suppressions}")
fi
if (( ${#too_many_args_lines[@]} > max_too_many_arguments_suppressions )); then
  violations+=("too-many-arguments suppressions ${#too_many_args_lines[@]} exceeds max ${max_too_many_arguments_suppressions}")
fi

if (( ${#violations[@]} > 0 )); then
  case "${gate_mode}" in
    off)
      ;;
    warn)
      echo "[structure-drift] threshold warnings:" >&2
      printf '  - %s\n' "${violations[@]}" >&2
      ;;
    fail)
      echo "[structure-drift] threshold gate failed:" >&2
      printf '  - %s\n' "${violations[@]}" >&2
      exit 1
      ;;
  esac
fi
