# AGENTS.md

## Purpose
This file is a stateless wake-up portal. It should stay minimal and point to the current mission, active queue, and architecture docs.

## 60-Second Wake-Up
1. Run `bash scripts/run_agent_request.sh` (preflight).
2. Read `MEMORY.md` for current state and constraints.
3. Read `docs/plans/active/todo.md` for the immediate ordered queue.
4. Read `docs/plans/index.md` for plan context.
5. Use `docs/README.md` to find deeper technical docs.

## Active Mission
Build a Windows-first, GPU-only node-graph shader/video playground with high-performance real-time output and fast GPU-accelerated export, while keeping architecture and terminology legally distinct.

## Source of Truth
- Current state: `MEMORY.md`
- Immediate execution queue: `docs/plans/active/todo.md`
- Plan map: `docs/plans/index.md`
- Architecture and subsystem docs: `docs/README.md`
- Performance ROI backlog (temporary): `tmp/perf_plan.md`
- Cleanup ROI backlog (temporary, current audit): `tmp/cleanup_plan.md`

## Update Rules
- Keep this file short; no implementation detail.
- Keep status and task detail out of this file; store that in `MEMORY.md` and `docs/plans/active/todo.md`.
- Update `MEMORY.md` and `docs/plans/active/todo.md` whenever priorities change.
- Keep links in this file valid.
