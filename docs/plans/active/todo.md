# Active TODO (Ordered)

1. [ ] Keep guardrails green and signal-heavy: run `scripts/ci_local.sh`, fix failures, and continue reducing warning noise that masks regressions.
2. [ ] Close remaining engine-model migration gaps so runtime/compiler/validation consistently enforce `ResourceKind + ExecutionKind + ClockDomain`.
3. [ ] Finish the immediate V1 node-registry delivery slice with strict typed ports and no implicit conversions.
4. [ ] Complete output-driven pull evaluation with deterministic dirty propagation and coverage for failure/boundary paths.
5. [ ] Advance Windows GPU export throughput path for H.264/image sequence workflows, sequencing NVENC first and AMF second, targeting zero-readback where feasible.
6. [ ] Keep handoff docs synchronized (`AGENTS.md`, `MEMORY.md`, `docs/plans/active/todo.md`) whenever priorities or status change.

Status notes (2026-03-02):
- Mission focus is a Windows-first, GPU-only shader/video playground with node-graph authoring and real-time generative output.
- Canonical design source is `docs/v2/engine-v1-playground.md`.
- Architecture is engine-centric and uses `ResourceKind + ExecutionKind + ClockDomain`.
- UX/capability target is comparable to high-end node-graph operator workflows while maintaining legal separation via original nomenclature and architecture.
- Runtime is always real-time with user-selected target FPS (typically 60 FPS), with headroom required in idle/low-complexity scenes.
- Baseline perf tier is RTX 2060; target deployment class is high-tier gaming GPUs or better.
- Export scope is currently H.264 and image sequences via GPU-accelerated workflows.
- H.264 rollout order is NVENC first, then AMF.
- `scripts/run_agent_request.sh` is the mandatory preflight entrypoint for agent housekeeping requests.
- Performance ROI cycle in `tmp/perf_plan.md` is implemented for items 1-10; hardware-tier threshold locking remains blocked until representative GPU hosts are available.
