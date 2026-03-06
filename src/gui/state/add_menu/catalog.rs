//! Static add-node catalog declarations and category metadata.

use crate::gui::project::{NodeMenuCategory, ProjectNodeKind, NODE_MENU_CATEGORIES};

/// Add-node category alias sourced from the project node-kind registry.
pub(crate) type AddNodeCategory = NodeMenuCategory;

/// One add-node menu option.
#[derive(Clone, Copy, Debug)]
pub(crate) struct AddNodeOption {
    pub(crate) kind: ProjectNodeKind,
}

impl AddNodeOption {
    /// Return menu label for this option.
    pub(crate) fn label(self) -> &'static str {
        self.kind.label()
    }

    /// Return add-node menu category for this option.
    pub(crate) fn category(self) -> AddNodeCategory {
        self.kind.menu_category()
    }
}

/// Menu entries currently exposed in the graph editor.
pub(crate) const ADD_NODE_OPTIONS: [AddNodeOption; 34] = [
    AddNodeOption {
        kind: ProjectNodeKind::TexSolid,
    },
    AddNodeOption {
        kind: ProjectNodeKind::TexCircle,
    },
    AddNodeOption {
        kind: ProjectNodeKind::TexSourceNoise,
    },
    AddNodeOption {
        kind: ProjectNodeKind::BufSphere,
    },
    AddNodeOption {
        kind: ProjectNodeKind::BufBox,
    },
    AddNodeOption {
        kind: ProjectNodeKind::BufGrid,
    },
    AddNodeOption {
        kind: ProjectNodeKind::BufCircleNurbs,
    },
    AddNodeOption {
        kind: ProjectNodeKind::BufNoise,
    },
    AddNodeOption {
        kind: ProjectNodeKind::SceneEntity,
    },
    AddNodeOption {
        kind: ProjectNodeKind::SceneBuild,
    },
    AddNodeOption {
        kind: ProjectNodeKind::RenderCamera,
    },
    AddNodeOption {
        kind: ProjectNodeKind::RenderScenePass,
    },
    AddNodeOption {
        kind: ProjectNodeKind::TexTransform2D,
    },
    AddNodeOption {
        kind: ProjectNodeKind::TexLevel,
    },
    AddNodeOption {
        kind: ProjectNodeKind::TexMask,
    },
    AddNodeOption {
        kind: ProjectNodeKind::TexMorphology,
    },
    AddNodeOption {
        kind: ProjectNodeKind::TexToneMap,
    },
    AddNodeOption {
        kind: ProjectNodeKind::TexFeedback,
    },
    AddNodeOption {
        kind: ProjectNodeKind::TexReactionDiffusion,
    },
    AddNodeOption {
        kind: ProjectNodeKind::TexDomainWarp,
    },
    AddNodeOption {
        kind: ProjectNodeKind::TexDirectionalSmear,
    },
    AddNodeOption {
        kind: ProjectNodeKind::TexWarpTransform,
    },
    AddNodeOption {
        kind: ProjectNodeKind::TexPostColorTone,
    },
    AddNodeOption {
        kind: ProjectNodeKind::TexPostEdgeStructure,
    },
    AddNodeOption {
        kind: ProjectNodeKind::TexPostBlurDiffusion,
    },
    AddNodeOption {
        kind: ProjectNodeKind::TexPostDistortion,
    },
    AddNodeOption {
        kind: ProjectNodeKind::TexPostTemporal,
    },
    AddNodeOption {
        kind: ProjectNodeKind::TexPostNoiseTexture,
    },
    AddNodeOption {
        kind: ProjectNodeKind::TexPostLighting,
    },
    AddNodeOption {
        kind: ProjectNodeKind::TexPostScreenSpace,
    },
    AddNodeOption {
        kind: ProjectNodeKind::TexPostExperimental,
    },
    AddNodeOption {
        kind: ProjectNodeKind::TexBlend,
    },
    AddNodeOption {
        kind: ProjectNodeKind::CtlLfo,
    },
    AddNodeOption {
        kind: ProjectNodeKind::IoWindowOut,
    },
];

pub(super) const ADD_NODE_CATEGORIES: [AddNodeCategory; 6] = NODE_MENU_CATEGORIES;

pub(super) fn category_count() -> usize {
    ADD_NODE_CATEGORIES.len()
}

#[cfg(test)]
mod tests {
    use super::{AddNodeCategory, ADD_NODE_CATEGORIES, ADD_NODE_OPTIONS};
    use crate::gui::project::{ProjectNodeKind, NODE_MENU_CATEGORIES};

    #[test]
    fn add_node_options_follow_project_registry_category_metadata() {
        assert_eq!(
            ADD_NODE_OPTIONS.len(),
            ProjectNodeKind::descriptors().len(),
            "add-node options should track the full project node-kind registry"
        );

        for option in ADD_NODE_OPTIONS {
            assert_eq!(
                option.category(),
                option.kind.menu_category(),
                "add-node option category should come from the project registry"
            );
        }
    }

    #[test]
    fn add_node_categories_match_project_registry_order() {
        assert_eq!(ADD_NODE_CATEGORIES, NODE_MENU_CATEGORIES);
        assert_eq!(
            ADD_NODE_CATEGORIES,
            [
                AddNodeCategory::Texture,
                AddNodeCategory::Buffer,
                AddNodeCategory::Scene,
                AddNodeCategory::Render,
                AddNodeCategory::Control,
                AddNodeCategory::Io,
            ]
        );
    }
}
