# AGENTS.md

## Purpose
This file is a stateless wake-up portal. It should stay minimal and point to the current mission, active queue, and architecture docs.

## 60-Second Wake-Up
1. Read `MEMORY.md` for current state and constraints.
2. Read `docs/plans/active/todo.md` for the immediate ordered queue.
3. Read `docs/plans/index.md` for plan context.
4. Use `docs/README.md` to find deeper technical docs.

## Active Mission
Stabilize and extend V2 (programmatic node-graph, GPU-retained runtime, and animation path) so it can become the default path.

## Source of Truth
- Current state: `MEMORY.md`
- Immediate execution queue: `docs/plans/active/todo.md`
- Plan map: `docs/plans/index.md`
- Architecture and subsystem docs: `docs/README.md`

## Update Rules
- Keep this file short; no implementation detail.
- Update `MEMORY.md` and `docs/plans/active/todo.md` whenever priorities change.
- Keep links in this file valid.
