#!/usr/bin/env bash
set -euo pipefail

root="${1:-target/rust-gpu}"
required=(
  "fractal_main.spv"
  "graph_ops.spv"
  "graph_decode.spv"
  "retained_post.spv"
)

for file in "${required[@]}"; do
  path="${root}/${file}"
  if [[ ! -f "${path}" ]]; then
    echo "missing shader artifact: ${path}" >&2
    exit 1
  fi

  magic="$(xxd -p -l 4 "${path}" | tr -d '\n')"
  if [[ "${magic}" != "03022307" ]]; then
    echo "invalid SPIR-V magic in ${path}: got ${magic}, expected 03022307" >&2
    exit 1
  fi
done

echo "rust-gpu shader artifacts look valid in ${root}"
