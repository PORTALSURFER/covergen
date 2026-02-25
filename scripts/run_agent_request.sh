#!/usr/bin/env bash
set -euo pipefail

# Preflight guard for one-shot agent requests. Keep this fast and deterministic.

required_files=(
  "AGENTS.md"
  "MEMORY.md"
  "docs/README.md"
  "docs/plans/index.md"
  "docs/plans/active/todo.md"
)

for file in "${required_files[@]}"; do
  if [[ ! -f "${file}" ]]; then
    echo "[preflight] missing required file: ${file}" >&2
    exit 1
  fi
done

if ! rg -q "docs/plans/active/todo.md" AGENTS.md; then
  echo "[preflight] AGENTS.md must link to docs/plans/active/todo.md" >&2
  exit 1
fi

if ! rg -q "^\s*[0-9]+\.\s+\[.\]\s" docs/plans/active/todo.md; then
  echo "[preflight] docs/plans/active/todo.md must contain an ordered queue" >&2
  exit 1
fi

echo "[preflight] agent request preflight passed"
