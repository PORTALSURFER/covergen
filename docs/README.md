# Docs Index

## Start Here
- `../scripts/run_agent_request.sh`: mandatory preflight for agent-request housekeeping.
- `../AGENTS.md`: stateless wake-up portal.
- `../MEMORY.md`: current present-tense project status.
- `plans/index.md`: active plan map.

## Active Plans
- `plans/active/todo.md`: immediate ordered next tasks.

## V2 Design Docs
- `v2/engine-v1-playground.md` (canonical v1 engine model and node registry)
- `v2/architecture.md`
- `v2/graph-spec.md`
- `v2/gpu-runtime.md`
- `v2/preset-authoring.md`
- `v2/migration.md`
- `v2/benchmarks/README.md`
- `v2/rust-gpu.md`

## GUI Help Docs
- `help/in_app_help.md` (single-source Markdown catalog for readable docs + in-app `F1` help)

## Example Graphs
- `../examples/graphs/README.md`: loadable GUI graph examples and usage notes.
- `../examples/graphs/circle_noise_feedback_trail.json`: circle + noise + TD-style feedback trail reference graph.

## CI and Validation
- Preflight entrypoint: `../scripts/run_agent_request.sh`
- Local CI entrypoint: `../scripts/ci_local.sh`
- Tier benchmark lock/validate: `../scripts/bench/tier_gate.sh`
