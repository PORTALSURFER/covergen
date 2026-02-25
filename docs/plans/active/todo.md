# Active TODO (Ordered)

1. [ ] Write and ratify a Windows-first, GPU-only product contract doc covering runtime assumptions, unsupported configurations, and export constraints.
2. [ ] Define and implement deterministic performance gates for interactive TOP viewing on RTX 2060 baseline hardware: real-time target `>=60 FPS @1080p`, idle headroom primary gate `p95 frametime <= 10 ms`, secondary gate `average FPS >= 90` in empty/light scenes.
3. [ ] Profile node-editor and TOP-viewer hotspots on representative heavy graphs; ship the highest-impact optimization without visual regressions.
4. [ ] Implement and validate a GPU-accelerated export path for H.264 and image sequences, with zero-readback transfer as the target architecture and Windows sequencing of NVENC first, then AMF.
5. [ ] Build out core TouchDesigner-style node capabilities in prioritized slices, focusing first on stable and performant essentials.
6. [ ] Reduce dead-code warning surface in core runtime modules so checks stay signal-heavy for regressions.

Status notes (2026-02-25):
- Mission focus is a TouchDesigner-style Windows-first, GPU-only generative art workflow.
- Runtime is always real-time with user-selected target FPS (typically 60 FPS), with headroom required in idle/low-complexity scenes.
- Baseline perf tier is RTX 2060; target deployment class is high-tier gaming GPUs or better.
- Export scope is currently H.264 and image sequences via GPU-accelerated workflows.
- H.264 rollout order is NVENC first, then AMF.
- `scripts/run_agent_request.sh` is the mandatory preflight entrypoint for agent housekeeping requests.
