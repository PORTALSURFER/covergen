//! Default node-parameter layouts keyed by node kind.
//!
//! This module keeps the `ProjectNodeKind -> Vec<NodeParamSlot>` mapping in one
//! typed registry and routes each kind to a focused node-family builder.

mod buffer;
mod control;
mod scene;
mod texture;

use super::*;

type ParamBuilder = fn() -> Vec<NodeParamSlot>;

#[derive(Clone, Copy)]
struct KindParamDefaults {
    kind: ProjectNodeKind,
    build: ParamBuilder,
}

impl KindParamDefaults {
    const fn new(kind: ProjectNodeKind, build: ParamBuilder) -> Self {
        Self { kind, build }
    }
}

const PARAM_DEFAULT_REGISTRY: [KindParamDefaults; 30] = [
    KindParamDefaults::new(ProjectNodeKind::TexSolid, texture::tex_solid_params),
    KindParamDefaults::new(ProjectNodeKind::TexCircle, texture::tex_circle_params),
    KindParamDefaults::new(
        ProjectNodeKind::TexSourceNoise,
        texture::tex_source_noise_params,
    ),
    KindParamDefaults::new(ProjectNodeKind::BufSphere, buffer::buf_sphere_params),
    KindParamDefaults::new(
        ProjectNodeKind::BufCircleNurbs,
        buffer::buf_circle_nurbs_params,
    ),
    KindParamDefaults::new(ProjectNodeKind::BufNoise, buffer::buf_noise_params),
    KindParamDefaults::new(
        ProjectNodeKind::TexTransform2D,
        texture::tex_transform_2d_params,
    ),
    KindParamDefaults::new(ProjectNodeKind::TexLevel, texture::tex_level_params),
    KindParamDefaults::new(ProjectNodeKind::TexMask, texture::tex_mask_params),
    KindParamDefaults::new(ProjectNodeKind::TexToneMap, texture::tex_tone_map_params),
    KindParamDefaults::new(ProjectNodeKind::TexFeedback, texture::tex_feedback_params),
    KindParamDefaults::new(
        ProjectNodeKind::TexReactionDiffusion,
        texture::tex_reaction_diffusion_params,
    ),
    KindParamDefaults::new(
        ProjectNodeKind::TexDomainWarp,
        texture::tex_domain_warp_params,
    ),
    KindParamDefaults::new(
        ProjectNodeKind::TexWarpTransform,
        texture::tex_warp_transform_params,
    ),
    KindParamDefaults::new(
        ProjectNodeKind::TexPostColorTone,
        texture::tex_post_color_tone_params,
    ),
    KindParamDefaults::new(
        ProjectNodeKind::TexPostEdgeStructure,
        texture::tex_post_edge_structure_params,
    ),
    KindParamDefaults::new(
        ProjectNodeKind::TexPostBlurDiffusion,
        texture::tex_post_blur_diffusion_params,
    ),
    KindParamDefaults::new(
        ProjectNodeKind::TexPostDistortion,
        texture::tex_post_distortion_params,
    ),
    KindParamDefaults::new(
        ProjectNodeKind::TexPostTemporal,
        texture::tex_post_temporal_params,
    ),
    KindParamDefaults::new(
        ProjectNodeKind::TexPostNoiseTexture,
        texture::tex_post_noise_texture_params,
    ),
    KindParamDefaults::new(
        ProjectNodeKind::TexPostLighting,
        texture::tex_post_lighting_params,
    ),
    KindParamDefaults::new(
        ProjectNodeKind::TexPostScreenSpace,
        texture::tex_post_screen_space_params,
    ),
    KindParamDefaults::new(
        ProjectNodeKind::TexPostExperimental,
        texture::tex_post_experimental_params,
    ),
    KindParamDefaults::new(ProjectNodeKind::TexBlend, texture::tex_blend_params),
    KindParamDefaults::new(ProjectNodeKind::SceneEntity, scene::scene_entity_params),
    KindParamDefaults::new(ProjectNodeKind::SceneBuild, scene::scene_build_params),
    KindParamDefaults::new(ProjectNodeKind::RenderCamera, scene::render_camera_params),
    KindParamDefaults::new(
        ProjectNodeKind::RenderScenePass,
        scene::render_scene_pass_params,
    ),
    KindParamDefaults::new(ProjectNodeKind::CtlLfo, control::ctl_lfo_params),
    KindParamDefaults::new(ProjectNodeKind::IoWindowOut, scene::io_window_out_params),
];

pub(super) fn default_params_for_kind(kind: ProjectNodeKind) -> Vec<NodeParamSlot> {
    PARAM_DEFAULT_REGISTRY
        .iter()
        .find(|entry| entry.kind == kind)
        .map(|entry| (entry.build)())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_param_registry_covers_every_node_kind_once() {
        let mut registry_kinds = Vec::new();
        for entry in PARAM_DEFAULT_REGISTRY {
            assert!(
                !registry_kinds.contains(&entry.kind),
                "duplicate default-params registry entry for {:?}",
                entry.kind
            );
            registry_kinds.push(entry.kind);
        }

        assert_eq!(
            registry_kinds.len(),
            PROJECT_NODE_KIND_DESCRIPTORS.len(),
            "default-params registry should cover all node-kind descriptors"
        );

        for descriptor in PROJECT_NODE_KIND_DESCRIPTORS {
            assert!(
                registry_kinds.contains(&descriptor.kind),
                "missing default-params registry entry for {:?}",
                descriptor.kind
            );
        }
    }
}
