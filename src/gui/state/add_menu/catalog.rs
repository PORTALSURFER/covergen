//! Static add-node catalog declarations and category metadata.

use crate::gui::project::ProjectNodeKind;

/// Category for one add-node menu option.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AddNodeCategory {
    Texture,
    Buffer,
    Scene,
    Render,
    Control,
    Io,
}

impl AddNodeCategory {
    /// Return display label used in category rows.
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Texture => "Texture",
            Self::Buffer => "Buffer",
            Self::Scene => "Scene",
            Self::Render => "Render",
            Self::Control => "Control",
            Self::Io => "IO",
        }
    }

    /// Return a lowercase category label used for query filtering.
    pub(super) const fn normalized_label(self) -> &'static str {
        match self {
            Self::Texture => "texture",
            Self::Buffer => "buffer",
            Self::Scene => "scene",
            Self::Render => "render",
            Self::Control => "control",
            Self::Io => "io",
        }
    }
}

/// One add-node menu option.
#[derive(Clone, Copy, Debug)]
pub(crate) struct AddNodeOption {
    pub(crate) kind: ProjectNodeKind,
    pub(crate) category: AddNodeCategory,
}

impl AddNodeOption {
    /// Return menu label for this option.
    pub(crate) fn label(self) -> &'static str {
        self.kind.label()
    }
}

/// Menu entries currently exposed in the graph editor.
pub(crate) const ADD_NODE_OPTIONS: [AddNodeOption; 29] = [
    AddNodeOption {
        kind: ProjectNodeKind::TexSolid,
        category: AddNodeCategory::Texture,
    },
    AddNodeOption {
        kind: ProjectNodeKind::TexCircle,
        category: AddNodeCategory::Texture,
    },
    AddNodeOption {
        kind: ProjectNodeKind::TexSourceNoise,
        category: AddNodeCategory::Texture,
    },
    AddNodeOption {
        kind: ProjectNodeKind::BufSphere,
        category: AddNodeCategory::Buffer,
    },
    AddNodeOption {
        kind: ProjectNodeKind::BufCircleNurbs,
        category: AddNodeCategory::Buffer,
    },
    AddNodeOption {
        kind: ProjectNodeKind::BufNoise,
        category: AddNodeCategory::Buffer,
    },
    AddNodeOption {
        kind: ProjectNodeKind::SceneEntity,
        category: AddNodeCategory::Scene,
    },
    AddNodeOption {
        kind: ProjectNodeKind::SceneBuild,
        category: AddNodeCategory::Scene,
    },
    AddNodeOption {
        kind: ProjectNodeKind::RenderCamera,
        category: AddNodeCategory::Render,
    },
    AddNodeOption {
        kind: ProjectNodeKind::RenderScenePass,
        category: AddNodeCategory::Render,
    },
    AddNodeOption {
        kind: ProjectNodeKind::TexTransform2D,
        category: AddNodeCategory::Texture,
    },
    AddNodeOption {
        kind: ProjectNodeKind::TexLevel,
        category: AddNodeCategory::Texture,
    },
    AddNodeOption {
        kind: ProjectNodeKind::TexMask,
        category: AddNodeCategory::Texture,
    },
    AddNodeOption {
        kind: ProjectNodeKind::TexToneMap,
        category: AddNodeCategory::Texture,
    },
    AddNodeOption {
        kind: ProjectNodeKind::TexFeedback,
        category: AddNodeCategory::Texture,
    },
    AddNodeOption {
        kind: ProjectNodeKind::TexReactionDiffusion,
        category: AddNodeCategory::Texture,
    },
    AddNodeOption {
        kind: ProjectNodeKind::TexWarpTransform,
        category: AddNodeCategory::Texture,
    },
    AddNodeOption {
        kind: ProjectNodeKind::TexPostColorTone,
        category: AddNodeCategory::Texture,
    },
    AddNodeOption {
        kind: ProjectNodeKind::TexPostEdgeStructure,
        category: AddNodeCategory::Texture,
    },
    AddNodeOption {
        kind: ProjectNodeKind::TexPostBlurDiffusion,
        category: AddNodeCategory::Texture,
    },
    AddNodeOption {
        kind: ProjectNodeKind::TexPostDistortion,
        category: AddNodeCategory::Texture,
    },
    AddNodeOption {
        kind: ProjectNodeKind::TexPostTemporal,
        category: AddNodeCategory::Texture,
    },
    AddNodeOption {
        kind: ProjectNodeKind::TexPostNoiseTexture,
        category: AddNodeCategory::Texture,
    },
    AddNodeOption {
        kind: ProjectNodeKind::TexPostLighting,
        category: AddNodeCategory::Texture,
    },
    AddNodeOption {
        kind: ProjectNodeKind::TexPostScreenSpace,
        category: AddNodeCategory::Texture,
    },
    AddNodeOption {
        kind: ProjectNodeKind::TexPostExperimental,
        category: AddNodeCategory::Texture,
    },
    AddNodeOption {
        kind: ProjectNodeKind::TexBlend,
        category: AddNodeCategory::Texture,
    },
    AddNodeOption {
        kind: ProjectNodeKind::CtlLfo,
        category: AddNodeCategory::Control,
    },
    AddNodeOption {
        kind: ProjectNodeKind::IoWindowOut,
        category: AddNodeCategory::Io,
    },
];

pub(super) const ADD_NODE_CATEGORIES: [AddNodeCategory; 6] = [
    AddNodeCategory::Texture,
    AddNodeCategory::Buffer,
    AddNodeCategory::Scene,
    AddNodeCategory::Render,
    AddNodeCategory::Control,
    AddNodeCategory::Io,
];

pub(super) fn category_count() -> usize {
    ADD_NODE_CATEGORIES.len()
}
