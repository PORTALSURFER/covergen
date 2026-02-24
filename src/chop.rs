//! CHOP (channel-operator) node types for scalar modulation graphs.

use serde::{Deserialize, Serialize};

/// Oscillator waveform used by `ChopLfoNode`.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum ChopWave {
    Sine,
    Triangle,
    Saw,
}

impl ChopWave {
    /// Build a waveform enum from stable integer encoding.
    pub fn from_u32(value: u32) -> Self {
        match value % 3 {
            0 => Self::Sine,
            1 => Self::Triangle,
            _ => Self::Saw,
        }
    }
}

/// Time-driven scalar oscillator node.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct ChopLfoNode {
    pub wave: ChopWave,
    pub frequency: f32,
    pub phase: f32,
    pub amplitude: f32,
    pub offset: f32,
}

/// Scalar math operation for combining one/two channel inputs.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum ChopMathMode {
    Add,
    Multiply,
    Min,
    Max,
    Mix,
}

impl ChopMathMode {
    /// Build operation enum from stable integer encoding.
    pub fn from_u32(value: u32) -> Self {
        match value % 5 {
            0 => Self::Add,
            1 => Self::Multiply,
            2 => Self::Min,
            3 => Self::Max,
            _ => Self::Mix,
        }
    }
}

/// Scalar math node driven by one or two channel inputs.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct ChopMathNode {
    pub mode: ChopMathMode,
    pub value: f32,
    pub blend: f32,
}

/// Scalar remap node with optional clamping.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct ChopRemapNode {
    pub in_min: f32,
    pub in_max: f32,
    pub out_min: f32,
    pub out_max: f32,
    pub clamp: bool,
}
