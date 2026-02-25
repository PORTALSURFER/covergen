#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${repo_root}"

# shellcheck source=../lib/rust_tooling.sh
source "${repo_root}/scripts/lib/rust_tooling.sh"
ensure_rust_command cargo

cargo run --quiet --bin covergen -- bench "$@"
