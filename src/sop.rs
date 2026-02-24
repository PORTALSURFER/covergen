//! SOP (surface-operator) node types for primitive geometry sources.

use serde::{Deserialize, Serialize};

/// 2D circle primitive in normalized camera space.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct SopCircleNode {
    pub radius: f32,
    pub feather: f32,
    pub center_x: f32,
    pub center_y: f32,
}

/// 3D-lit sphere primitive projected in normalized camera space.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct SopSphereNode {
    pub radius: f32,
    pub center_x: f32,
    pub center_y: f32,
    pub light_x: f32,
    pub light_y: f32,
    pub ambient: f32,
}

/// SOP geometry-modulation node driven by optional noise/channel inputs.
///
/// This node deforms primitive parameters before camera rasterization, enabling
/// graph shapes like `sphere -> noise -> geometry -> top-camera-render`.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct SopGeometryNode {
    /// Radius deformation response to modulation input.
    pub radius_response: f32,
    /// Center translation response to modulation input.
    pub center_response: f32,
    /// Lighting-direction response to modulation input.
    pub light_response: f32,
    /// Bias applied to modulation before shaping, in `[-1, 1]`.
    pub bias: f32,
}

/// TOP camera node that rasterizes SOP primitives to luma.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct TopCameraRenderNode {
    pub exposure: f32,
    pub gamma: f32,
    pub zoom: f32,
    pub pan_x: f32,
    pub pan_y: f32,
    pub rotate: f32,
    pub invert: bool,
}
