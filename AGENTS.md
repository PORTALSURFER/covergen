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
Operate and harden the V2-only runtime (GPU-required), local CI/perf baselines, and GUI node-editor responsiveness.

## Source of Truth
- Current state: `MEMORY.md`
- Immediate execution queue: `docs/plans/active/todo.md`
- Plan map: `docs/plans/index.md`
- Architecture and subsystem docs: `docs/README.md`

## Update Rules
- Keep this file short; no implementation detail.
- Update `MEMORY.md` and `docs/plans/active/todo.md` whenever priorities change.
- Keep links in this file valid.
