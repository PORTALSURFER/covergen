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

## Load In GUI

1. Open the Main menu.
2. Choose `Load Project`.
3. Pick one of the JSON files in this directory.

Tip: If the trail is too short, increase `tex.transform_2d.alpha_mul` toward `1.0`.
