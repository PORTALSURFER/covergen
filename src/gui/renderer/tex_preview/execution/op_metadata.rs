//! Runtime-op metadata and uniform mapping helpers.

use super::*;

pub(super) fn runtime_op_descriptor(runtime_op: TexViewerOp) -> Option<RuntimeOpDescriptor> {
    let descriptor = match runtime_op {
        TexViewerOp::Solid { .. } => RuntimeOpDescriptor {
            pipeline: RuntimeOpPipelineKind::Solid,
            source_binding: RuntimeSourceBinding::Dummy,
            feedback_binding: RuntimeFeedbackBinding::Dummy,
        },
        TexViewerOp::Circle { .. } => RuntimeOpDescriptor {
            pipeline: RuntimeOpPipelineKind::Circle,
            source_binding: RuntimeSourceBinding::Dummy,
            feedback_binding: RuntimeFeedbackBinding::Dummy,
        },
        TexViewerOp::Box { .. } => RuntimeOpDescriptor {
            pipeline: RuntimeOpPipelineKind::Box,
            source_binding: RuntimeSourceBinding::Dummy,
            feedback_binding: RuntimeFeedbackBinding::Dummy,
        },
        TexViewerOp::Grid { .. } => RuntimeOpDescriptor {
            pipeline: RuntimeOpPipelineKind::Grid,
            source_binding: RuntimeSourceBinding::Dummy,
            feedback_binding: RuntimeFeedbackBinding::Dummy,
        },
        TexViewerOp::Sphere { .. } => RuntimeOpDescriptor {
            pipeline: RuntimeOpPipelineKind::Sphere,
            source_binding: RuntimeSourceBinding::Dummy,
            feedback_binding: RuntimeFeedbackBinding::Dummy,
        },
        TexViewerOp::SourceNoise { .. } => RuntimeOpDescriptor {
            pipeline: RuntimeOpPipelineKind::SourceNoise,
            source_binding: RuntimeSourceBinding::Dummy,
            feedback_binding: RuntimeFeedbackBinding::Dummy,
        },
        TexViewerOp::Transform { .. } => RuntimeOpDescriptor {
            pipeline: RuntimeOpPipelineKind::Transform,
            source_binding: RuntimeSourceBinding::SourceTarget,
            feedback_binding: RuntimeFeedbackBinding::Dummy,
        },
        TexViewerOp::Level { .. } => RuntimeOpDescriptor {
            pipeline: RuntimeOpPipelineKind::Level,
            source_binding: RuntimeSourceBinding::SourceTarget,
            feedback_binding: RuntimeFeedbackBinding::Dummy,
        },
        TexViewerOp::Mask { .. } => RuntimeOpDescriptor {
            pipeline: RuntimeOpPipelineKind::Mask,
            source_binding: RuntimeSourceBinding::SourceTarget,
            feedback_binding: RuntimeFeedbackBinding::Dummy,
        },
        TexViewerOp::Morphology { .. } => RuntimeOpDescriptor {
            pipeline: RuntimeOpPipelineKind::Morphology,
            source_binding: RuntimeSourceBinding::SourceTarget,
            feedback_binding: RuntimeFeedbackBinding::Dummy,
        },
        TexViewerOp::ToneMap { .. } => RuntimeOpDescriptor {
            pipeline: RuntimeOpPipelineKind::ToneMap,
            source_binding: RuntimeSourceBinding::SourceTarget,
            feedback_binding: RuntimeFeedbackBinding::Dummy,
        },
        TexViewerOp::Feedback { .. } => RuntimeOpDescriptor {
            pipeline: RuntimeOpPipelineKind::Feedback,
            source_binding: RuntimeSourceBinding::SourceTarget,
            feedback_binding: RuntimeFeedbackBinding::HistoryRequired,
        },
        TexViewerOp::ReactionDiffusion { .. } => RuntimeOpDescriptor {
            pipeline: RuntimeOpPipelineKind::ReactionDiffusion,
            source_binding: RuntimeSourceBinding::SourceTarget,
            feedback_binding: RuntimeFeedbackBinding::HistoryRequired,
        },
        TexViewerOp::DomainWarp { .. } => {
            return None;
        }
        TexViewerOp::DirectionalSmear { .. } => RuntimeOpDescriptor {
            pipeline: RuntimeOpPipelineKind::DirectionalSmear,
            source_binding: RuntimeSourceBinding::SourceTarget,
            feedback_binding: RuntimeFeedbackBinding::Dummy,
        },
        TexViewerOp::WarpTransform { .. } => RuntimeOpDescriptor {
            pipeline: RuntimeOpPipelineKind::WarpTransform,
            source_binding: RuntimeSourceBinding::SourceTarget,
            feedback_binding: RuntimeFeedbackBinding::Dummy,
        },
        TexViewerOp::PostProcess { history, .. } => RuntimeOpDescriptor {
            pipeline: RuntimeOpPipelineKind::PostProcess,
            source_binding: RuntimeSourceBinding::SourceTarget,
            feedback_binding: if history.is_some() {
                RuntimeFeedbackBinding::HistoryRequired
            } else {
                RuntimeFeedbackBinding::Dummy
            },
        },
        TexViewerOp::Blend { .. } | TexViewerOp::StoreTexture { .. } => {
            return None;
        }
    };
    Some(descriptor)
}

pub(super) fn op_uniform_for_runtime_op(runtime_op: TexViewerOp) -> TexOpUniform {
    match runtime_op {
        TexViewerOp::Solid { .. } | TexViewerOp::StoreTexture { .. } => {
            TexOpUniform::solid(runtime_op)
        }
        TexViewerOp::Circle { .. } => TexOpUniform::circle(runtime_op),
        TexViewerOp::Box { .. } => TexOpUniform::box_shape(runtime_op),
        TexViewerOp::Grid { .. } => TexOpUniform::grid(runtime_op),
        TexViewerOp::Sphere { .. } => TexOpUniform::sphere(runtime_op),
        TexViewerOp::SourceNoise { .. } => TexOpUniform::source_noise(runtime_op),
        TexViewerOp::Transform { .. } => TexOpUniform::transform(runtime_op),
        TexViewerOp::Level { .. } => TexOpUniform::level(runtime_op),
        TexViewerOp::Mask { .. } => TexOpUniform::mask(runtime_op),
        TexViewerOp::Morphology { .. } => TexOpUniform::morphology(runtime_op),
        TexViewerOp::ToneMap { .. } => TexOpUniform::tone_map(runtime_op),
        TexViewerOp::Feedback { .. } => TexOpUniform::feedback(runtime_op),
        TexViewerOp::ReactionDiffusion { .. } => TexOpUniform::reaction_diffusion(runtime_op),
        TexViewerOp::DomainWarp { .. } => TexOpUniform::domain_warp(runtime_op),
        TexViewerOp::DirectionalSmear { .. } => TexOpUniform::directional_smear(runtime_op),
        TexViewerOp::WarpTransform { .. } => TexOpUniform::warp_transform(runtime_op),
        TexViewerOp::PostProcess { .. } => TexOpUniform::post_process(runtime_op),
        TexViewerOp::Blend { .. } => TexOpUniform::blend(runtime_op),
    }
}

pub(super) fn feedback_key_for_runtime_op(runtime_op: TexViewerOp) -> Option<FeedbackHistoryKey> {
    match runtime_op {
        TexViewerOp::Feedback { history, .. } => Some(FeedbackHistoryKey::from_binding(history)),
        TexViewerOp::ReactionDiffusion { history, .. } => {
            Some(FeedbackHistoryKey::from_binding(history))
        }
        TexViewerOp::PostProcess {
            history: Some(history),
            ..
        } => Some(FeedbackHistoryKey::from_binding(history)),
        _ => None,
    }
}

pub(super) fn is_feedback_history_tap_runtime_op(runtime_op: TexViewerOp) -> bool {
    matches!(runtime_op, TexViewerOp::Feedback { .. })
}

pub(super) fn external_feedback_accumulation_texture_for_runtime_op(
    runtime_op: TexViewerOp,
) -> Option<u32> {
    let TexViewerOp::Feedback { history, .. } = runtime_op else {
        return None;
    };
    let crate::gui::runtime::TexRuntimeFeedbackHistoryBinding::External { texture_node_id } =
        history
    else {
        return None;
    };
    Some(texture_node_id)
}

pub(super) fn feedback_frame_gap_for_runtime_op(runtime_op: TexViewerOp) -> u32 {
    let TexViewerOp::Feedback { frame_gap, .. } = runtime_op else {
        return 0;
    };
    frame_gap
}

pub(super) fn op_clear_color(op: TexViewerOp) -> wgpu::Color {
    match op {
        TexViewerOp::Sphere {
            alpha_clip: true, ..
        }
        | TexViewerOp::Box {
            alpha_clip: true, ..
        }
        | TexViewerOp::Grid {
            alpha_clip: true, ..
        }
        | TexViewerOp::Circle {
            alpha_clip: true, ..
        } => TRANSPARENT_BG,
        _ => PREVIEW_BG,
    }
}
