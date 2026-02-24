# V1 to V2 Migration Notes

## Current Contract

- `covergen` runs V2 (`src/v2/*`) by default.
- `covergen v2 ...` runs V2 explicitly.
- `covergen v1` is no longer supported (removed from CLI dispatch in `src/main.rs`).

## Cutover + Deprecation Timeline

- V2 default-path cutover started on **February 23, 2026**.
- A V1 deprecation window was announced for **February 23, 2026** through **May 24, 2026**.
- V1 CLI mode was retired early on **February 24, 2026** after stability-gate review.

## Final Checklist Status

1. CLI contract
   - Complete: default and explicit paths are V2-only (`covergen`, `covergen v2 ...`).
   - Complete: `covergen v1` now returns a hard deprecation error.
2. CI performance gate
   - Complete: benchmark threshold workflow exists in `.github/workflows/perf-gates.yml`.
   - Complete: CI software threshold baseline is locked at `.github/bench/ci_software.thresholds.ini`.
3. CI visual regression gate
   - Complete: fixed-seed still + sampled animation snapshot tests run in `.github/workflows/perf-gates.yml`.
4. Documentation + handoff
   - Complete: README mode contract and migration timeline updated.
   - Complete: migration notes, active TODO, and memory state aligned to V2-only operation.

## Post-Deprecation Notes

- V1 rendering code remains in-repo for internal benchmark/comparison workflows only.
- User-facing runtime support is V2-only.

## Evidence Artifacts

- CLI dispatch: `src/main.rs`
- CI gate workflow: `.github/workflows/perf-gates.yml`
- CI software thresholds: `.github/bench/ci_software.thresholds.ini`
- Runtime benchmark report output: `target/bench_ci_software/benchmark_report.md`
- Runtime benchmark metrics output: `target/bench_ci_software/benchmark_metrics.ini`
