//! High-level art-direction controls for graph generation and rendering.
//!
//! These controls expose human-friendly creative intent knobs (mood, energy,
//! symmetry, chaos, palette) and map them into deterministic numeric tuning.

use clap::{Args, ValueEnum};

/// Emotional lighting and contrast intent for generated output.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub enum MoodDirection {
    /// Balanced contrast and neutral tonal bias.
    #[default]
    #[value(alias = "neutral")]
    Balanced,
    /// Darker, denser tone response.
    #[value(alias = "dark")]
    Moody,
    /// Lighter, airier tone response.
    #[value(alias = "light")]
    Bright,
    /// Soft contrast with gentle glow bias.
    #[value(alias = "soft")]
    Dreamy,
}

/// Overall motion/detail intensity target.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub enum EnergyDirection {
    /// Restrained modulation and lower geometric aggression.
    Low,
    /// Balanced default behavior.
    #[default]
    #[value(alias = "normal")]
    Medium,
    /// Strong modulation and larger geometric movement.
    #[value(alias = "high")]
    High,
}

/// Symmetry intent applied to generated layer parameters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub enum SymmetryDirection {
    /// Prefer lower symmetry counts for asymmetric compositions.
    #[value(alias = "asymmetric", alias = "loose")]
    Low,
    /// Balanced symmetry spread.
    #[default]
    #[value(alias = "mixed")]
    Medium,
    /// Prefer strong radial/mirror symmetry.
    #[value(alias = "strong", alias = "high")]
    High,
}

/// Controlled randomness level for structure and modulation variance.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub enum ChaosDirection {
    /// Keep structure stable and reduce extremes.
    #[value(alias = "low", alias = "calm")]
    Controlled,
    /// Balanced randomness.
    #[default]
    #[value(alias = "medium")]
    Balanced,
    /// Aggressive randomness and stronger extremes.
    #[value(alias = "high", alias = "wild")]
    Wild,
}

/// Palette family bias for layer style selection.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub enum PaletteDirection {
    /// Allow broad style variety.
    #[default]
    #[value(alias = "mixed")]
    Mixed,
    /// Bias styles toward warm/high-energy families.
    Warm,
    /// Bias styles toward cool/field-like families.
    Cool,
    /// Bias styles toward lower-chroma families.
    #[value(alias = "mono")]
    Monochrome,
    /// Bias styles toward high-contrast stylized families.
    Neon,
}

/// CLI args for high-level art-direction controls.
#[derive(Args, Debug, Clone)]
pub struct ArtDirectionArgs {
    /// Mood target controlling tone/contrast character.
    #[arg(long, default_value = "balanced")]
    pub mood: MoodDirection,
    /// Energy target controlling geometric and modulation intensity.
    #[arg(long, default_value = "medium")]
    pub energy: EnergyDirection,
    /// Symmetry target controlling radial/mirror bias.
    #[arg(long, default_value = "medium")]
    pub symmetry: SymmetryDirection,
    /// Chaos target controlling randomness and structural variance.
    #[arg(long, default_value = "balanced")]
    pub chaos: ChaosDirection,
    /// Palette target controlling style-family bias.
    #[arg(long, default_value = "mixed")]
    pub palette: PaletteDirection,
}

impl ArtDirectionArgs {
    /// Convert parsed CLI args into runtime art-direction config.
    pub fn into_config(self) -> ArtDirectionConfig {
        ArtDirectionConfig {
            mood: self.mood,
            energy: self.energy,
            symmetry: self.symmetry,
            chaos: self.chaos,
            palette: self.palette,
        }
    }
}

/// Runtime art-direction profile applied to graph node parameters.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct ArtDirectionConfig {
    pub mood: MoodDirection,
    pub energy: EnergyDirection,
    pub symmetry: SymmetryDirection,
    pub chaos: ChaosDirection,
    pub palette: PaletteDirection,
}

impl ArtDirectionConfig {
    /// Scale factor for motion/detail intensity.
    pub fn energy_gain(self) -> f32 {
        match self.energy {
            EnergyDirection::Low => 0.78,
            EnergyDirection::Medium => 1.0,
            EnergyDirection::High => 1.30,
        }
    }

    /// Scale factor for randomness and warp/offset amplitude.
    pub fn chaos_gain(self) -> f32 {
        match self.chaos {
            ChaosDirection::Controlled => 0.72,
            ChaosDirection::Balanced => 1.0,
            ChaosDirection::Wild => 1.35,
        }
    }

    /// Contrast multiplier bias derived from mood.
    pub fn mood_contrast_gain(self) -> f32 {
        match self.mood {
            MoodDirection::Balanced => 1.0,
            MoodDirection::Moody => 1.12,
            MoodDirection::Bright => 0.92,
            MoodDirection::Dreamy => 0.88,
        }
    }

    /// Opacity multiplier bias derived from mood.
    pub fn mood_opacity_gain(self) -> f32 {
        match self.mood {
            MoodDirection::Balanced => 1.0,
            MoodDirection::Moody => 1.06,
            MoodDirection::Bright => 0.96,
            MoodDirection::Dreamy => 0.90,
        }
    }

    /// Preferred symmetry-count envelope `(min, max)`.
    pub fn symmetry_range(self) -> (u32, u32) {
        match self.symmetry {
            SymmetryDirection::Low => (1, 4),
            SymmetryDirection::Medium => (2, 9),
            SymmetryDirection::High => (6, 14),
        }
    }

    /// Preferred style pool for palette-driven layer remapping.
    pub fn palette_style_pool(self) -> &'static [u32] {
        match self.palette {
            PaletteDirection::Mixed => &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16],
            PaletteDirection::Warm => &[2, 4, 5, 8, 10, 12, 13],
            PaletteDirection::Cool => &[1, 3, 6, 7, 9, 14, 16],
            PaletteDirection::Monochrome => &[3, 7, 11, 12, 15],
            PaletteDirection::Neon => &[0, 8, 9, 12, 13, 14, 16],
        }
    }

    /// Preferred art-style mix target.
    pub fn palette_mix_target(self) -> f32 {
        match self.palette {
            PaletteDirection::Mixed => 0.55,
            PaletteDirection::Warm => 0.62,
            PaletteDirection::Cool => 0.48,
            PaletteDirection::Monochrome => 0.22,
            PaletteDirection::Neon => 0.78,
        }
    }
}
