# V1 to V2 Migration Notes

## Current Contract

- `covergen` runs V2 (`src/v2/*`) by default.
- `covergen v2 ...` runs V2 explicitly.
- `covergen v1` runs the legacy V1 compatibility path (`src/engine.rs`).

## Cutover + Deprecation Window

- Default-path cutover to V2 is active as of **February 23, 2026**.
- V1 deprecation window is active from **February 23, 2026** through **May 24, 2026**.
- During this window, V1 remains supported only through explicit `covergen v1`.

## Cutover Checklist Status

1. CLI contract
   - Complete: explicit `covergen v1` mode exists.
   - Complete: default path routes to V2 (`src/main.rs`).
2. CI performance gate
   - Complete: benchmark threshold workflow exists in `.github/workflows/perf-gates.yml`.
   - Complete: CI software threshold baseline is locked at `.github/bench/ci_software.thresholds.ini`.
3. CI visual regression gate
   - Complete: fixed-seed still + sampled animation snapshot tests run in `.github/workflows/perf-gates.yml`.
4. Documentation + handoff
   - Complete: README mode contract and deprecation dates updated.
   - Complete: migration notes, active TODO, and memory state aligned.

## Remaining Migration Work

- Capture and lock threshold files for target non-CI hardware tiers in `docs/v2/benchmarks/*.thresholds.ini`.
- Monitor benchmark + snapshot gate stability through the full deprecation window.
- Decide V1 removal after deprecation window closes and stability criteria remain green.

## Evidence Artifacts

- CI gate workflow: `.github/workflows/perf-gates.yml`
- CI software thresholds: `.github/bench/ci_software.thresholds.ini`
- Runtime benchmark report output: `target/bench_ci_software/benchmark_report.md`
- Runtime benchmark metrics output: `target/bench_ci_software/benchmark_metrics.ini`
