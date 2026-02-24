//! Graph-time and temporal curve controls for V2 nodes.

mod expression;

pub use expression::{TemporalExpression, TemporalExpressionError};
use serde::{Deserialize, Serialize};

/// Normalized graph-time input sampled once per rendered frame.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct GraphTimeInput {
    /// Normalized frame position in `[0, 1]`.
    pub normalized: f32,
    /// Global temporal intensity scale applied to modulation amplitudes.
    pub intensity: f32,
    /// Normalized delta between adjacent frames, used by slew-rate limiting.
    pub frame_step: f32,
    /// Optional modulation envelope clamp `(min, max)` applied after sampling.
    pub envelope: Option<(f32, f32)>,
    /// Optional per-frame slew limit for sampled modulation deltas.
    pub max_slew_per_frame: Option<f32>,
}

impl GraphTimeInput {
    /// Build a normalized graph-time input for a frame in a fixed frame count.
    pub fn from_frame(frame_index: u32, total_frames: u32) -> Self {
        let normalized = if total_frames <= 1 {
            0.0
        } else {
            frame_index as f32 / (total_frames - 1) as f32
        }
        .clamp(0.0, 1.0);

        Self {
            normalized,
            intensity: 1.0,
            frame_step: if total_frames <= 1 {
                0.0
            } else {
                1.0 / (total_frames - 1) as f32
            },
            envelope: None,
            max_slew_per_frame: None,
        }
    }

    /// Override the global temporal intensity for this sample.
    pub fn with_intensity(mut self, intensity: f32) -> Self {
        self.intensity = intensity.clamp(0.0, 1.5);
        self
    }

    /// Clamp sampled modulation with one envelope `(min, max)`.
    pub fn with_envelope(mut self, min: f32, max: f32) -> Self {
        self.envelope = Some((min.min(max), min.max(max)));
        self
    }

    /// Limit per-frame modulation change to `max_delta`.
    pub fn with_slew_limit(mut self, max_delta: f32) -> Self {
        self.max_slew_per_frame = Some(max_delta.max(0.0));
        self
    }
}

/// Oscillator shape used by temporal parameter curves.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum TemporalWave {
    Sine,
}

/// Modulation curve sampled against normalized graph time.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct TemporalCurve {
    /// Base wave function.
    pub wave: TemporalWave,
    /// Output amplitude applied to sampled wave.
    pub amplitude: f32,
    /// Number of cycles across normalized `[0, 1]` time.
    pub frequency: f32,
    /// Phase offset in normalized cycles.
    pub phase: f32,
    /// Constant offset added after wave sampling.
    pub offset: f32,
}

impl TemporalCurve {
    /// Construct a sine curve using cycles across the clip as frequency.
    pub const fn sine(amplitude: f32, frequency: f32, phase: f32, offset: f32) -> Self {
        Self {
            wave: TemporalWave::Sine,
            amplitude,
            frequency,
            phase,
            offset,
        }
    }

    /// Sample the curve at a graph-time input.
    pub fn sample(self, time: GraphTimeInput) -> f32 {
        let cycle = (time.normalized * self.frequency + self.phase).rem_euclid(1.0);
        let wave = match self.wave {
            TemporalWave::Sine => (cycle * std::f32::consts::TAU).sin(),
        };
        self.offset + (self.amplitude * time.intensity * wave)
    }
}

/// Temporal modulation source used by node channels.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum TemporalModulation {
    /// Legacy fixed sine-curve modulation.
    Curve(TemporalCurve),
    /// Expression DSL modulation evaluated per frame.
    Expr(TemporalExpression),
}

impl TemporalModulation {
    /// Parse one temporal expression string into a compiled modulation source.
    ///
    /// Supported variables:
    /// - `t`: normalized clip time in `[0, 1]`
    /// - `i`: global modulation intensity
    ///
    /// Example:
    /// `0.08 * sin((t * 0.9 + 0.2) * tau) * i`
    pub fn parse(expression: &str) -> Result<Self, TemporalExpressionError> {
        Ok(Self::Expr(TemporalExpression::parse(expression)?))
    }

    /// Evaluate the modulation source at graph-time sample.
    pub fn sample(self, time: GraphTimeInput) -> f32 {
        match self {
            Self::Curve(curve) => curve.sample(time),
            Self::Expr(expr) => expr.sample(time),
        }
    }
}

impl From<TemporalCurve> for TemporalModulation {
    fn from(value: TemporalCurve) -> Self {
        Self::Curve(value)
    }
}

impl From<TemporalExpression> for TemporalModulation {
    fn from(value: TemporalExpression) -> Self {
        Self::Expr(value)
    }
}

/// Temporal modulation channels for one `GenerateLayerNode`.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct GenerateLayerTemporal {
    pub iterations_scale: Option<TemporalModulation>,
    pub fill_scale_mul: Option<TemporalModulation>,
    pub fractal_zoom_mul: Option<TemporalModulation>,
    pub art_style_mix_add: Option<TemporalModulation>,
    pub warp_strength_mul: Option<TemporalModulation>,
    pub warp_frequency_add: Option<TemporalModulation>,
    pub tile_phase_add: Option<TemporalModulation>,
    pub center_x_add: Option<TemporalModulation>,
    pub center_y_add: Option<TemporalModulation>,
    pub opacity_mul: Option<TemporalModulation>,
    pub contrast_mul: Option<TemporalModulation>,
}

/// Temporal modulation channels for one `SourceNoiseNode`.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct SourceNoiseTemporal {
    pub scale_mul: Option<TemporalModulation>,
    pub amplitude_mul: Option<TemporalModulation>,
}

/// Temporal modulation channels for one `MaskNode`.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct MaskTemporal {
    pub threshold_add: Option<TemporalModulation>,
    pub softness_mul: Option<TemporalModulation>,
}

/// Temporal modulation channels for one `BlendNode`.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct BlendTemporal {
    pub opacity_mul: Option<TemporalModulation>,
}

/// Temporal modulation channels for one `ToneMapNode`.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct ToneMapTemporal {
    pub contrast_mul: Option<TemporalModulation>,
    pub low_pct_add: Option<TemporalModulation>,
    pub high_pct_add: Option<TemporalModulation>,
}

/// Temporal modulation channels for one `WarpTransformNode`.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct WarpTransformTemporal {
    pub strength_mul: Option<TemporalModulation>,
    pub frequency_mul: Option<TemporalModulation>,
    pub phase_add: Option<TemporalModulation>,
}

pub(crate) fn apply_add(
    base: f32,
    curve: Option<TemporalModulation>,
    time: GraphTimeInput,
    min: f32,
    max: f32,
) -> f32 {
    (base + sample(curve, time)).clamp(min, max)
}

pub(crate) fn apply_mul(
    base: f32,
    curve: Option<TemporalModulation>,
    time: GraphTimeInput,
    min: f32,
    max: f32,
) -> f32 {
    (base * (1.0 + sample(curve, time))).clamp(min, max)
}

pub(crate) fn sample(curve: Option<TemporalModulation>, time: GraphTimeInput) -> f32 {
    curve
        .map(|value| sample_modulation(value, time))
        .unwrap_or(0.0)
}

fn sample_modulation(modulation: TemporalModulation, time: GraphTimeInput) -> f32 {
    let mut current = modulation.sample(time);
    current = apply_envelope(current, time.envelope);
    if let Some(limit) = time.max_slew_per_frame.filter(|value| *value > 0.0) {
        if time.frame_step > 0.0 && time.normalized > 0.0 {
            let prev_time = GraphTimeInput {
                normalized: (time.normalized - time.frame_step).clamp(0.0, 1.0),
                intensity: time.intensity,
                frame_step: time.frame_step,
                envelope: time.envelope,
                max_slew_per_frame: None,
            };
            let prev = apply_envelope(modulation.sample(prev_time), time.envelope);
            current = current.clamp(prev - limit, prev + limit);
            current = apply_envelope(current, time.envelope);
        }
    }
    current
}

fn apply_envelope(value: f32, envelope: Option<(f32, f32)>) -> f32 {
    if let Some((min, max)) = envelope {
        value.clamp(min, max)
    } else {
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expression_modulation_matches_expected_signal() {
        let modulation =
            TemporalModulation::parse("0.1 * sin((t * 2.0 + 0.25) * tau) * i").unwrap();
        let sample = modulation.sample(GraphTimeInput::from_frame(5, 10).with_intensity(0.5));
        assert!(sample.is_finite());
        assert!(sample.abs() <= 0.05 + 1e-5);
    }

    #[test]
    fn expression_drives_node_channel() {
        let node = crate::node::SourceNoiseNode {
            seed: 123,
            scale: 4.0,
            octaves: 4,
            amplitude: 1.0,
            output_port: crate::node::PortType::LumaTexture,
            temporal: SourceNoiseTemporal {
                scale_mul: Some(TemporalModulation::parse("0.2 * i").unwrap()),
                amplitude_mul: None,
            },
        };

        let evaluated = node.with_time(GraphTimeInput {
            normalized: 0.75,
            intensity: 0.5,
            frame_step: 0.0,
            envelope: None,
            max_slew_per_frame: None,
        });
        assert!((evaluated.scale - 4.4).abs() < 1e-6);
    }

    #[test]
    fn envelope_clamps_modulation_sample() {
        let modulation = TemporalModulation::parse("2.0 * i").unwrap();
        let sampled = sample(
            Some(modulation),
            GraphTimeInput::from_frame(3, 10)
                .with_intensity(1.0)
                .with_envelope(-0.5, 0.5),
        );
        assert!((sampled - 0.5).abs() < 1e-6);
    }

    #[test]
    fn slew_limit_caps_frame_to_frame_delta() {
        let modulation = TemporalModulation::parse("t").unwrap();
        let sampled = sample(
            Some(modulation),
            GraphTimeInput::from_frame(5, 11).with_slew_limit(0.02),
        );
        // raw(t=0.5) would be 0.5, previous is 0.4, capped to 0.42.
        assert!((sampled - 0.42).abs() < 1e-4);
    }
}
