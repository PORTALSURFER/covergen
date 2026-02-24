# Active TODO (Ordered)

1. [ ] Provision local hardware-tier hosts (`desktop_mid`, `laptop_integrated`) as the authoritative CI environments.
2. [ ] Lock real hardware thresholds on each tier host via `scripts/ci_local.sh lock <tier>` (PowerShell: `scripts/ci_local.ps1 lock <tier>`).
3. [ ] Run full local CI validation on both tiers via `scripts/ci_local.sh validate <tier>` (PowerShell: `scripts/ci_local.ps1 validate <tier>`).
4. [ ] Capture and store local CI evidence artifacts (`benchmark_report.md`, `benchmark_metrics.ini`) for both tiers in the release handoff (PowerShell: `scripts/bench/store_handoff_artifacts.ps1 -Tier <tier>` or `scripts/ci_local.ps1 validate <tier> -CaptureHandoff`).
5. [x] Continue rust-gpu shader backend hardening for production-default parity.

Status notes (2026-02-24):
- Items 1-4 are blocked on external infrastructure: required local tier hosts are not yet available and hardware-tier threshold files are still placeholder `LOCK REQUIRED` files.
- Local CI now auto-builds SPIR-V artifacts when missing via `scripts/shaders/build_rust_gpu_artifacts.sh` or `scripts/shaders/build_rust_gpu_artifacts.ps1`.
- Item 5 is complete for current scope: runtime is strict SPIR-V only (`src/shaders.rs`) and runtime/bench enforce a hardware-GPU requirement (`src/runtime.rs`, `src/bench/mod.rs`).

Completed 2026-02-24:
- Expanded visual regression coverage with larger still outputs, additional animation sampling, and stronger GPU-path confidence checks.
- Added benchmark/regression contract checks for primary+tap outputs and documented tap-output compositor strategy.
