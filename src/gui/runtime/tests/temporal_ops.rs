use super::super::{
    GuiCompiledRuntime, PostProcessCategory, TexRuntimeFeedbackHistoryBinding, TexRuntimeOp,
};
use crate::gui::project::{GuiProject, ProjectNodeKind, SignalEvalStack};

#[test]
fn reaction_diffusion_emits_temporal_op_with_defaults() {
    let mut project = GuiProject::new_empty(640, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 420, 480);
    let reaction = project.add_node(ProjectNodeKind::TexReactionDiffusion, 180, 40, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 340, 40, 420, 480);
    assert!(project.connect_image_link(solid, reaction));
    assert!(project.connect_image_link(reaction, out));

    let runtime = GuiCompiledRuntime::compile(&project).expect("runtime should compile");
    let mut eval_stack = SignalEvalStack::default();
    let mut ops = Vec::new();
    runtime.evaluate_ops(&project, 0.0, &mut eval_stack, &mut ops);
    assert_eq!(ops.len(), 2);
    assert!(matches!(
        ops[1],
        TexRuntimeOp::ReactionDiffusion {
            diffusion_a,
            diffusion_b,
            feed,
            kill,
            dt,
            seed_mix,
            history: TexRuntimeFeedbackHistoryBinding::Internal { feedback_node_id },
        } if diffusion_a == 1.0
            && diffusion_b == 0.5
            && (feed - 0.055).abs() < 1e-6
            && (kill - 0.062).abs() < 1e-6
            && dt == 1.0
            && (seed_mix - 0.04).abs() < 1e-6
            && feedback_node_id == reaction
    ));
}

#[test]
fn post_temporal_emits_post_process_op_with_history() {
    let mut project = GuiProject::new_empty(640, 480);
    let solid = project.add_node(ProjectNodeKind::TexSolid, 20, 40, 420, 480);
    let post = project.add_node(ProjectNodeKind::TexPostTemporal, 180, 40, 420, 480);
    let out = project.add_node(ProjectNodeKind::IoWindowOut, 340, 40, 420, 480);
    assert!(project.connect_image_link(solid, post));
    assert!(project.connect_image_link(post, out));

    let runtime = GuiCompiledRuntime::compile(&project).expect("runtime should compile");
    let mut eval_stack = SignalEvalStack::default();
    let mut ops = Vec::new();
    runtime.evaluate_ops(&project, 1.25, &mut eval_stack, &mut ops);
    assert_eq!(ops.len(), 2);
    assert!(matches!(
        ops[1],
        TexRuntimeOp::PostProcess {
            category: PostProcessCategory::Temporal,
            amount,
            scale,
            threshold,
            speed,
            history: Some(TexRuntimeFeedbackHistoryBinding::Internal { feedback_node_id }),
            ..
        } if (amount - 0.5).abs() < 1e-6
            && (scale - 1.0).abs() < 1e-6
            && (threshold - 0.5).abs() < 1e-6
            && (speed - 1.0).abs() < 1e-6
            && feedback_node_id == post
    ));
}
