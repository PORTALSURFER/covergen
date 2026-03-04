use super::super::{param_schema, GuiCompiledRuntime};
use crate::gui::project::{GuiProject, ProjectNodeKind, SignalEvalStack};

#[test]
fn scene_pass_resolution_defaults_to_project_size_when_zero() {
    let mut project = GuiProject::new_empty(640, 480);
    let sphere = project.add_node(ProjectNodeKind::BufSphere, 20, 40, 420, 480);
    let entity = project.add_node(ProjectNodeKind::SceneEntity, 180, 40, 420, 480);
    let scene = project.add_node(ProjectNodeKind::SceneBuild, 340, 40, 420, 480);
    let pass = project.add_node(ProjectNodeKind::RenderScenePass, 500, 40, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 660, 40, 420, 480);
    assert!(project.connect_image_link(sphere, entity));
    assert!(project.connect_image_link(entity, scene));
    assert!(project.connect_image_link(scene, pass));
    assert!(project.connect_image_link(pass, out));
    let runtime = GuiCompiledRuntime::compile(&project).expect("runtime should compile");
    let mut eval_stack = SignalEvalStack::default();
    let (w, h) = runtime.output_texture_size(&project, 0.0, &mut eval_stack);
    assert_eq!((w, h), (640, 480));
    assert!(project.set_param_value(pass, 0, 320.0));
    assert!(project.set_param_value(pass, 1, 200.0));
    let (w2, h2) = runtime.output_texture_size(&project, 0.0, &mut eval_stack);
    assert_eq!((w2, h2), (320, 200));
}

#[test]
fn output_texture_size_does_not_reuse_signal_memo_across_projects() {
    let mut project_a = GuiProject::new_empty(640, 480);
    let lfo_a = project_a.add_node(ProjectNodeKind::CtlLfo, 20, 40, 420, 480);
    let sphere_a = project_a.add_node(ProjectNodeKind::BufSphere, 180, 40, 420, 480);
    let entity_a = project_a.add_node(ProjectNodeKind::SceneEntity, 340, 40, 420, 480);
    let scene_a = project_a.add_node(ProjectNodeKind::SceneBuild, 500, 40, 420, 480);
    let pass_a = project_a.add_node(ProjectNodeKind::RenderScenePass, 660, 40, 420, 480);
    let out_a = project_a.add_node(ProjectNodeKind::IoWindowOut, 820, 40, 420, 480);
    assert!(project_a.connect_image_link(sphere_a, entity_a));
    assert!(project_a.connect_image_link(entity_a, scene_a));
    assert!(project_a.connect_image_link(scene_a, pass_a));
    assert!(project_a.connect_image_link(pass_a, out_a));
    assert!(project_a.connect_signal_link_to_param(
        lfo_a,
        pass_a,
        param_schema::render_scene_pass::RES_WIDTH_INDEX,
    ));
    assert!(project_a.set_param_value(lfo_a, param_schema::ctl_lfo::RATE_HZ_INDEX, 0.0));
    assert!(project_a.set_param_value(lfo_a, param_schema::ctl_lfo::AMPLITUDE_INDEX, 8.0));
    assert!(project_a.set_param_value(lfo_a, param_schema::ctl_lfo::PHASE_INDEX, 0.25));
    assert!(project_a.set_param_value(lfo_a, param_schema::ctl_lfo::BIAS_INDEX, 0.0));

    let mut project_b = GuiProject::new_empty(640, 480);
    let lfo_b = project_b.add_node(ProjectNodeKind::CtlLfo, 20, 40, 420, 480);
    let sphere_b = project_b.add_node(ProjectNodeKind::BufSphere, 180, 40, 420, 480);
    let entity_b = project_b.add_node(ProjectNodeKind::SceneEntity, 340, 40, 420, 480);
    let scene_b = project_b.add_node(ProjectNodeKind::SceneBuild, 500, 40, 420, 480);
    let pass_b = project_b.add_node(ProjectNodeKind::RenderScenePass, 660, 40, 420, 480);
    let out_b = project_b.add_node(ProjectNodeKind::IoWindowOut, 820, 40, 420, 480);
    assert!(project_b.connect_image_link(sphere_b, entity_b));
    assert!(project_b.connect_image_link(entity_b, scene_b));
    assert!(project_b.connect_image_link(scene_b, pass_b));
    assert!(project_b.connect_image_link(pass_b, out_b));
    assert!(project_b.connect_signal_link_to_param(
        lfo_b,
        pass_b,
        param_schema::render_scene_pass::RES_WIDTH_INDEX,
    ));
    assert!(project_b.set_param_value(lfo_b, param_schema::ctl_lfo::RATE_HZ_INDEX, 0.0));
    assert!(project_b.set_param_value(lfo_b, param_schema::ctl_lfo::AMPLITUDE_INDEX, 20.0));
    assert!(project_b.set_param_value(lfo_b, param_schema::ctl_lfo::PHASE_INDEX, 0.25));
    assert!(project_b.set_param_value(lfo_b, param_schema::ctl_lfo::BIAS_INDEX, 0.0));

    let runtime_a = GuiCompiledRuntime::compile(&project_a).expect("runtime A should compile");
    let runtime_b = GuiCompiledRuntime::compile(&project_b).expect("runtime B should compile");
    let mut eval_stack = SignalEvalStack::default();

    let (w_a, h_a) = runtime_a.output_texture_size(&project_a, 0.0, &mut eval_stack);
    let (w_b, h_b) = runtime_b.output_texture_size(&project_b, 0.0, &mut eval_stack);

    assert_eq!(h_a, 480);
    assert_eq!(h_b, 480);
    assert_eq!(w_a, 8);
    assert_eq!(w_b, 20);
}
