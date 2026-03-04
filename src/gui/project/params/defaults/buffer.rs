use super::*;

pub(super) fn buf_sphere_params() -> Vec<NodeParamSlot> {
    vec![
        param("radius", "radius", 0.28, 0.02, 0.5, 0.005),
        param("segments", "segments", 32.0, 3.0, 128.0, 1.0),
        param("rings", "rings", 16.0, 2.0, 64.0, 1.0),
    ]
}

pub(super) fn buf_circle_nurbs_params() -> Vec<NodeParamSlot> {
    vec![
        param("radius", "radius", 0.28, 0.02, 0.95, 0.005),
        param("arc_start", "arc_start", 0.0, 0.0, 360.0, 1.0),
        param("arc_end", "arc_end", 360.0, 0.0, 360.0, 1.0),
        param_dropdown("arc_style", "arc_style", 0, &BUF_CIRCLE_ARC_STYLE_OPTIONS),
        param("line_width", "line_width", 0.01, 0.0005, 0.35, 0.001),
        param("order", "order", 3.0, 2.0, 5.0, 1.0),
        param("divisions", "divisions", 64.0, 8.0, 512.0, 1.0),
    ]
}

pub(super) fn buf_noise_params() -> Vec<NodeParamSlot> {
    vec![
        // Keep deformation disabled by default so inserting this node is
        // identity until users increase amplitude.
        param("amplitude", "amplitude", 0.0, 0.0, 1.0, 0.01),
        param("frequency", "frequency", 2.0, 0.05, 32.0, 0.05),
        param("speed_hz", "speed_hz", 0.35, 0.0, 16.0, 0.05),
        param("phase", "phase", 0.0, -8.0, 8.0, 0.05),
        param("seed", "seed", 1.0, 0.0, 1024.0, 1.0),
        param("twist", "twist", 0.0, -8.0, 8.0, 0.05),
        param("stretch", "stretch", 0.0, 0.0, 1.0, 0.01),
        // Loop mode quantizes time to timeline phase for clean first/last
        // frame matching and deterministic clip playback.
        param("loop_cyc", "loop_cyc", 12.0, 0.0, 256.0, 1.0),
        param_dropdown("loop_mode", "loop_mode", 0, &BUF_NOISE_LOOP_MODE_OPTIONS),
    ]
}
