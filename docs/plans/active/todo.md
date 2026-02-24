# Active TODO (Ordered)

1. [ ] Register self-hosted hardware runners for `covergen-desktop-mid` and `covergen-laptop-integrated`.
2. [ ] Lock real hardware thresholds on each tier host via `scripts/bench/tier_gate.sh lock <tier>`.
3. [ ] Run `scripts/bench/hardware_gate_readiness.sh assert-ready` and then `scripts/bench/hardware_gate_readiness.sh enable`.
4. [ ] Verify `benchmark-thresholds-hardware` and `visual-regression-gpu` jobs pass in `.github/workflows/perf-gates.yml`.
5. [x] Continue rust-gpu shader backend hardening for production-default parity.

Status notes (2026-02-24):
- Items 1-4 are blocked on external infrastructure: repository currently reports zero registered self-hosted runners and hardware-tier threshold files are still placeholder `LOCK REQUIRED` files.
- Item 5 is complete for current scope: runtime is strict SPIR-V only (`src/shaders.rs`), runtime/bench enforce hardware-GPU requirement (`src/v2/runtime.rs`, `src/bench/mod.rs`), and hardware CI jobs validate shader artifacts before workload execution (`.github/workflows/perf-gates.yml`).

Completed 2026-02-24:
- Expanded visual regression coverage with larger still outputs, additional animation sampling, and stronger GPU-path confidence checks.
- Added benchmark/regression contract checks for primary+tap outputs and documented tap-output artifact strategy (primary-only encode by default).
