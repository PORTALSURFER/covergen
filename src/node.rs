//! Node, port, and temporal-control definitions for the V2 graph IR.

use super::temporal::{apply_add, apply_mul, sample};
use crate::chop::{ChopLfoNode, ChopMathNode, ChopRemapNode};
use crate::model::{LayerBlendMode, Params};
use crate::sop::{SopCircleNode, SopGeometryNode, SopSphereNode, TopCameraRenderNode};
use serde::{Deserialize, Serialize};

pub use super::temporal::{
    BlendTemporal, GenerateLayerTemporal, GraphTimeInput, MaskTemporal, SourceNoiseTemporal,
    TemporalCurve, TemporalModulation, ToneMapTemporal, WarpTransformTemporal,
};

/// Operator families used for graph authoring semantics.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OperatorFamily {
    /// Texture/image operators and post-processing nodes.
    Top,
    /// Channel/scalar operators.
    Chop,
    /// Geometry/shape operators.
    Sop,
    /// Terminal output contract nodes.
    Output,
}

/// Port categories supported by the V2 graph IR.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PortType {
    /// Single-channel luma texture payload.
    LumaTexture,
    /// Single-channel mask texture payload.
    MaskTexture,
    /// Scalar channel value payload.
    ChannelScalar,
    /// Signed-distance/geometry primitive payload.
    SopPrimitive,
}

/// GPU layer generation node parameters.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct GenerateLayerNode {
    /// Symmetry repetition count.
    pub symmetry: u32,
    /// Symmetry style selector.
    pub symmetry_style: u32,
    /// Fractal iteration count.
    pub iterations: u32,
    /// Deterministic random seed.
    pub seed: u32,
    /// Fill scale multiplier.
    pub fill_scale: f32,
    /// Fractal zoom multiplier.
    pub fractal_zoom: f32,
    /// Primary art-style selector.
    pub art_style: u32,
    /// Secondary art-style selector.
    pub art_style_secondary: u32,
    /// Blend amount between primary and secondary art styles.
    pub art_style_mix: f32,
    /// Bend/distortion strength.
    pub bend_strength: f32,
    /// Warp distortion strength.
    pub warp_strength: f32,
    /// Warp noise frequency.
    pub warp_frequency: f32,
    /// Tile scaling factor.
    pub tile_scale: f32,
    /// Tile phase offset.
    pub tile_phase: f32,
    /// Horizontal center offset in normalized graph space.
    pub center_x: f32,
    /// Vertical center offset in normalized graph space.
    pub center_y: f32,
    /// Shader-side layer count budget.
    pub shader_layer_count: u32,
    /// Layer blend mode in compositing stages.
    pub blend_mode: LayerBlendMode,
    /// Layer opacity multiplier.
    pub opacity: f32,
    /// Contrast multiplier.
    pub contrast: f32,
    /// Time-varying modulation controls.
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
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct SourceNoiseNode {
    /// Deterministic random seed.
    pub seed: u32,
    /// Spatial scale multiplier.
    pub scale: f32,
    /// Octave count for multi-octave noise.
    pub octaves: u32,
    /// Output amplitude multiplier.
    pub amplitude: f32,
    /// Output port type produced by this node.
    pub output_port: PortType,
    /// Time-varying modulation controls.
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
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct MaskNode {
    /// Base threshold in `[0, 1]`.
    pub threshold: f32,
    /// Softness amount in `[0, 1]`.
    pub softness: f32,
    /// Invert mask output when true.
    pub invert: bool,
    /// Time-varying modulation controls.
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
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct BlendNode {
    /// Blend mode operator.
    pub mode: LayerBlendMode,
    /// Blend opacity in `[0, 1]`.
    pub opacity: f32,
    /// Time-varying modulation controls.
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
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct ToneMapNode {
    /// Contrast multiplier.
    pub contrast: f32,
    /// Lower percentile clamp.
    pub low_pct: f32,
    /// Upper percentile clamp.
    pub high_pct: f32,
    /// Time-varying modulation controls.
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
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct WarpTransformNode {
    /// Warp strength multiplier.
    pub strength: f32,
    /// Warp frequency multiplier.
    pub frequency: f32,
    /// Base phase offset.
    pub phase: f32,
    /// Time-varying modulation controls.
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

/// Stateful feedback node that mixes prior-frame memory into current input.
///
/// The runtime stores one persistent GPU buffer per feedback node and updates it
/// after each frame so subsequent frames can evolve from prior state.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct StatefulFeedbackNode {
    /// Feedback amount in `[0, 1]`, where `0` keeps current input and `1` uses prior state.
    pub mix: f32,
}

/// Role of an output node in a graph with one or more outputs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OutputRole {
    /// Primary output used by default runtime encode/finalization.
    Primary,
    /// Additional output tap used for parallel products or module boundaries.
    Tap,
}

/// Output node contract describing role and output slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[allow(clippy::large_enum_variant)]
pub enum NodeKind {
    /// Layer-generation operator.
    GenerateLayer(GenerateLayerNode),
    /// Procedural noise source operator.
    SourceNoise(SourceNoiseNode),
    /// Threshold/soft-mask operator.
    Mask(MaskNode),
    /// Blend/composite operator.
    Blend(BlendNode),
    /// Tone-map/contrast operator.
    ToneMap(ToneMapNode),
    /// Warp/transform operator.
    WarpTransform(WarpTransformNode),
    /// Stateful feedback operator.
    StatefulFeedback(StatefulFeedbackNode),
    /// LFO scalar generator.
    ChopLfo(ChopLfoNode),
    /// Scalar math operator.
    ChopMath(ChopMathNode),
    /// Scalar remap operator.
    ChopRemap(ChopRemapNode),
    /// Circle SOP primitive generator.
    SopCircle(SopCircleNode),
    /// Sphere SOP primitive generator.
    SopSphere(SopSphereNode),
    /// SOP geometry assembler.
    SopGeometry(SopGeometryNode),
    /// Camera render operator for SOP primitives.
    TopCameraRender(TopCameraRenderNode),
    /// Output contract node.
    Output(OutputNode),
}

impl NodeKind {
    /// Return the operator family for this node.
    #[cfg(test)]
    pub const fn operator_family(self) -> OperatorFamily {
        match self {
            Self::GenerateLayer(_)
            | Self::SourceNoise(_)
            | Self::Mask(_)
            | Self::Blend(_)
            | Self::ToneMap(_)
            | Self::WarpTransform(_)
            | Self::StatefulFeedback(_)
            | Self::TopCameraRender(_) => OperatorFamily::Top,
            Self::ChopLfo(_) | Self::ChopMath(_) | Self::ChopRemap(_) => OperatorFamily::Chop,
            Self::SopCircle(_) | Self::SopSphere(_) | Self::SopGeometry(_) => OperatorFamily::Sop,
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
            Self::StatefulFeedback(_) => (slot == 0).then_some(PortType::LumaTexture),
            Self::ChopLfo(_) => None,
            Self::ChopMath(_) => (slot <= 1).then_some(PortType::ChannelScalar),
            Self::ChopRemap(_) => (slot == 0).then_some(PortType::ChannelScalar),
            Self::SopCircle(_) | Self::SopSphere(_) => None,
            Self::SopGeometry(_) => match slot {
                0 => Some(PortType::SopPrimitive),
                1 => Some(PortType::ChannelScalar),
                _ => None,
            },
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
            Self::StatefulFeedback(_) => Some(PortType::LumaTexture),
            Self::ChopLfo(_) | Self::ChopMath(_) | Self::ChopRemap(_) => {
                Some(PortType::ChannelScalar)
            }
            Self::SopCircle(_) | Self::SopSphere(_) | Self::SopGeometry(_) => {
                Some(PortType::SopPrimitive)
            }
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
            Self::StatefulFeedback(_) => (1, 1),
            Self::ChopLfo(_) => (0, 0),
            Self::ChopMath(_) => (1, 2),
            Self::ChopRemap(_) => (1, 1),
            Self::SopCircle(_) | Self::SopSphere(_) => (0, 0),
            Self::SopGeometry(_) => (1, 2),
            Self::TopCameraRender(_) => (1, 2),
            Self::Output(_) => (1, 1),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_node_constructors_set_expected_role_and_slot() {
        assert_eq!(OutputNode::primary().role, OutputRole::Primary);
        assert_eq!(OutputNode::primary().slot, 0);
        assert_eq!(OutputNode::tap(3).role, OutputRole::Tap);
        assert_eq!(OutputNode::tap(3).slot, 3);
    }

    #[test]
    fn blend_node_port_contract_matches_declared_slots() {
        let kind = NodeKind::Blend(BlendNode {
            mode: LayerBlendMode::Normal,
            opacity: 1.0,
            temporal: BlendTemporal::default(),
        });
        assert_eq!(kind.operator_family(), OperatorFamily::Top);
        assert_eq!(kind.input_port(0), Some(PortType::LumaTexture));
        assert_eq!(kind.input_port(1), Some(PortType::LumaTexture));
        assert_eq!(kind.input_port(2), Some(PortType::MaskTexture));
        assert_eq!(kind.input_port(3), None);
        assert_eq!(kind.output_port(), Some(PortType::LumaTexture));
        assert_eq!(kind.input_range(), (2, 3));
    }

    #[test]
    fn output_node_is_terminal_and_has_no_output_port() {
        let kind = NodeKind::Output(OutputNode::primary());
        assert_eq!(kind.operator_family(), OperatorFamily::Output);
        assert_eq!(kind.input_port(0), Some(PortType::LumaTexture));
        assert_eq!(kind.output_port(), None);
        assert_eq!(kind.input_range(), (1, 1));
    }
}
