# Active TODO (Ordered)

1. [ ] Lock `laptop_integrated` hardware thresholds on the laptop host (`scripts/ci_local.sh lock laptop_integrated`).
2. [ ] Run full local CI validation on the laptop host and capture handoff artifacts (`scripts/ci_local.sh validate laptop_integrated`, then `scripts/bench/store_handoff_artifacts.ps1 -Tier laptop_integrated` on Windows hosts as needed).
3. [ ] Keep `desktop_mid` tier marked deferred until a desktop host is available; once provisioned, run lock + validate + handoff capture.
4. [ ] Reduce dead-code warning surface in core runtime modules so CI logs stay signal-heavy for regressions.

Status notes (2026-02-25):
- `covergen` is V2-only and requires hardware GPU adapters at runtime.
- Local CI is the authoritative gate; GitHub-hosted perf gates are non-authoritative.
- `scripts/run_agent_request.sh` is the mandatory preflight entrypoint for agent housekeeping requests.
