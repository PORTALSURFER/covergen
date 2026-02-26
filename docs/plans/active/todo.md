# Active TODO (Ordered)

1. [ ] Implement graph/runtime core types (`ResourceKind`, `ExecutionKind`, `ClockDomain`) and migrate validation/compiler contracts to the engine-centric V1 model.
2. [ ] Implement V1 node registry slice (`io.window_out`, `io.image_load/save`, `tex.solid/noise`, `tex.transform_2d`, `tex.mix`, `tex.shader`, `ctl.time`, `data.params`) with strict typed ports and no implicit conversions.
3. [ ] Implement pull-based evaluation with version/hash dirty propagation and output-driven scheduling.
4. [ ] Implement `Struct -> ParamBlock` explicit bridge and stable uniform layout validation for WGSL nodes.
5. [ ] Define and enforce deterministic performance gates for interactive tex viewing on RTX 2060 baseline hardware: real-time target `>=60 FPS @1080p`, idle headroom primary gate `p95 frametime <= 10 ms`, secondary gate `average FPS >= 90` in empty/light scenes.
6. [ ] Implement and validate a GPU-accelerated export path for H.264 and image sequences, with zero-readback transfer as the target architecture and Windows sequencing of NVENC first, then AMF.
7. [ ] Profile node-editor and tex-viewer hotspots on representative heavy graphs; ship highest-impact optimizations without visual regressions.
8. [ ] Reduce dead-code warning surface in core runtime modules so checks stay signal-heavy for regressions.

Status notes (2026-02-25):
- Mission focus is a Windows-first, GPU-only shader/video playground with node-graph authoring and real-time generative output.
- Canonical design source is `docs/v2/engine-v1-playground.md`.
- Architecture is engine-centric and uses `ResourceKind + ExecutionKind + ClockDomain`.
- UX/capability target is comparable to TouchDesigner-style workflows while maintaining legal separation via original nomenclature and architecture.
- Runtime is always real-time with user-selected target FPS (typically 60 FPS), with headroom required in idle/low-complexity scenes.
- Baseline perf tier is RTX 2060; target deployment class is high-tier gaming GPUs or better.
- Export scope is currently H.264 and image sequences via GPU-accelerated workflows.
- H.264 rollout order is NVENC first, then AMF.
- `scripts/run_agent_request.sh` is the mandatory preflight entrypoint for agent housekeeping requests.
