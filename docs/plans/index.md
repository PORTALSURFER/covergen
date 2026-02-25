# Plans Index

## Active
- `active/todo.md` — immediate execution queue for Windows-first, GPU-only core delivery under the engine-centric V1 model.

## Scope of Current Plan
- Deliver a node-graph shader/video playground workflow focused on generative art authoring.
- Keep UX and capability targets comparable to leading node tools while maintaining legal separation through original architecture and naming.
- Adopt the engine-centric V1 model (`ResourceKind + ExecutionKind + ClockDomain`) as the architecture baseline.
- Maintain Windows-first, GPU-only runtime and benchmark execution constraints.
- Use RTX 2060 as the baseline perf tier while targeting high-tier gaming GPUs or better for production usage.
- Hit a minimum interactive TOP-viewer target of 60 FPS at 1080p, maintain idle headroom above target, and preserve quality under growing graph complexity.
- Deliver fast export for H.264 and image sequences through GPU-accelerated workflows, with NVENC first and AMF second.
- Prioritize core stability, performance, and core feature depth before extensibility.
- Keep handoff documentation synchronized for stateless agent wake-up.

## Plan Hygiene
- Keep this index high-level.
- Keep step-by-step execution in `active/todo.md`.
