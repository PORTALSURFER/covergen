# Active TODO (Ordered)

1. Register self-hosted hardware runners for `covergen-desktop-mid` and `covergen-laptop-integrated`.
2. Lock real hardware thresholds on each tier host via `scripts/bench/tier_gate.sh lock <tier>`.
3. Run `scripts/bench/hardware_gate_readiness.sh assert-ready` and then `scripts/bench/hardware_gate_readiness.sh enable`.
4. Verify `benchmark-thresholds-hardware` and `visual-regression-gpu` jobs pass in `.github/workflows/perf-gates.yml`.
5. Continue rust-gpu shader backend hardening for production-default parity.

Completed 2026-02-24:
- Expanded visual regression coverage with larger still outputs, additional animation sampling, and stronger GPU-path confidence checks.
- Added benchmark/regression contract checks for primary+tap outputs and documented tap-output artifact strategy (primary-only encode by default).
