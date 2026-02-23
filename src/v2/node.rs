//! Node and port definitions for the V2 graph IR.

use crate::model::{LayerBlendMode, Params};

/// Port categories supported by the V2 graph IR.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortType {
    /// Single-channel image data in normalized [0, 1] range.
    LumaTexture,
    /// Single-channel mask data in normalized [0, 1] range.
    MaskTexture,
}

/// GPU layer generation node parameters.
#[derive(Clone, Copy, Debug)]
pub struct GenerateLayerNode {
    pub symmetry: u32,
    pub symmetry_style: u32,
    pub iterations: u32,
    pub seed: u32,
    pub fill_scale: f32,
    pub fractal_zoom: f32,
    pub art_style: u32,
    pub art_style_secondary: u32,
    pub art_style_mix: f32,
    pub bend_strength: f32,
    pub warp_strength: f32,
    pub warp_frequency: f32,
    pub tile_scale: f32,
    pub tile_phase: f32,
    pub center_x: f32,
    pub center_y: f32,
    pub shader_layer_count: u32,
    pub blend_mode: LayerBlendMode,
    pub opacity: f32,
    pub contrast: f32,
}

impl GenerateLayerNode {
    /// Convert this node to shader uniform payload for a target render size.
    pub fn to_params(self, width: u32, height: u32, seed_offset: u32) -> Params {
        Params {
            width,
            height,
            symmetry: self.symmetry,
            symmetry_style: self.symmetry_style,
            iterations: self.iterations,
            seed: self.seed.wrapping_add(seed_offset),
            fill_scale: self.fill_scale,
            fractal_zoom: self.fractal_zoom,
            art_style: self.art_style,
            art_style_secondary: self.art_style_secondary,
            art_style_mix: self.art_style_mix,
            bend_strength: self.bend_strength,
            warp_strength: self.warp_strength,
            warp_frequency: self.warp_frequency,
            tile_scale: self.tile_scale,
            tile_phase: self.tile_phase,
            center_x: self.center_x,
            center_y: self.center_y,
            layer_count: self.shader_layer_count,
        }
    }
}

/// Procedural source-node generating reusable noise maps.
#[derive(Clone, Copy, Debug)]
pub struct SourceNoiseNode {
    pub seed: u32,
    pub scale: f32,
    pub octaves: u32,
    pub amplitude: f32,
    pub output_port: PortType,
}

/// Mask extraction node converting luma into a soft threshold mask.
#[derive(Clone, Copy, Debug)]
pub struct MaskNode {
    pub threshold: f32,
    pub softness: f32,
    pub invert: bool,
}

/// Explicit blend/composite node with optional mask input.
#[derive(Clone, Copy, Debug)]
pub struct BlendNode {
    pub mode: LayerBlendMode,
    pub opacity: f32,
}

/// Tone-map node for post contrast/stretch style adjustments.
#[derive(Clone, Copy, Debug)]
pub struct ToneMapNode {
    pub contrast: f32,
    pub low_pct: f32,
    pub high_pct: f32,
}

/// Warp/transform node for lightweight geometric modulation.
#[derive(Clone, Copy, Debug)]
pub struct WarpTransformNode {
    pub strength: f32,
    pub frequency: f32,
    pub phase: f32,
}

/// Graph node kinds supported by V2.
#[derive(Clone, Copy, Debug)]
pub enum NodeKind {
    /// Produce a luma layer using the fractal compute shader.
    GenerateLayer(GenerateLayerNode),
    /// Produce a procedural source map from a seed.
    SourceNoise(SourceNoiseNode),
    /// Convert incoming luma into a soft mask.
    Mask(MaskNode),
    /// Blend two luma inputs with optional mask.
    Blend(BlendNode),
    /// Apply tone mapping to luma.
    ToneMap(ToneMapNode),
    /// Apply geometric warp to luma.
    WarpTransform(WarpTransformNode),
    /// Terminal node indicating which stream should be encoded.
    Output,
}

impl NodeKind {
    /// Returns the accepted input type for an input slot.
    pub fn input_port(self, slot: u8) -> Option<PortType> {
        match self {
            Self::GenerateLayer(_) => (slot == 0).then_some(PortType::LumaTexture),
            Self::SourceNoise(_) => None,
            Self::Mask(_) => (slot == 0).then_some(PortType::LumaTexture),
            Self::Blend(_) => match slot {
                0 | 1 => Some(PortType::LumaTexture),
                2 => Some(PortType::MaskTexture),
                _ => None,
            },
            Self::ToneMap(_) => (slot == 0).then_some(PortType::LumaTexture),
            Self::WarpTransform(_) => (slot == 0).then_some(PortType::LumaTexture),
            Self::Output => (slot == 0).then_some(PortType::LumaTexture),
        }
    }

    /// Returns output type when this node produces a value.
    pub fn output_port(self) -> Option<PortType> {
        match self {
            Self::GenerateLayer(_) => Some(PortType::LumaTexture),
            Self::SourceNoise(spec) => Some(spec.output_port),
            Self::Mask(_) => Some(PortType::MaskTexture),
            Self::Blend(_) => Some(PortType::LumaTexture),
            Self::ToneMap(_) => Some(PortType::LumaTexture),
            Self::WarpTransform(_) => Some(PortType::LumaTexture),
            Self::Output => None,
        }
    }

    /// Returns inclusive minimum/maximum allowed input count.
    pub fn input_range(self) -> (usize, usize) {
        match self {
            Self::GenerateLayer(_) => (0, 1),
            Self::SourceNoise(_) => (0, 0),
            Self::Mask(_) => (1, 1),
            Self::Blend(_) => (2, 3),
            Self::ToneMap(_) => (1, 1),
            Self::WarpTransform(_) => (1, 1),
            Self::Output => (1, 1),
        }
    }
}
