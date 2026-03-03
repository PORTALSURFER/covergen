#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
report_dir="${repo_root}/target/ci"
report_file="${report_dir}/structure_drift_report.txt"

mkdir -p "${report_dir}"

{
  echo "[structure-drift] generated: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo
  echo "[structure-drift] oversized Rust files (>400 LOC)"
  while IFS= read -r file; do
    lines="$(wc -l < "${file}")"
    if (( lines > 400 )); then
      printf '%6d %s\n' "${lines}" "${file#${repo_root}/}"
    fi
  done < <(cd "${repo_root}" && rg --files src -g '*.rs' | sort)
  echo
  echo "[structure-drift] dead-code lint suppressions"
  (cd "${repo_root}" && rg -n "allow\(dead_code\)" src scripts) || true
  echo
  echo "[structure-drift] too-many-arguments lint suppressions"
  (cd "${repo_root}" && rg -n "allow\(clippy::too_many_arguments\)" src scripts) || true
} > "${report_file}"

cat "${report_file}"
echo "[structure-drift] report written: ${report_file#${repo_root}/}"
