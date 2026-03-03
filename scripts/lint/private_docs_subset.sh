#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
manifest_path="${1:-${repo_root}/scripts/lint/private_docs_subset_files.txt}"

if [[ ! -f "${manifest_path}" ]]; then
  echo "[private-docs] missing manifest: ${manifest_path}" >&2
  exit 1
fi

errors=0
while IFS= read -r rel_path; do
  [[ -z "${rel_path}" || "${rel_path}" =~ ^[[:space:]]*# ]] && continue
  file_path="${repo_root}/${rel_path}"
  if [[ ! -f "${file_path}" ]]; then
    echo "[private-docs] listed file not found: ${rel_path}" >&2
    errors=1
    continue
  fi

  if ! awk -v file="${rel_path}" '
function trim(s) {
  sub(/^[ \t]+/, "", s);
  sub(/[ \t]+$/, "", s);
  return s;
}
{
  line = $0;

  if (line ~ /^[[:space:]]*\/\/\// || line ~ /^[[:space:]]*\/\/!/) {
    saw_doc = 1;
    next;
  }

  if (line ~ /^[[:space:]]*#\[/) {
    next;
  }

  if (line ~ /^[[:space:]]*$/) {
    saw_doc = 0;
    next;
  }

  is_item = (line ~ /^[[:space:]]*pub(\(crate\))?[[:space:]]+((async|const|unsafe)[[:space:]]+)*(fn|struct|enum|const|static|mod|trait|type)\b/);
  is_field = (line ~ /^[[:space:]]*pub(\(crate\))?[[:space:]]+[A-Za-z_][A-Za-z0-9_]*[[:space:]]*:/);

  if ((is_item || is_field) && !saw_doc) {
    printf "%s:%d missing doc comment for `%s`\n", file, NR, trim(line);
    missing = 1;
  }

  saw_doc = 0;
}
END {
  if (missing) {
    exit 2;
  }
}
' "${file_path}"; then
    errors=1
  fi
done < "${manifest_path}"

if [[ "${errors}" -ne 0 ]]; then
  echo "[private-docs] documentation lint failed" >&2
  exit 1
fi

echo "[private-docs] documentation lint passed"
