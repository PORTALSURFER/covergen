# Example Graphs

This directory contains ready-to-load GUI project graphs.

## Included

- `circle_noise_feedback_trail.json`
  - Demonstrates a working TD-style trail setup:
    1. `tex.circle` produces the live shape.
    2. `tex.post_noise_texture` modulates the feedback-history branch.
    3. `tex.feedback` outputs delayed history.
    4. `tex.transform_2d` fades history via `alpha_mul`.
    5. `tex.blend` composites the raw live circle over faded trail.
    6. `tex.feedback.accumulation_tex` is bound to the fade branch output for loop storage.

## Load In GUI

1. Open the Main menu.
2. Choose `Load Project`.
3. Pick one of the JSON files in this directory.

Tip: If trails saturate, lower `tex.transform_2d.alpha_mul` or `tex.feedback.feedback`.
