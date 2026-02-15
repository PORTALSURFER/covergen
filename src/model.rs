//! Core data types shared across GPU/CUDA renderers and strategy selection.

use bytemuck::{Pod, Zeroable};

/// Uniform buffer payload consumed by the WGSL shader.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct Params {
    /// Render width in pixels.
    pub(crate) width: u32,
    /// Render height in pixels.
    pub(crate) height: u32,
    /// Symmetry replication count for the selected base symmetry style.
    pub(crate) symmetry: u32,
    /// Encoded symmetry style to apply in shader code.
    pub(crate) symmetry_style: u32,
    /// Fractal iteration count for active style.
    pub(crate) iterations: u32,
    /// Deterministic random seed for this layer.
    pub(crate) seed: u32,
    /// User/world scale factor affecting shape fill region.
    pub(crate) fill_scale: f32,
    /// Zoom multiplier for world coordinate mapping.
    pub(crate) fractal_zoom: f32,
    /// Primary art style style index.
    pub(crate) art_style: u32,
    /// Secondary art style style index.
    pub(crate) art_style_secondary: u32,
    /// Mix factor between primary and secondary style fields.
    pub(crate) art_style_mix: f32,
    /// Domain-bending intensity.
    pub(crate) bend_strength: f32,
    /// Domain-warp strength.
    pub(crate) warp_strength: f32,
    /// Domain-warp frequency.
    pub(crate) warp_frequency: f32,
    /// Tiling scale override.
    pub(crate) tile_scale: f32,
    /// Tiling phase offset.
    pub(crate) tile_phase: f32,
    /// Base world offset in X.
    pub(crate) center_x: f32,
    /// Base world offset in Y.
    pub(crate) center_y: f32,
    /// Number of shader layers blended within a single layer.
    pub(crate) layer_count: u32,
}

/// Runtime filter kernels used by post-processing.
#[derive(Clone, Copy)]
pub(crate) enum FilterMode {
    Motion,
    Gaussian,
    Median,
    Bilateral,
}

impl FilterMode {
    pub(crate) fn from_u32(value: u32) -> Self {
        match value % 4 {
            0 => Self::Motion,
            1 => Self::Gaussian,
            2 => Self::Median,
            _ => Self::Bilateral,
        }
    }

    pub(crate) fn label(&self) -> &'static str {
        match self {
            Self::Motion => "motion",
            Self::Gaussian => "gaussian",
            Self::Median => "median",
            Self::Bilateral => "bilateral",
        }
    }
}

/// Available style families for layered shader rendering.
#[derive(Clone, Copy)]
pub(crate) enum ArtStyle {
    Hybrid,
    Julia,
    BurningShip,
    Tricorn,
    Phoenix,
    OrbitTrap,
    Field,
    Mandelbrot,
    Nova,
    Vortex,
    Dragon,
    Ifs,
    Moire,
    Knot,
    RadialWave,
    RecursiveFold,
    AttractorHybrid,
}

impl ArtStyle {
    /// Convert a random or profile value into a valid style.
    pub(crate) fn from_u32(value: u32) -> Self {
        match value % 17 {
            0 => Self::Hybrid,
            1 => Self::Julia,
            2 => Self::BurningShip,
            3 => Self::Tricorn,
            4 => Self::Phoenix,
            5 => Self::OrbitTrap,
            6 => Self::Field,
            7 => Self::Mandelbrot,
            8 => Self::Nova,
            9 => Self::Vortex,
            10 => Self::Dragon,
            11 => Self::Ifs,
            12 => Self::Moire,
            13 => Self::Knot,
            14 => Self::RadialWave,
            15 => Self::RecursiveFold,
            _ => Self::AttractorHybrid,
        }
    }

    /// Numeric representation used by shader and serialization.
    pub(crate) fn as_u32(self) -> u32 {
        match self {
            Self::Hybrid => 0,
            Self::Julia => 1,
            Self::BurningShip => 2,
            Self::Tricorn => 3,
            Self::Phoenix => 4,
            Self::OrbitTrap => 5,
            Self::Field => 6,
            Self::Mandelbrot => 7,
            Self::Nova => 8,
            Self::Vortex => 9,
            Self::Dragon => 10,
            Self::Ifs => 11,
            Self::Moire => 12,
            Self::Knot => 13,
            Self::RadialWave => 14,
            Self::RecursiveFold => 15,
            Self::AttractorHybrid => 16,
        }
    }

    /// Human-readable logging label.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Hybrid => "hybrid",
            Self::Julia => "julia",
            Self::BurningShip => "burning",
            Self::Tricorn => "tricorn",
            Self::Phoenix => "phoenix",
            Self::OrbitTrap => "orbit",
            Self::Field => "field",
            Self::Mandelbrot => "mandelbrot",
            Self::Nova => "nova",
            Self::Vortex => "vortex",
            Self::Dragon => "dragon",
            Self::Ifs => "ifs",
            Self::Moire => "moire",
            Self::Knot => "knot",
            Self::RadialWave => "radial-wave",
            Self::RecursiveFold => "recursive-fold",
            Self::AttractorHybrid => "attractor-hybrid",
        }
    }

    /// Number of registered styles.
    pub(crate) const fn total() -> u32 {
        17
    }

    /// Styles that usually produce periodic tiles.
    pub(crate) fn is_tiling_like(self) -> bool {
        matches!(
            self,
            Self::Field | Self::RadialWave | Self::Knot | Self::RecursiveFold | Self::Moire
        )
    }

    /// Pick a style avoiding the most tile-like families when possible.
    pub(crate) fn next_non_tiling_from(rng: &mut XorShift32) -> Self {
        let mut candidate = Self::from_u32(rng.next_u32());
        if !candidate.is_tiling_like() {
            return candidate;
        }

        // Keep trying until we hit a non-tiling style.
        for _ in 0..Self::total() {
            candidate = Self::from_u32(candidate.as_u32() + 1);
            if !candidate.is_tiling_like() {
                break;
            }
        }

        candidate
    }
}

#[derive(Clone, Copy)]
pub(crate) enum GradientMode {
    Linear,
    Contrast,
    Gamma,
    Sine,
    Sigmoid,
    Posterize,
}

impl GradientMode {
    pub(crate) fn from_u32(value: u32) -> Self {
        match value % 6 {
            0 => Self::Linear,
            1 => Self::Contrast,
            2 => Self::Gamma,
            3 => Self::Sine,
            4 => Self::Sigmoid,
            _ => Self::Posterize,
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct BlurConfig {
    pub(crate) mode: FilterMode,
    pub(crate) max_radius: u32,
    pub(crate) axis_x: i32,
    pub(crate) axis_y: i32,
    pub(crate) softness: u32,
}

#[derive(Clone, Copy)]
pub(crate) struct GradientConfig {
    pub(crate) mode: GradientMode,
    pub(crate) gamma: f32,
    pub(crate) contrast: f32,
    pub(crate) pivot: f32,
    pub(crate) invert: bool,
    pub(crate) frequency: f32,
    pub(crate) phase: f32,
    pub(crate) bands: u32,
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum SymmetryStyle {
    None,
    Radial,
    Mirror,
    MirrorX,
    MirrorY,
    MirrorDiagonal,
    MirrorCross,
    Grid,
}

impl SymmetryStyle {
    pub(crate) fn from_u32(value: u32) -> Self {
        match value % 8 {
            0 => Self::None,
            1 => Self::Radial,
            2 => Self::Mirror,
            3 => Self::MirrorX,
            4 => Self::MirrorY,
            5 => Self::MirrorDiagonal,
            6 => Self::MirrorCross,
            _ => Self::Grid,
        }
    }

    pub(crate) fn as_u32(self) -> u32 {
        match self {
            Self::None => 0,
            Self::Radial => 1,
            Self::Mirror => 2,
            Self::MirrorX => 3,
            Self::MirrorY => 4,
            Self::MirrorDiagonal => 5,
            Self::MirrorCross => 6,
            Self::Grid => 7,
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Radial => "radial",
            Self::Mirror => "mirror",
            Self::MirrorX => "mirror-x",
            Self::MirrorY => "mirror-y",
            Self::MirrorDiagonal => "mirror-diagonal",
            Self::MirrorCross => "mirror-cross",
            Self::Grid => "grid",
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum LayerBlendMode {
    Normal,
    Add,
    Multiply,
    Screen,
    Overlay,
    Difference,
    Lighten,
    Darken,
    Glow,
    Shadow,
}

impl LayerBlendMode {
    pub(crate) fn from_u32(value: u32) -> Self {
        match value % 10 {
            0 => Self::Normal,
            1 => Self::Add,
            2 => Self::Multiply,
            3 => Self::Screen,
            4 => Self::Overlay,
            5 => Self::Difference,
            6 => Self::Lighten,
            7 => Self::Darken,
            8 => Self::Glow,
            _ => Self::Shadow,
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Add => "add",
            Self::Multiply => "multiply",
            Self::Screen => "screen",
            Self::Overlay => "overlay",
            Self::Difference => "difference",
            Self::Lighten => "lighten",
            Self::Darken => "darken",
            Self::Glow => "glow",
            Self::Shadow => "shadow",
        }
    }
}

/// Lightweight xorshift RNG used for all deterministic randomness in generation.
#[derive(Clone, Copy)]
pub(crate) struct XorShift32 {
    state: u32,
}

impl XorShift32 {
    pub(crate) fn new(seed: u32) -> Self {
        let state = if seed == 0 { 0x9e3779b9 } else { seed };
        Self { state }
    }

    /// Return next pseudorandom `u32` state.
    pub(crate) fn next_u32(&mut self) -> u32 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.state = x;
        x
    }

    /// Return next pseudorandom float in [0, 1).
    pub(crate) fn next_f32(&mut self) -> f32 {
        (self.next_u32() as f32) / (u32::MAX as f32)
    }
}
