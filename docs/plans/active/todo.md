# Active TODO (Ordered)

1. Capture and commit locked benchmark thresholds for target hardware tiers in `docs/v2/benchmarks/*.thresholds.ini` (desktop + laptop classes).
2. After hardware tier files are locked, enable `COVERGEN_ENABLE_HARDWARE_TIER_GATES=true` and verify both self-hosted tier jobs pass in `.github/workflows/perf-gates.yml`.
3. Expand visual regression fixtures for larger output sizes and additional graph-native preset families.
4. Validate multi-output graph contracts in benchmark/regression suites and decide tap-output artifact strategy.
5. Continue rust-gpu shader backend hardening for production-default parity.
