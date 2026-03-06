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

## Node `tex.source_noise`
Generates deterministic monochrome procedural noise.
Use this as a seed field for masks, reaction-diffusion, and warp-driven organic textures.
### Param `seed`
Deterministic noise seed controlling the sampled pattern.
### Param `scale`
Spatial scale of the noise field; higher values add denser features.
### Param `octaves`
Number of layered octave bands used in the noise sum.
### Param `amplitude`
Output gain applied after octave normalization.

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
Opacity/energy multiplier applied to the transformed output (alpha and visible color).

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

## Node `tex.mask`
Extracts a soft threshold mask from incoming texture luma.
Use this to isolate veins, voids, or bright detail before compositing.
### Param `threshold`
Luma threshold pivot used for mask extraction.
### Param `softness`
Transition width around the threshold edge.
### Param `invert`
Flips the mask so dark regions become selected instead of bright ones.

## Node `tex.tone_map`
Applies percentile clamping and contrast shaping to input luma.
Use this after structure-building stages to push marbled forms into cleaner dark/light separation.
### Param `contrast`
Contrast multiplier applied after percentile normalization.
### Param `low_pct`
Lower percentile-style clamp controlling the black point.
### Param `high_pct`
Upper percentile-style clamp controlling the white point.

## Node `tex.feedback`
Outputs delayed persistent history (feedback tap).
Input pin writes the next history frame; displayed output is prior history.
### Param `accumulation_tex`
Optional external history texture target for read/write delayed state.
Leave unbound to use the node's internal persistent history buffer.
### Param `feedback`
History output gain (`history * feedback`).
### Param `frame_gap`
Extra hold frames between history writes (`0` = update every frame).
### Param `reset`
Clears persistent feedback history for this node.

## Node `tex.reaction_diffusion`
Runs one Gray-Scott reaction-diffusion simulation step per frame.
Primary input provides seed concentrations (`R = A`, `G = B`) for initialization/injection.
### Param `diff_a`
Diffusion coefficient for reagent `A`.
### Param `diff_b`
Diffusion coefficient for reagent `B`.
### Param `feed`
Feed rate replenishing reagent `A`.
### Param `kill`
Kill rate removing reagent `B`.
### Param `dt`
Per-frame integration step multiplier.
### Param `seed_mix`
Blend amount for injecting source concentrations into the evolving state.

## Node `tex.warp_transform`
Applies a lightweight deterministic UV warp to the input texture.
Use this to bend diffusion structures and feed subtle directional flow into feedback.
### Param `strength`
Overall UV offset strength of the warp.
### Param `frequency`
Spatial frequency of the sinusoidal warp field.
### Param `phase`
Phase offset used to animate or offset the warp pattern.

## Node `tex.post_color_tone`
Color and tone post-processing category node.
Use `effect` to choose bloom/tone-map/grading style operators.
### Param `effect`
Selects color/tone effect preset for this category node.
### Param `amount`
Master blend amount of the selected effect.
### Param `scale`
Radius/strength scale used by effect kernels and remaps.
### Param `thresh`
Threshold gate for bright-pass or contrast-sensitive effects.
### Param `speed`
Animation rate for time-varying variants.

## Node `tex.post_edge_structure`
Edge and structure post-processing category node.
Includes edge detect, toon edge, emboss, sharpen, and painterly variants.
### Param `effect`
Selects edge/structure effect preset.
### Param `amount`
Master blend amount of the selected effect.
### Param `scale`
Kernel radius and edge amplification scale.
### Param `thresh`
Edge threshold used by contour-style modes.
### Param `speed`
Animation rate for time-varying variants.

## Node `tex.post_blur_diffusion`
Blur and diffusion post-processing category node.
Includes gaussian/box/kawase/radial/motion-style blur variants.
### Param `effect`
Selects blur/diffusion effect preset.
### Param `amount`
Master blend amount of the selected effect.
### Param `scale`
Blur radius or accumulation scale.
### Param `thresh`
Threshold gate for selective blur.
### Param `speed`
Animation rate for time-varying variants.

## Node `tex.post_distortion`
Spatial distortion post-processing category node.
Includes chromatic aberration, lens warp, heat/ripple/glitch-style offsets.
### Param `effect`
Selects distortion effect preset.
### Param `amount`
Master blend amount of the selected effect.
### Param `scale`
Distortion radius or UV offset scale.
### Param `thresh`
Threshold gate for distortion masking.
### Param `speed`
Animation rate for time-varying variants.

## Node `tex.post_temporal`
Temporal post-processing category node.
Uses frame history for trails/feedback/datamosh/afterimage-style effects.
### Param `effect`
Selects temporal effect preset.
### Param `amount`
Master blend amount of the selected effect.
### Param `scale`
Temporal offset/radius scaling.
### Param `thresh`
Threshold gate for history contribution.
### Param `speed`
Animation rate for time-varying variants.

## Node `tex.post_noise_texture`
Noise and texture post-processing category node.
Includes film grain, dither, scanline, VHS, pixelate, and mosaic variants.
### Param `effect`
Selects noise/texture effect preset.
### Param `amount`
Master blend amount of the selected effect.
### Param `scale`
Cell size / density / frequency scale.
### Param `thresh`
Threshold gate for selective application.
### Param `speed`
Animation rate for time-varying variants.

## Node `tex.post_lighting`
Lighting simulation post-processing category node.
Includes glow shafts, lens-style artifacts, vignette/leak/halation variants.
### Param `effect`
Selects lighting simulation effect preset.
### Param `amount`
Master blend amount of the selected effect.
### Param `scale`
Kernel radius and streak/stretch scale.
### Param `thresh`
Threshold gate for bright-region extraction.
### Param `speed`
Animation rate for time-varying variants.

## Node `tex.post_screen_space`
Geometric and screen-space post-processing category node.
Provides stylized SSAO/SSR/fog/refraction/curvature-inspired variants.
### Param `effect`
Selects screen-space effect preset.
### Param `amount`
Master blend amount of the selected effect.
### Param `scale`
Neighborhood radius / curvature scale.
### Param `thresh`
Threshold gate for shading transitions.
### Param `speed`
Animation rate for time-varying variants.

## Node `tex.post_experimental`
Experimental and pattern-driven post-processing category node.
Includes reaction-diffusion filter, kaleidoscope, polar warp, and flow variants.
### Param `effect`
Selects experimental effect preset.
### Param `amount`
Master blend amount of the selected effect.
### Param `scale`
Pattern density / warp scale.
### Param `thresh`
Threshold gate for stylization transitions.
### Param `speed`
Animation rate for time-varying variants.

## Node `tex.blend`
Composites primary input with optional secondary texture.
Similar to common DCC compositing workflows.
### Param `blend_tex`
Optional secondary texture input used by blend operations.
### Param `blend_mode`
Blend equation preset (normal/add/subtract/multiply/screen/overlay/darken/lighten/difference).
### Param `opacity`
Blend contribution amount from `blend_tex`.
### Param `bg_r`
Background fill red channel.
### Param `bg_g`
Background fill green channel.
### Param `bg_b`
Background fill blue channel.
### Param `bg_a`
Background fill alpha amount applied behind blend output.

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
### Param `sync_mode`
Timing mode: free-running hertz or beat-synced to timeline BPM.
### Param `beat_mul`
Beat-sync multiplier; cycles per beat when `sync_mode` is `beat`.
### Param `lfo_type`
Wave shape selector: sine, saw, triangle, pulse, or drift.
`drift` is a slow, smooth, softly random undulating curve.
### Param `shape`
Wave-shape morph control.
For `drift`, lower values bias smoother motion; higher values add subtle roughness.

## Node `io.window_out`
Final output sink that presents the incoming texture to the window.
No editable parameters on this node.
