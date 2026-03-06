# Geometry Node Roadmap

## Purpose
Add a staged geometry-node program that grows the current analytic buffer-to-scene pipeline into a broader procedural geometry toolkit without pretending the engine already has a general mesh/SOP runtime.

## Architecture Constraint
- The current `buf.* -> scene.* -> render.scene_pass` path renders analytic impostor primitives through fullscreen GPU passes.
- Phase 1 should therefore favor primitives that can be represented analytically with stable, cheap shading.
- Surface-construction nodes such as `extrude`, `revolve`, `sweep`, `skin`, `metaball`, and `lsystem` need an explicit geometry-runtime expansion after the initial primitive tranche.

## Delivery Phases

### Phase 1: Analytic Primitive Sources
Goal: rapidly broaden the geometry vocabulary inside the existing renderer.

Nodes:
- `buf.box`
- `buf.grid`
- `buf.tube`
- `buf.torus`

Implementation notes:
- Keep these as analytic scene primitives, not general mesh buffers.
- Reuse the existing `scene.entity -> scene.build -> render.scene_pass` chain.
- Preserve `buf.noise` compatibility where shape semantics are clear.
- Add one example graph that stages the new primitives together.

Exit criteria:
- All four nodes appear in Add Node, persist, compile, render, and participate in scene graphs.
- Runtime tests and tex-preview tests cover parameter propagation and scene-pass emission.

### Phase 2: Profile and Surface Builders
Goal: move from isolated primitives to authored surfaces.

Nodes:
- `buf.line`
- `buf.curve`
- `buf.extrude`
- `buf.revolve`
- `buf.sweep`

Implementation notes:
- Introduce an explicit profile/path representation instead of overloading primitive-only state.
- Add typed geometry sub-kinds so `sweep` and `extrude` can reject invalid inputs early.
- Expect this phase to push beyond the current analytic-only model.

Exit criteria:
- At least one profile-to-surface chain can be authored entirely in the GUI with deterministic rendering and persistence coverage.

### Phase 3: Deformers and Instancing
Goal: make geometry graphs composition-friendly.

Nodes:
- `buf.transform`
- `buf.copy_to_points`
- `buf.twist`
- `buf.bend`
- `buf.taper`

Implementation notes:
- Keep parameter containers explicit; do not add high-arity mutation helpers.
- Prefer shape-agnostic transforms and deformations that compose cleanly with Phase 1 and 2 nodes.

Exit criteria:
- Instancing and three deformation workflows are covered by focused runtime tests and one example graph.

### Phase 4: Signature Generators
Goal: deliver the distinctive procedural nodes users expect from TouchDesigner-style workflows.

Nodes:
- `buf.superquad`
- `buf.metaball`
- `buf.lsystem`
- `buf.sprinkle`

Implementation notes:
- `superquad` can likely remain analytic.
- `metaball`, `lsystem`, and `sprinkle` should wait for the richer geometry runtime introduced in Phase 2 and 3.

Exit criteria:
- Each node has clear docs describing constraints and expected performance costs.

### Phase 5: Topology and Cleanup Utilities
Goal: support practical downstream authoring and keep generated geometry usable.

Nodes:
- `buf.subdivide`
- `buf.boolean`
- `buf.polyreduce`
- `buf.resample`
- `buf.convert`

Implementation notes:
- These nodes should not ship as shallow UI stubs.
- Only add each node once a real underlying geometry representation exists for it.

Exit criteria:
- Utilities operate on explicit geometry data, not ad hoc shape-profile flags.

## Immediate Implementation Order
1. [x] `buf.box`
2. [x] `buf.grid`
3. [ ] `buf.tube`
4. [ ] `buf.torus`

## Validation
- `cargo test gui::project:: -- --nocapture`
- `cargo test gui::runtime::tests::scene_ops -- --nocapture`
- `cargo test gui::tex_view::tests::ops_scene -- --nocapture`
- `cargo build --release`

## Notes
- The current renderer is still strongest at stylized 2.5D analytic forms.
- The plan intentionally separates “interesting nodes we should expose” from “nodes the current runtime can already support honestly.”
