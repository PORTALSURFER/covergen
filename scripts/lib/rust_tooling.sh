#!/usr/bin/env bash

# Shared Rust tooling discovery for local scripts.
#
# Some environments install rustup/cargo under ~/.cargo/bin without adding that
# directory to PATH for non-interactive shells. Script entrypoints source this
# file so benchmark and CI commands remain executable.

rust_user_bin_dir() {
  if [[ -n "${HOME:-}" && -d "${HOME}/.cargo/bin" ]]; then
    printf '%s\n' "${HOME}/.cargo/bin"
    return 0
  fi
  return 1
}

prepend_rust_user_bin_to_path() {
  local rust_bin_dir
  rust_bin_dir="$(rust_user_bin_dir || true)"
  if [[ -z "${rust_bin_dir}" ]]; then
    return 0
  fi

  case ":${PATH:-}:" in
    *":${rust_bin_dir}:"*) ;;
    *) export PATH="${rust_bin_dir}:${PATH:-}" ;;
  esac
}

ensure_rust_command() {
  local command_name="${1:?missing command name}"
  prepend_rust_user_bin_to_path
  if command -v "${command_name}" >/dev/null 2>&1; then
    return 0
  fi

  echo "[rust_tooling] missing required command '${command_name}'." >&2
  echo "[rust_tooling] install rustup/cargo or add ~/.cargo/bin to PATH." >&2
  return 1
}
