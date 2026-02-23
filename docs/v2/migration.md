# V1 to V2 Migration Notes

## Current Contract

- `covergen` runs V1 (`src/engine.rs`) as the legacy compatibility path.
- `covergen v2 ...` runs the V2 graph runtime (`src/v2/*`).
- V1 is intentionally retained short-term while V2 rollout gates are validated.

## Migration Phases

### Phase 0: Parallel Stabilization (current)

- Keep V1 as default path.
- Expand and stabilize V2 graph/runtime features.
- Collect benchmark and visual regression evidence for cutover.

### Phase 1: Cutover Decision

Trigger a cutover decision only when all gates below are met in CI/local release runs:

1. Performance gate:
   - Benchmark report generated via `cargo run -- bench`.
   - V2 still render p95 latency is at or below agreed target for primary preset/profile set.
   - V2 animation throughput and p95 frame-time satisfy reels target profile.
2. Determinism/visual gate:
   - V2 fixed-seed still snapshot tests pass.
   - V2 sampled animation-frame snapshot tests pass.
3. Coverage gate:
   - Preset library uses graph-native node topology (branch/merge), not only layer stacks.
   - Critical node operators (source/mask/blend/tone/warp) exercised by tests.
4. Operational gate:
   - Benchmark + regression workflows are documented and repeatable from a clean checkout.

### Phase 2: Default Switch (after decision)

- Make V2 the default command path.
- Keep V1 reachable via explicit legacy invocation for one deprecation window.
- Monitor benchmark deltas and regression failures across at least one release cycle.

### Phase 3: V1 Deprecation

- Announce V1 deprecation window end.
- Remove V1 default-path wiring after V2 coverage and quality remain stable through the window.
- Keep migration notes and historical benchmark/regression artifacts for traceability.

## Decision Artifacts

Before approving cutover, capture and review:

- `target/bench/benchmark_report.md` from representative hosts.
- Snapshot test pass results for V2 still + animation sampled frames.
- Any known visual differences and accepted exceptions (if any).

## Current State Summary

- V2 now has graph-native presets, runtime telemetry, and benchmark reporting.
- V2 deterministic visual regression tests exist for fixed-seed stills and sampled animation frames.
- V1 remains available as a compatibility path until cutover gates are explicitly signed off.
