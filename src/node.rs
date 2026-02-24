//! Node, port, and temporal-control definitions for the V2 graph IR.

use super::temporal::{apply_add, apply_mul, sample};
use crate::chop::{ChopLfoNode, ChopMathNode, ChopRemapNode};
use crate::model::{LayerBlendMode, Params};
use crate::sop::{SopCircleNode, SopSphereNode, TopCameraRenderNode};

pub use super::temporal::{
    BlendTemporal, GenerateLayerTemporal, GraphTimeInput, MaskTemporal, SourceNoiseTemporal,
    TemporalCurve, TemporalModulation, ToneMapTemporal, WarpTransformTemporal,
};

/// TouchDesigner-style operator families used for graph authoring semantics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OperatorFamily {
    Top,
    Chop,
    Sop,
    Output,
}

/// Port categories supported by the V2 graph IR.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortType {
    LumaTexture,
    MaskTexture,
    ChannelScalar,
    SopPrimitive,
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
    pub temporal: GenerateLayerTemporal,
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

    /// Apply graph-time modulation curves and return an evaluated per-frame node.
    pub fn with_time(self, time: GraphTimeInput) -> Self {
        let iterations_scale = 1.0 + sample(self.temporal.iterations_scale, time);
        let iterations = ((self.iterations as f32 * iterations_scale)
            .round()
            .clamp(32.0, 2400.0)) as u32;

        Self {
            iterations,
            fill_scale: apply_mul(
                self.fill_scale,
                self.temporal.fill_scale_mul,
                time,
                0.4,
                3.0,
            ),
            fractal_zoom: apply_mul(
                self.fractal_zoom,
                self.temporal.fractal_zoom_mul,
                time,
                0.2,
                2.6,
            ),
            art_style_mix: apply_add(
                self.art_style_mix,
                self.temporal.art_style_mix_add,
                time,
                0.0,
                1.0,
            ),
            warp_strength: apply_mul(
                self.warp_strength,
                self.temporal.warp_strength_mul,
                time,
                0.0,
                2.2,
            ),
            warp_frequency: apply_add(
                self.warp_frequency,
                self.temporal.warp_frequency_add,
                time,
                0.1,
                8.0,
            ),
            tile_phase: (self.tile_phase + sample(self.temporal.tile_phase_add, time))
                .rem_euclid(1.0),
            center_x: apply_add(self.center_x, self.temporal.center_x_add, time, -0.6, 0.6),
            center_y: apply_add(self.center_y, self.temporal.center_y_add, time, -0.6, 0.6),
            opacity: apply_mul(self.opacity, self.temporal.opacity_mul, time, 0.0, 1.0),
            contrast: apply_mul(self.contrast, self.temporal.contrast_mul, time, 1.0, 3.0),
            ..self
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
    pub temporal: SourceNoiseTemporal,
}

impl SourceNoiseNode {
    /// Apply graph-time modulation curves and return an evaluated per-frame node.
    pub fn with_time(self, time: GraphTimeInput) -> Self {
        Self {
            scale: apply_mul(self.scale, self.temporal.scale_mul, time, 0.05, 32.0),
            amplitude: apply_mul(self.amplitude, self.temporal.amplitude_mul, time, 0.0, 2.0),
            ..self
        }
    }
}

/// Mask extraction node converting luma into a soft threshold mask.
#[derive(Clone, Copy, Debug)]
pub struct MaskNode {
    pub threshold: f32,
    pub softness: f32,
    pub invert: bool,
    pub temporal: MaskTemporal,
}

impl MaskNode {
    /// Apply graph-time modulation curves and return an evaluated per-frame node.
    pub fn with_time(self, time: GraphTimeInput) -> Self {
        Self {
            threshold: apply_add(self.threshold, self.temporal.threshold_add, time, 0.0, 1.0),
            softness: apply_mul(self.softness, self.temporal.softness_mul, time, 0.0, 1.0),
            ..self
        }
    }
}

/// Explicit blend/composite node with optional mask input.
#[derive(Clone, Copy, Debug)]
pub struct BlendNode {
    pub mode: LayerBlendMode,
    pub opacity: f32,
    pub temporal: BlendTemporal,
}

impl BlendNode {
    /// Apply graph-time modulation curves and return an evaluated per-frame node.
    pub fn with_time(self, time: GraphTimeInput) -> Self {
        Self {
            opacity: apply_mul(self.opacity, self.temporal.opacity_mul, time, 0.0, 1.0),
            ..self
        }
    }
}

/// Tone-map node for post contrast/stretch style adjustments.
#[derive(Clone, Copy, Debug)]
pub struct ToneMapNode {
    pub contrast: f32,
    pub low_pct: f32,
    pub high_pct: f32,
    pub temporal: ToneMapTemporal,
}

impl ToneMapNode {
    /// Apply graph-time modulation curves and return an evaluated per-frame node.
    pub fn with_time(self, time: GraphTimeInput) -> Self {
        let low = apply_add(self.low_pct, self.temporal.low_pct_add, time, 0.0, 0.9);
        let high = apply_add(
            self.high_pct,
            self.temporal.high_pct_add,
            time,
            low + 0.01,
            1.0,
        );
        Self {
            contrast: apply_mul(self.contrast, self.temporal.contrast_mul, time, 1.0, 3.0),
            low_pct: low,
            high_pct: high,
            ..self
        }
    }
}

/// Warp/transform node for lightweight geometric modulation.
#[derive(Clone, Copy, Debug)]
pub struct WarpTransformNode {
    pub strength: f32,
    pub frequency: f32,
    pub phase: f32,
    pub temporal: WarpTransformTemporal,
}

impl WarpTransformNode {
    /// Apply graph-time modulation curves and return an evaluated per-frame node.
    pub fn with_time(self, time: GraphTimeInput) -> Self {
        Self {
            strength: apply_mul(self.strength, self.temporal.strength_mul, time, 0.0, 2.4),
            frequency: apply_mul(
                self.frequency,
                self.temporal.frequency_mul,
                time,
                0.05,
                12.0,
            ),
            phase: self.phase + sample(self.temporal.phase_add, time),
            ..self
        }
    }
}

/// Role of an output node in a graph with one or more outputs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputRole {
    /// Primary output used by default runtime encode/finalization.
    Primary,
    /// Additional output tap used for parallel products or module boundaries.
    Tap,
}

/// Output node contract describing role and output slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OutputNode {
    /// Semantic role of this output.
    pub role: OutputRole,
    /// Stable output slot index used to address parallel outputs.
    pub slot: u8,
}

impl OutputNode {
    /// Construct the default primary output contract.
    pub const fn primary() -> Self {
        Self {
            role: OutputRole::Primary,
            slot: 0,
        }
    }

    /// Construct a tap output contract for a non-primary slot.
    pub const fn tap(slot: u8) -> Self {
        Self {
            role: OutputRole::Tap,
            slot,
        }
    }
}

/// Graph node kinds supported by V2.
#[derive(Clone, Copy, Debug)]
pub enum NodeKind {
    GenerateLayer(GenerateLayerNode),
    SourceNoise(SourceNoiseNode),
    Mask(MaskNode),
    Blend(BlendNode),
    ToneMap(ToneMapNode),
    WarpTransform(WarpTransformNode),
    ChopLfo(ChopLfoNode),
    ChopMath(ChopMathNode),
    ChopRemap(ChopRemapNode),
    SopCircle(SopCircleNode),
    SopSphere(SopSphereNode),
    TopCameraRender(TopCameraRenderNode),
    Output(OutputNode),
}

impl NodeKind {
    /// Return the TouchDesigner-style family for this node.
    pub const fn operator_family(self) -> OperatorFamily {
        match self {
            Self::GenerateLayer(_)
            | Self::SourceNoise(_)
            | Self::Mask(_)
            | Self::Blend(_)
            | Self::ToneMap(_)
            | Self::WarpTransform(_)
            | Self::TopCameraRender(_) => OperatorFamily::Top,
            Self::ChopLfo(_) | Self::ChopMath(_) | Self::ChopRemap(_) => OperatorFamily::Chop,
            Self::SopCircle(_) | Self::SopSphere(_) => OperatorFamily::Sop,
            Self::Output(_) => OperatorFamily::Output,
        }
    }

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
            Self::ToneMap(_) => match slot {
                0 => Some(PortType::LumaTexture),
                1 => Some(PortType::ChannelScalar),
                _ => None,
            },
            Self::WarpTransform(_) => match slot {
                0 => Some(PortType::LumaTexture),
                1 => Some(PortType::ChannelScalar),
                _ => None,
            },
            Self::ChopLfo(_) => None,
            Self::ChopMath(_) => (slot <= 1).then_some(PortType::ChannelScalar),
            Self::ChopRemap(_) => (slot == 0).then_some(PortType::ChannelScalar),
            Self::SopCircle(_) | Self::SopSphere(_) => None,
            Self::TopCameraRender(_) => match slot {
                0 => Some(PortType::SopPrimitive),
                1 => Some(PortType::ChannelScalar),
                _ => None,
            },
            Self::Output(_) => (slot == 0).then_some(PortType::LumaTexture),
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
            Self::ChopLfo(_) | Self::ChopMath(_) | Self::ChopRemap(_) => {
                Some(PortType::ChannelScalar)
            }
            Self::SopCircle(_) | Self::SopSphere(_) => Some(PortType::SopPrimitive),
            Self::TopCameraRender(_) => Some(PortType::LumaTexture),
            Self::Output(_) => None,
        }
    }

    /// Returns inclusive minimum/maximum allowed input count.
    pub fn input_range(self) -> (usize, usize) {
        match self {
            Self::GenerateLayer(_) => (0, 1),
            Self::SourceNoise(_) => (0, 0),
            Self::Mask(_) => (1, 1),
            Self::Blend(_) => (2, 3),
            Self::ToneMap(_) => (1, 2),
            Self::WarpTransform(_) => (1, 2),
            Self::ChopLfo(_) => (0, 0),
            Self::ChopMath(_) => (1, 2),
            Self::ChopRemap(_) => (1, 1),
            Self::SopCircle(_) | Self::SopSphere(_) => (0, 0),
            Self::TopCameraRender(_) => (1, 2),
            Self::Output(_) => (1, 1),
        }
    }
}
