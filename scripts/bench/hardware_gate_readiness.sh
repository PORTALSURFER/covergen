#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  scripts/bench/hardware_gate_readiness.sh status
  scripts/bench/hardware_gate_readiness.sh assert-ready
  scripts/bench/hardware_gate_readiness.sh enable
  scripts/bench/hardware_gate_readiness.sh disable

Environment overrides:
  COVERGEN_GH_REPO               default: inferred from git remote, else PORTALSURFER/covergen
  COVERGEN_HARDWARE_GATE_VAR     default: COVERGEN_ENABLE_HARDWARE_TIER_GATES
EOF
}

cmd="${1:-status}"
if [[ $# -gt 1 ]]; then
  usage
  exit 1
fi

case "${cmd}" in
  status|assert-ready|enable|disable) ;;
  *)
    echo "unknown command: ${cmd}" >&2
    usage
    exit 1
    ;;
esac

require_tool() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "missing required tool: $1" >&2
    exit 1
  fi
}

infer_repo() {
  if [[ -n "${COVERGEN_GH_REPO:-}" ]]; then
    echo "${COVERGEN_GH_REPO}"
    return
  fi

  if command -v gh >/dev/null 2>&1; then
    local inferred
    inferred="$(gh repo view --json nameWithOwner --jq '.nameWithOwner' 2>/dev/null || true)"
    if [[ -n "${inferred}" ]]; then
      echo "${inferred}"
      return
    fi
  fi

  echo "PORTALSURFER/covergen"
}

repo="$(infer_repo)"
var_name="${COVERGEN_HARDWARE_GATE_VAR:-COVERGEN_ENABLE_HARDWARE_TIER_GATES}"
required_labels=(
  "covergen-desktop-mid"
  "covergen-laptop-integrated"
)
required_tiers=(
  "desktop_mid"
  "laptop_integrated"
)
required_sections=(
  "v2_compile"
  "v2_still"
  "v2_animation"
)

gh_labels_online() {
  gh api "repos/${repo}/actions/runners" --jq \
    '.runners[] | select(.status=="online") | .labels[]?.name' \
    | sort -u
}

gh_var_value() {
  gh variable get "${var_name}" --repo "${repo}" --json value --jq '.value' 2>/dev/null || true
}

threshold_file_is_locked() {
  local tier="$1"
  local path="docs/v2/benchmarks/${tier}.thresholds.ini"
  local section

  if [[ ! -f "${path}" ]]; then
    return 1
  fi
  if grep -q 'LOCK REQUIRED' "${path}"; then
    return 1
  fi
  for section in "${required_sections[@]}"; do
    if ! grep -q "^\\[${section}\\]$" "${path}"; then
      return 1
    fi
  done
  return 0
}

print_status() {
  local labels_online="$1"
  local var_value="$2"
  local label
  local tier

  echo "repo: ${repo}"
  echo "hardware gate variable: ${var_name}=${var_value:-unset}"
  echo "online runner labels:"
  if [[ -n "${labels_online}" ]]; then
    while IFS= read -r label; do
      [[ -z "${label}" ]] && continue
      echo "  - ${label}"
    done <<<"${labels_online}"
  else
    echo "  - (none)"
  fi

  echo "required runner labels:"
  for label in "${required_labels[@]}"; do
    if grep -Fxq "${label}" <<<"${labels_online}"; then
      echo "  - ${label}: ok"
    else
      echo "  - ${label}: missing"
    fi
  done

  echo "tier threshold files:"
  for tier in "${required_tiers[@]}"; do
    if threshold_file_is_locked "${tier}"; then
      echo "  - ${tier}: locked"
    else
      echo "  - ${tier}: pending lock"
    fi
  done
}

is_ready() {
  local labels_online="$1"
  local label
  local tier

  for label in "${required_labels[@]}"; do
    if ! grep -Fxq "${label}" <<<"${labels_online}"; then
      return 1
    fi
  done

  for tier in "${required_tiers[@]}"; do
    if ! threshold_file_is_locked "${tier}"; then
      return 1
    fi
  done
  return 0
}

require_tool gh
if ! gh auth status >/dev/null 2>&1; then
  echo "gh is not authenticated; run 'gh auth login' first" >&2
  exit 1
fi

labels_online="$(gh_labels_online)"
var_value="$(gh_var_value)"
print_status "${labels_online}" "${var_value}"

case "${cmd}" in
  status)
    ;;
  assert-ready)
    if is_ready "${labels_online}"; then
      echo "hardware gate readiness: ready"
      exit 0
    fi
    echo "hardware gate readiness: not ready" >&2
    exit 1
    ;;
  enable)
    if ! is_ready "${labels_online}"; then
      echo "cannot enable hardware tier gates: readiness checks failed" >&2
      exit 1
    fi
    gh variable set "${var_name}" --repo "${repo}" --body "true"
    echo "set ${var_name}=true"
    ;;
  disable)
    gh variable set "${var_name}" --repo "${repo}" --body "false"
    echo "set ${var_name}=false"
    ;;
esac
