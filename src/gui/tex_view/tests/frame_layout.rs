use super::*;
#[test]
fn disconnected_graph_returns_empty_gpu_payload() {
    let project = GuiProject::new_empty(640, 480);
    let mut viewer = TexViewerGenerator::default();
    viewer.update(
        &project,
        TexViewerUpdate {
            viewport_width: 960,
            viewport_height: 540,
            panel_width: 420,
            frame_index: 0,
            timeline_total_frames: 1_800,
            timeline_fps: 60,
            tex_eval_epoch: project.invalidation().tex_eval,
        },
    );
    let frame = viewer.frame().expect("viewer frame should exist");
    let ops = match frame.payload {
        TexViewerPayload::GpuOps(ops) => ops,
    };
    assert!(ops.is_empty());
}

#[test]
fn viewer_frame_fits_texture_aspect_inside_output_panel() {
    let project = GuiProject::new_empty(1920, 1080);
    let mut viewer = TexViewerGenerator::default();
    viewer.update(
        &project,
        TexViewerUpdate {
            viewport_width: 1200,
            viewport_height: 900,
            panel_width: 420,
            frame_index: 0,
            timeline_total_frames: 1_800,
            timeline_fps: 60,
            tex_eval_epoch: project.invalidation().tex_eval,
        },
    );
    let frame = viewer.frame().expect("viewer frame should exist");
    assert_eq!(frame.texture_width, 1920);
    assert_eq!(frame.texture_height, 1080);
    assert_eq!(frame.width, 780);
    assert_eq!(frame.height, 438);
    assert_eq!(frame.x, 420);
    let expected_y = ((editor_panel_height(900) as u32 - frame.height) / 2) as i32;
    assert_eq!(frame.y, expected_y);
}
