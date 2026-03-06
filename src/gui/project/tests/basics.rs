use super::*;
#[test]
fn empty_project_has_no_nodes() {
    let project = GuiProject::new_empty(640, 480);
    assert_eq!(project.node_count(), 0);
}

#[test]
fn add_node_assigns_incrementing_ids() {
    let mut project = GuiProject::new_empty(640, 480);
    let a = project.add_node(ProjectNodeKind::TexSolid, 80, 80, 420, 480);
    let b = project.add_node(ProjectNodeKind::IoWindowOut, 120, 120, 420, 480);
    assert_eq!(a, 1);
    assert_eq!(b, 2);
}

#[test]
fn node_hit_test_uses_topmost_order() {
    let mut project = GuiProject::new_empty(640, 480);
    let a = project.add_node(ProjectNodeKind::TexSolid, 80, 80, 420, 480);
    let b = project.add_node(ProjectNodeKind::IoWindowOut, 80, 80, 420, 480);
    assert_eq!(project.node_at(90, 90), Some(b));
    assert_ne!(project.node_at(90, 90), Some(a));
}

#[test]
fn node_hit_test_updates_after_move_without_full_scan_state_drift() {
    let mut project = GuiProject::new_empty(640, 480);
    let node = project.add_node(ProjectNodeKind::TexSolid, 80, 80, 420, 480);
    assert_eq!(project.node_at(90, 90), Some(node));
    assert!(project.move_node(node, 260, 220, 420, 480));
    assert_eq!(project.node_at(90, 90), None);
    assert_eq!(project.node_at(270, 230), Some(node));
}

#[test]
fn expanded_node_hit_bounds_update_after_toggle() {
    let mut project = GuiProject::new_empty(640, 480);
    let node = project.add_node(ProjectNodeKind::TexSolid, 60, 60, 420, 480);
    let base_miss_y = 60 + NODE_HEIGHT + 4;
    assert_eq!(project.node_at(72, base_miss_y), None);
    assert!(project.toggle_node_expanded(node, 420, 480));
    assert_eq!(project.node_at(72, base_miss_y), Some(node));
}

#[test]
fn pin_hit_tests_work_through_spatial_bins() {
    let mut project = GuiProject::new_empty(640, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 80, 80, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 240, 80, 420, 480);
    let solid_node = project.node(solid).expect("solid node");
    let out_node = project.node(out).expect("output node");
    let (ox, oy) = output_pin_center(solid_node).expect("solid output");
    let (ix, iy) = input_pin_center(out_node).expect("output input");
    assert_eq!(project.output_pin_at(ox, oy, 10), Some(solid));
    assert_eq!(project.input_pin_at(ix, iy, 10, None), Some(out));
    assert_eq!(project.input_pin_at(ix, iy, 10, Some(out)), None);
}

#[test]
fn node_rect_query_returns_overlapping_nodes_only() {
    let mut project = GuiProject::new_empty(640, 480);
    let a = project.add_node(ProjectNodeKind::TexSolid, 40, 40, 420, 480);
    let b = project.add_node(ProjectNodeKind::TexCircle, 280, 180, 420, 480);
    let c = project.add_node(ProjectNodeKind::IoWindowOut, 360, 40, 420, 480);
    let overlaps = project.node_ids_overlapping_graph_rect(20, 20, 250, 170);
    assert_eq!(overlaps, vec![a]);
    let overlaps_multi = project.node_ids_overlapping_graph_rect(260, 20, 620, 260);
    assert_eq!(overlaps_multi, vec![b, c]);
}

#[test]
fn connect_image_link_wires_solid_to_window_out() {
    let mut project = GuiProject::new_empty(640, 480);
    let tex_source = project.add_node(ProjectNodeKind::TexSolid, 80, 80, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 220, 80, 420, 480);
    assert!(project.connect_image_link(tex_source, out));
    assert_eq!(project.edge_count(), 1);
    let source_id = project
        .window_out_input_node_id()
        .expect("window-out input must exist");
    let source = project.node(source_id).expect("source node must exist");
    assert_eq!(source.kind(), ProjectNodeKind::TexSolid);
    assert!(!project.connect_image_link(tex_source, out));
}

#[test]
fn transform_node_supports_in_and_out_links() {
    let mut project = GuiProject::new_empty(640, 480);
    let source = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 420, 480);
    let xform = project.add_node(ProjectNodeKind::TexTransform2D, 160, 40, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 300, 40, 420, 480);
    assert!(project.connect_image_link(source, xform));
    assert!(project.connect_image_link(xform, out));
    assert_eq!(project.edge_count(), 2);
    let source_id = project
        .window_out_input_node_id()
        .expect("window-out input must exist");
    let source = project.node(source_id).expect("source node must exist");
    assert_eq!(source.kind(), ProjectNodeKind::TexTransform2D);
}

#[test]
fn feedback_node_supports_in_and_out_links() {
    let mut project = GuiProject::new_empty(640, 480);
    let source = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 420, 480);
    let feedback = project.add_node(ProjectNodeKind::TexFeedback, 160, 40, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 300, 40, 420, 480);
    assert!(project.connect_image_link(source, feedback));
    assert!(project.connect_image_link(feedback, out));
    assert_eq!(project.edge_count(), 2);
    let source_id = project
        .window_out_input_node_id()
        .expect("window-out input must exist");
    let source = project.node(source_id).expect("source node must exist");
    assert_eq!(source.kind(), ProjectNodeKind::TexFeedback);
}

#[test]
fn source_noise_node_supports_out_links() {
    let mut project = GuiProject::new_empty(640, 480);
    let noise = project.add_node(ProjectNodeKind::TexSourceNoise, 20, 40, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 160, 40, 420, 480);
    assert!(project.connect_image_link(noise, out));
    assert_eq!(project.edge_count(), 1);
    let source_id = project
        .window_out_input_node_id()
        .expect("window-out input must exist");
    let source = project.node(source_id).expect("source node must exist");
    assert_eq!(source.kind(), ProjectNodeKind::TexSourceNoise);
}

#[test]
fn reaction_diffusion_node_supports_in_and_out_links() {
    let mut project = GuiProject::new_empty(640, 480);
    let source = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 420, 480);
    let reaction = project.add_node(ProjectNodeKind::TexReactionDiffusion, 160, 40, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 300, 40, 420, 480);
    assert!(project.connect_image_link(source, reaction));
    assert!(project.connect_image_link(reaction, out));
    assert_eq!(project.edge_count(), 2);
    let source_id = project
        .window_out_input_node_id()
        .expect("window-out input must exist");
    let source = project.node(source_id).expect("source node must exist");
    assert_eq!(source.kind(), ProjectNodeKind::TexReactionDiffusion);
}

#[test]
fn mask_morphology_tone_map_domain_warp_smear_and_warp_nodes_support_in_and_out_links() {
    let mut project = GuiProject::new_empty(640, 480);
    let source = project.add_node(ProjectNodeKind::TexSourceNoise, 20, 40, 420, 480);
    let mask = project.add_node(ProjectNodeKind::TexMask, 180, 40, 420, 480);
    let morphology = project.add_node(ProjectNodeKind::TexMorphology, 340, 40, 420, 480);
    let tone = project.add_node(ProjectNodeKind::TexToneMap, 500, 40, 420, 480);
    let domain_warp = project.add_node(ProjectNodeKind::TexDomainWarp, 660, 40, 420, 480);
    let smear = project.add_node(ProjectNodeKind::TexDirectionalSmear, 820, 40, 420, 480);
    let warp = project.add_node(ProjectNodeKind::TexWarpTransform, 980, 40, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 1140, 40, 420, 480);
    assert!(project.connect_image_link(source, mask));
    assert!(project.connect_image_link(mask, morphology));
    assert!(project.connect_image_link(morphology, tone));
    assert!(project.connect_image_link(tone, domain_warp));
    assert!(project.connect_image_link(domain_warp, smear));
    assert!(project.connect_image_link(smear, warp));
    assert!(project.connect_image_link(warp, out));
    let source_id = project
        .window_out_input_node_id()
        .expect("window-out input must exist");
    let source = project.node(source_id).expect("source node must exist");
    assert_eq!(source.kind(), ProjectNodeKind::TexWarpTransform);
}

#[test]
fn post_process_node_supports_in_and_out_links() {
    let mut project = GuiProject::new_empty(640, 480);
    let source = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 420, 480);
    let post = project.add_node(ProjectNodeKind::TexPostColorTone, 160, 40, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 300, 40, 420, 480);
    assert!(project.connect_image_link(source, post));
    assert!(project.connect_image_link(post, out));
    assert_eq!(project.edge_count(), 2);
    let source_id = project
        .window_out_input_node_id()
        .expect("window-out input must exist");
    let source = project.node(source_id).expect("source node must exist");
    assert_eq!(source.kind(), ProjectNodeKind::TexPostColorTone);
}
