//! Default node-parameter layouts keyed by node kind.
//!
//! This module keeps the `ProjectNodeKind -> Vec<NodeParamSlot>` mapping in one
//! typed registry and routes each kind to a focused node-family builder.

mod buffer;
mod control;
mod scene;
mod texture;

use super::*;

pub(super) fn default_params_for_kind(kind: ProjectNodeKind) -> Vec<NodeParamSlot> {
    match kind {
        ProjectNodeKind::TexSolid => texture::tex_solid_params(),
        ProjectNodeKind::TexCircle => texture::tex_circle_params(),
        ProjectNodeKind::TexSourceNoise => texture::tex_source_noise_params(),
        ProjectNodeKind::BufSphere => buffer::buf_sphere_params(),
        ProjectNodeKind::BufBox => buffer::buf_box_params(),
        ProjectNodeKind::BufGrid => buffer::buf_grid_params(),
        ProjectNodeKind::BufCircleNurbs => buffer::buf_circle_nurbs_params(),
        ProjectNodeKind::BufNoise => buffer::buf_noise_params(),
        ProjectNodeKind::TexTransform2D => texture::tex_transform_2d_params(),
        ProjectNodeKind::TexLevel => texture::tex_level_params(),
        ProjectNodeKind::TexMask => texture::tex_mask_params(),
        ProjectNodeKind::TexMorphology => texture::tex_morphology_params(),
        ProjectNodeKind::TexToneMap => texture::tex_tone_map_params(),
        ProjectNodeKind::TexFeedback => texture::tex_feedback_params(),
        ProjectNodeKind::TexReactionDiffusion => texture::tex_reaction_diffusion_params(),
        ProjectNodeKind::TexDomainWarp => texture::tex_domain_warp_params(),
        ProjectNodeKind::TexDirectionalSmear => texture::tex_directional_smear_params(),
        ProjectNodeKind::TexWarpTransform => texture::tex_warp_transform_params(),
        ProjectNodeKind::TexPostColorTone => texture::tex_post_color_tone_params(),
        ProjectNodeKind::TexPostEdgeStructure => texture::tex_post_edge_structure_params(),
        ProjectNodeKind::TexPostBlurDiffusion => texture::tex_post_blur_diffusion_params(),
        ProjectNodeKind::TexPostDistortion => texture::tex_post_distortion_params(),
        ProjectNodeKind::TexPostTemporal => texture::tex_post_temporal_params(),
        ProjectNodeKind::TexPostNoiseTexture => texture::tex_post_noise_texture_params(),
        ProjectNodeKind::TexPostLighting => texture::tex_post_lighting_params(),
        ProjectNodeKind::TexPostScreenSpace => texture::tex_post_screen_space_params(),
        ProjectNodeKind::TexPostExperimental => texture::tex_post_experimental_params(),
        ProjectNodeKind::TexBlend => texture::tex_blend_params(),
        ProjectNodeKind::SceneEntity => scene::scene_entity_params(),
        ProjectNodeKind::SceneBuild => scene::scene_build_params(),
        ProjectNodeKind::RenderCamera => scene::render_camera_params(),
        ProjectNodeKind::RenderScenePass => scene::render_scene_pass_params(),
        ProjectNodeKind::CtlLfo => control::ctl_lfo_params(),
        ProjectNodeKind::IoWindowOut => scene::io_window_out_params(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_param_registry_covers_every_node_kind_once() {
        assert_eq!(
            ProjectNodeKind::descriptors().len(),
            34,
            "descriptor registry should enumerate every GUI node kind"
        );

        for descriptor in ProjectNodeKind::descriptors() {
            let _ = default_params_for_kind(descriptor.kind);
        }
    }
}
