//! Graph-time and temporal curve controls for V2 nodes.

/// Normalized graph-time input sampled once per rendered frame.
#[derive(Clone, Copy, Debug)]
pub struct GraphTimeInput {
    /// Normalized frame position in `[0, 1]`.
    pub normalized: f32,
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

        Self { normalized }
    }
}

/// Oscillator shape used by temporal parameter curves.
#[derive(Clone, Copy, Debug)]
pub enum TemporalWave {
    Sine,
}

/// Modulation curve sampled against normalized graph time.
#[derive(Clone, Copy, Debug)]
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
        self.offset + (self.amplitude * wave)
    }
}

/// Temporal modulation channels for one `GenerateLayerNode`.
#[derive(Clone, Copy, Debug, Default)]
pub struct GenerateLayerTemporal {
    pub iterations_scale: Option<TemporalCurve>,
    pub fill_scale_mul: Option<TemporalCurve>,
    pub fractal_zoom_mul: Option<TemporalCurve>,
    pub art_style_mix_add: Option<TemporalCurve>,
    pub warp_strength_mul: Option<TemporalCurve>,
    pub warp_frequency_add: Option<TemporalCurve>,
    pub tile_phase_add: Option<TemporalCurve>,
    pub center_x_add: Option<TemporalCurve>,
    pub center_y_add: Option<TemporalCurve>,
    pub opacity_mul: Option<TemporalCurve>,
    pub contrast_mul: Option<TemporalCurve>,
}

/// Temporal modulation channels for one `SourceNoiseNode`.
#[derive(Clone, Copy, Debug, Default)]
pub struct SourceNoiseTemporal {
    pub scale_mul: Option<TemporalCurve>,
    pub amplitude_mul: Option<TemporalCurve>,
}

/// Temporal modulation channels for one `MaskNode`.
#[derive(Clone, Copy, Debug, Default)]
pub struct MaskTemporal {
    pub threshold_add: Option<TemporalCurve>,
    pub softness_mul: Option<TemporalCurve>,
}

/// Temporal modulation channels for one `BlendNode`.
#[derive(Clone, Copy, Debug, Default)]
pub struct BlendTemporal {
    pub opacity_mul: Option<TemporalCurve>,
}

/// Temporal modulation channels for one `ToneMapNode`.
#[derive(Clone, Copy, Debug, Default)]
pub struct ToneMapTemporal {
    pub contrast_mul: Option<TemporalCurve>,
    pub low_pct_add: Option<TemporalCurve>,
    pub high_pct_add: Option<TemporalCurve>,
}

/// Temporal modulation channels for one `WarpTransformNode`.
#[derive(Clone, Copy, Debug, Default)]
pub struct WarpTransformTemporal {
    pub strength_mul: Option<TemporalCurve>,
    pub frequency_mul: Option<TemporalCurve>,
    pub phase_add: Option<TemporalCurve>,
}

pub(crate) fn apply_add(
    base: f32,
    curve: Option<TemporalCurve>,
    time: GraphTimeInput,
    min: f32,
    max: f32,
) -> f32 {
    (base + sample(curve, time)).clamp(min, max)
}

pub(crate) fn apply_mul(
    base: f32,
    curve: Option<TemporalCurve>,
    time: GraphTimeInput,
    min: f32,
    max: f32,
) -> f32 {
    (base * (1.0 + sample(curve, time))).clamp(min, max)
}

pub(crate) fn sample(curve: Option<TemporalCurve>, time: GraphTimeInput) -> f32 {
    curve.map(|value| value.sample(time)).unwrap_or(0.0)
}
