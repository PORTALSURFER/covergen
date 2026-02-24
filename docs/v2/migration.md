# V1 to V2 Migration Notes

## Current Contract

- `covergen` runs V2 (`src/*`) by default.
- V1 runtime code and CLI mode are removed.

## Cutover + Deprecation Timeline

- V2 default-path cutover started on **February 23, 2026**.
- A V1 deprecation window was announced for **February 23, 2026** through **May 24, 2026**.
- V1 CLI mode was retired early on **February 24, 2026** after stability-gate review.

## Final Checklist Status

1. CLI contract
   - Complete: default path is V2-only (`covergen`).
   - Complete: V1 runtime path no longer exists in source.
2. Local performance gate
   - Complete: tiered benchmark lock/validate workflow exists via `scripts/ci_local.sh` and `scripts/bench/tier_gate.sh`.
   - In progress: hardware-tier threshold files need lock refresh on real tier hosts.
3. Local visual regression gate
   - Complete: fixed-seed still + sampled animation snapshot tests are included in `scripts/ci_local.sh`.
4. Documentation + handoff
   - Complete: README mode contract and migration timeline updated.
   - Complete: migration notes, active TODO, and memory state aligned to V2-only operation.

## Post-Deprecation Notes

- Runtime and benchmark execution are V2-only.

## Evidence Artifacts

- CLI dispatch: `src/main.rs`
- Local CI runner script: `scripts/ci_local.sh`
- Tier gate script: `scripts/bench/tier_gate.sh`
- Tier thresholds: `docs/v2/benchmarks/desktop_mid.thresholds.ini`, `docs/v2/benchmarks/laptop_integrated.thresholds.ini`
- Runtime benchmark report output: `target/bench/<tier-name>/benchmark_report.md`
- Runtime benchmark metrics output: `target/bench/<tier-name>/benchmark_metrics.ini`
