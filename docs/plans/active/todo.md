# Active TODO (Ordered)

1. [ ] Write and ratify a Windows-first, GPU-only product contract doc covering runtime assumptions, unsupported configurations, and export constraints.
2. [ ] Define and implement deterministic performance gates for interactive TOP viewing (`>=60 FPS @1080p`) plus idle headroom telemetry.
3. [ ] Profile node-editor and TOP-viewer hotspots on representative heavy graphs; ship the highest-impact optimization without visual regressions.
4. [ ] Implement and validate a GPU-accelerated export path for H.264 and image sequences, with zero-readback transfer as the target architecture.
5. [ ] Build out core TouchDesigner-style node capabilities in prioritized slices, focusing first on stable and performant essentials.
6. [ ] Reduce dead-code warning surface in core runtime modules so checks stay signal-heavy for regressions.

Status notes (2026-02-25):
- Mission focus is a TouchDesigner-style Windows-first, GPU-only generative art workflow.
- Minimum interactive bar is 60 FPS at 1080p; higher idle performance headroom is a design target.
- Export scope is currently H.264 and image sequences via GPU-accelerated workflows.
- `scripts/run_agent_request.sh` is the mandatory preflight entrypoint for agent housekeeping requests.
