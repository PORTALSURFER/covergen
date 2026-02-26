# In-App Help Catalog

This Markdown file is the source of truth for contextual `F1` help inside the
graph editor.

## Global
- Hover a node body or parameter row, then press `F1` for context help.
- Press `F1` again, left-click, or right-click to close help.
- Node help shows workflow notes plus live parameter values and bindings.

## Node `tex.solid`
Generates a full-frame constant RGBA texture.
Use this as a base source color for downstream texture chains.
### Param `color_r`
Red channel multiplier for the generated solid.
### Param `color_g`
Green channel multiplier for the generated solid.
### Param `color_b`
Blue channel multiplier for the generated solid.
### Param `alpha`
Alpha channel output for the generated solid.

## Node `tex.circle`
Renders a procedural circle over transparent background.
Use center, radius, and feather for soft-edged masks or shape sources.
### Param `center_x`
Horizontal center in normalized texture space.
### Param `center_y`
Vertical center in normalized texture space.
### Param `radius`
Circle radius in normalized texture space.
### Param `feather`
Edge falloff width for anti-aliased soft boundaries.
### Param `color_r`
Red channel multiplier for circle fill color.
### Param `color_g`
Green channel multiplier for circle fill color.
### Param `color_b`
Blue channel multiplier for circle fill color.
### Param `alpha`
Alpha channel multiplier for circle fill color.

## Node `buf.sphere`
Creates sphere geometry in buffer space.
Commonly routed into `scene.entity` for scene assembly.
### Param `radius`
Sphere radius in buffer/object space units.
### Param `segments`
Horizontal tessellation count; higher values increase detail and cost.
### Param `rings`
Vertical tessellation count; higher values increase detail and cost.

## Node `buf.circle_nurbs`
Generates circle/arc curve geometry with configurable tessellation.
Useful for line/curve based scene elements.
### Param `radius`
Base radius of the generated curve.
### Param `arc_start`
Arc start angle in degrees.
### Param `arc_end`
Arc end angle in degrees.
### Param `arc_style`
Closed circle or open arc mode.
### Param `line_width`
Render-space width parameter for line-like output.
### Param `order`
Curve interpolation order.
### Param `divisions`
Subdivision density along the curve.

## Node `buf.noise`
Applies procedural deformation to incoming buffer geometry.
Use `loop_mode=loop` for deterministic seamless timeline loops.
### Param `amplitude`
Overall displacement strength.
### Param `frequency`
Spatial frequency of the deformation pattern.
### Param `speed_hz`
Temporal playback speed for animated noise phase.
### Param `phase`
Phase offset applied to the procedural function.
### Param `seed`
Deterministic random seed.
### Param `twist`
Additional rotational distortion amount.
### Param `stretch`
Directional anisotropic stretch amount.
### Param `loop_cyc`
Cycle length used by loop mode timing.
### Param `loop_mode`
Free-running or timeline-locked seamless loop behavior.

## Node `tex.transform_2d`
Applies per-channel gain and brightness to input textures.
Defaults are identity so insertion does not alter output.
### Param `brightness`
Global brightness multiplier.
### Param `gain_r`
Red channel gain multiplier.
### Param `gain_g`
Green channel gain multiplier.
### Param `gain_b`
Blue channel gain multiplier.
### Param `alpha_mul`
Alpha channel multiplier.

## Node `tex.level`
Performs level remapping and gamma shaping on input textures.
Defaults are identity for non-destructive insertion.
### Param `in_low`
Input low clamp/remap threshold.
### Param `in_high`
Input high clamp/remap threshold.
### Param `gamma`
Gamma curve exponent.
### Param `out_low`
Output low remap target.
### Param `out_high`
Output high remap target.

## Node `tex.feedback`
Mixes current input with persistent accumulation history.
Input pin always provides current frame source; accumulation texture controls history storage.
### Param `accumulation_tex`
Optional external history texture target for read/write accumulation.
Leave unbound to use the node's internal persistent history buffer.
### Param `feedback`
History mix amount; higher values preserve more prior-frame content.

## Node `tex.blend`
Composites primary input with optional secondary texture.
Similar to common DCC compositing workflows.
### Param `blend_tex`
Optional secondary texture input used by blend operations.
### Param `blend_mode`
Blend equation preset (normal/add/subtract/multiply/screen/overlay/darken/lighten/difference).
### Param `opacity`
Blend contribution amount from `blend_tex`.

## Node `scene.entity`
Binds buffer geometry into scene entity data with transform/material-like controls.
Multiple entities can be aggregated by `scene.build`.
### Param `pos_x`
Entity horizontal position in scene space.
### Param `pos_y`
Entity vertical position in scene space.
### Param `scale`
Uniform entity scale.
### Param `ambient`
Ambient lighting contribution scalar.
### Param `color_r`
Entity red color component.
### Param `color_g`
Entity green color component.
### Param `color_b`
Entity blue color component.
### Param `alpha`
Entity alpha component.

## Node `scene.build`
Aggregates one or more entity streams into a renderable scene payload.
No editable parameters on this node.

## Node `render.camera`
Configures camera behavior for scene rendering.
### Param `zoom`
Camera zoom factor.

## Node `render.scene_pass`
Renders scene input to a texture output pass.
Background mode controls alpha behavior for compositing pipelines.
### Param `res_width`
Render width override; `0` keeps project preview width.
### Param `res_height`
Render height override; `0` keeps project preview height.
### Param `bg_mode`
Background behavior: keep clear/background or alpha-clip to geometry.
### Param `edge_softness`
Edge smoothing/softness control in scene shading.
### Param `light_x`
Lighting direction X component.
### Param `light_y`
Lighting direction Y component.
### Param `light_z`
Lighting direction Z component.

## Node `ctl.lfo`
Generates a looping scalar modulation signal for parameter binds.
### Param `rate_hz`
Oscillation rate in hertz.
### Param `amplitude`
Output oscillation amplitude.
### Param `phase`
Oscillation phase offset.
### Param `bias`
Constant output offset applied after oscillation.

## Node `io.window_out`
Final output sink that presents the incoming texture to the window.
No editable parameters on this node.
