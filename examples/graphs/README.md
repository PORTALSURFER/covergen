# Example Graphs

This directory contains ready-to-load GUI project graphs.

## Included

- `circle_noise_feedback_trail.json`
  - Minimal trail demo (no post-process nodes):
    1. `tex.circle` produces the moving live shape.
    2. `tex.feedback` reads prior history (one-frame delayed).
    3. `tex.blend` composites live circle over prior history.
    4. `tex.color_adjust` fades the composite via `alpha_mul` (opacity/energy).
    5. `tex.feedback.accumulation_tex` is bound to `tex.color_adjust` output to store the faded result for the next frame.
- `marbled_ink_monochrome.json`
  - Organic monochrome marbling demo:
    1. Three `tex.source_noise` nodes use `simplex`, `ridged`, and `cellular` modes for coarse mass, warp flow, and void-pocket seeds.
    2. `tex.mask` plus `tex.morphology` turn the cellular field into cleaner cavity membranes.
    3. `tex.blend` and `tex.reaction_diffusion` grow the base tendril structure.
    4. `tex.domain_warp`, `tex.feedback`, `tex.post_experimental(flow_adv)`, and `tex.directional_smear` provide coherent transport and stretched ink drag.
    5. `tex.warp_transform`, `tex.post_blur_diffusion`, `tex.post_edge_structure`, `tex.post_color_tone(mono)`, and `tex.tone_map` crush the result into dark monochrome marbling.
    6. Slow `ctl.lfo` modulation animates warp phase and diffusion feed for subtle living motion.

## Load In GUI

1. Open the Main menu.
2. Choose `Load Project`.
3. Pick one of the JSON files in this directory.

Tip: If the trail is too short, increase `tex.color_adjust.alpha_mul` toward `1.0`.
