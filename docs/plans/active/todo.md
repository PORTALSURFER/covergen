# Active TODO (Ordered)

1. Register self-hosted hardware runners for `covergen-desktop-mid` and `covergen-laptop-integrated`, then lock real tier thresholds via `scripts/bench/tier_gate.sh lock <tier>`.
2. After both tier files are locked, enable `COVERGEN_ENABLE_HARDWARE_TIER_GATES=true` and verify both self-hosted tier jobs pass in `.github/workflows/perf-gates.yml`.
3. Continue rust-gpu shader backend hardening for production-default parity.

Completed 2026-02-24:
- Expanded visual regression coverage with larger still outputs, additional animation sampling, and stronger GPU-path confidence checks.
- Added benchmark/regression contract checks for primary+tap outputs and documented tap-output artifact strategy (primary-only encode by default).
