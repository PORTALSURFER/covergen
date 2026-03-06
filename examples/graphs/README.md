# Example Graphs

This directory contains ready-to-load GUI project graphs.

## Included

- `circle_noise_feedback_trail.json`
  - Minimal trail demo (no post-process nodes):
    1. `tex.circle` produces the moving live shape.
    2. `tex.feedback` reads prior history (one-frame delayed).
    3. `tex.blend` composites live circle over prior history.
    4. `tex.transform_2d` fades the composite via `alpha_mul` (opacity/energy).
    5. `tex.feedback.accumulation_tex` is bound to `tex.transform_2d` output to store the faded result for the next frame.
- `marbled_ink_monochrome.json`
  - Organic monochrome marbling demo:
    1. `tex.source_noise` creates coarse and fine seed fields.
    2. `tex.mask` extracts sparse detail pockets from the fine noise.
    3. `tex.blend` combines those fields into a reaction-diffusion seed.
    4. `tex.reaction_diffusion`, `tex.warp_transform`, and `tex.feedback` build evolving tendrils.
    5. `tex.post_experimental`, `tex.post_blur_diffusion`, `tex.post_edge_structure`, and `tex.tone_map` shape the final ink texture.
    6. Slow `ctl.lfo` modulation animates warp phase and diffusion feed for subtle living motion.

## Load In GUI

1. Open the Main menu.
2. Choose `Load Project`.
3. Pick one of the JSON files in this directory.

Tip: If the trail is too short, increase `tex.transform_2d.alpha_mul` toward `1.0`.
