# Active TODO (Ordered)

1. Capture and commit locked benchmark thresholds for target hardware tiers in `docs/v2/benchmarks/*.thresholds.ini` (desktop + laptop classes).
2. After hardware tier files are locked, enable `COVERGEN_ENABLE_HARDWARE_TIER_GATES=true` and verify both self-hosted tier jobs pass in `.github/workflows/perf-gates.yml`.
3. Expand visual regression fixtures for larger output sizes and additional graph-native preset families.
4. Monitor V2-default gate stability through the V1 deprecation window and track any accepted visual/perf exceptions.
5. Prepare and review the V1 removal patch plan for post-window execution (after 2026-05-24).
