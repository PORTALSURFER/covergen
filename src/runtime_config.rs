//! CLI configuration parsing for the default graph runtime.

use std::error::Error;
use std::time::{SystemTime, UNIX_EPOCH};

use clap::{Args, Parser, ValueEnum};

/// Runtime profile used by graph execution and preset generation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub enum V2Profile {
    #[default]
    #[value(alias = "q")]
    Quality,
    #[value(alias = "perf", alias = "p")]
    Performance,
}

/// Animation motion profile controlling temporal intensity and seed jitter.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub enum AnimationMotion {
    /// Balanced modulation with stable per-clip seed.
    #[default]
    #[value(alias = "medium", alias = "balanced")]
    Normal,
    /// Slow, low-amplitude modulation with stable per-clip seed.
    #[value(alias = "soft", alias = "calm")]
    Gentle,
    /// High modulation and per-frame seed jitter for aggressive motion.
    #[value(alias = "intense", alias = "high")]
    Wild,
}

impl AnimationMotion {
    /// Return temporal modulation scale for this motion profile.
    pub fn modulation_intensity(self) -> f32 {
        match self {
            Self::Gentle => 0.25,
            Self::Normal => 0.55,
            Self::Wild => 1.0,
        }
    }

    /// Return whether per-frame seed jitter should be enabled.
    pub fn use_seed_jitter(self) -> bool {
        matches!(self, Self::Wild)
    }
}

/// Candidate exploration settings for generate-score-select rendering.
#[derive(Debug, Clone)]
pub struct SelectionConfig {
    /// Number of low-resolution candidates to explore before final rendering.
    pub explore_candidates: u32,
    /// Maximum dimension used for low-resolution candidate scoring renders.
    pub explore_size: u32,
}

impl SelectionConfig {
    /// Return true when low-res candidate exploration is enabled.
    pub fn enabled(&self) -> bool {
        self.explore_candidates > 0
    }
}

/// Command-line flags used by `covergen`.
#[derive(Args, Debug, Clone)]
pub struct V2Args {
    /// Set square output size (same as setting width and height).
    #[arg(long)]
    size: Option<u32>,
    /// Output width in pixels.
    #[arg(long)]
    width: Option<u32>,
    /// Output height in pixels.
    #[arg(long)]
    height: Option<u32>,
    /// Seed used for deterministic generation.
    #[arg(long)]
    seed: Option<u32>,
    /// Number of images/clips to generate.
    #[arg(long, short = 'n', default_value_t = 1)]
    count: u32,
    /// Output path (or base path when count > 1).
    #[arg(long, short = 'o', default_value = "covergen.png")]
    output: String,
    /// Layer budget used by preset generation.
    #[arg(long, default_value_t = 4)]
    layers: u32,
    /// Supersampling antialias factor.
    #[arg(long, visible_alias = "aa", default_value_t = 1)]
    antialias: u32,
    /// Preset family name.
    #[arg(long, default_value = "hybrid-stack")]
    preset: String,
    /// Runtime quality/performance profile.
    #[arg(long, default_value = "quality")]
    profile: V2Profile,
    /// Enable clip animation mode.
    #[arg(long)]
    animate: bool,
    /// Clip length in seconds.
    #[arg(long, default_value_t = 30)]
    seconds: u32,
    /// Clip frame rate.
    #[arg(long, default_value_t = 30)]
    fps: u32,
    /// Keep intermediate frame PNGs after mp4 assembly.
    #[arg(long)]
    keep_frames: bool,
    /// Force Instagram Reels dimensions and enable animation mode.
    #[arg(long)]
    reels: bool,
    /// Temporal modulation profile.
    #[arg(long, default_value = "normal")]
    motion: AnimationMotion,
    /// Explore N low-res candidates and render top-scoring outputs at full quality.
    #[arg(long, default_value_t = 0)]
    explore_candidates: u32,
    /// Maximum low-res candidate dimension used by the exploration pass.
    #[arg(long, default_value_t = 320)]
    explore_size: u32,
}

/// Animation settings for clip generation.
#[derive(Debug, Clone)]
pub struct AnimationConfig {
    pub enabled: bool,
    pub seconds: u32,
    pub fps: u32,
    pub keep_frames: bool,
    pub motion: AnimationMotion,
}

/// Parsed command-line configuration.
#[derive(Debug, Clone)]
pub struct V2Config {
    pub width: u32,
    pub height: u32,
    pub seed: u32,
    pub count: u32,
    pub output: String,
    pub layers: u32,
    pub antialias: u32,
    pub preset: String,
    pub profile: V2Profile,
    pub animation: AnimationConfig,
    pub selection: SelectionConfig,
}

#[derive(Parser, Debug)]
#[command(disable_help_subcommand = true)]
struct V2ArgsParser {
    #[command(flatten)]
    args: V2Args,
}

impl V2Config {
    /// Parse runtime arguments.
    #[cfg(test)]
    pub fn parse(args: Vec<String>) -> Result<Self, Box<dyn Error>> {
        let parsed = parse_v2_args(args)?;
        Self::from_args(parsed)
    }

    /// Convert validated clap arguments into runtime configuration.
    pub fn from_args(args: V2Args) -> Result<Self, Box<dyn Error>> {
        let size = args.size.unwrap_or(1024);
        let mut width = args.width.unwrap_or(size);
        let mut height = args.height.unwrap_or(size);

        if args.reels {
            width = 1080;
            height = 1920;
        }

        let mut output = args.output;
        let animation_enabled = args.animate || args.reels;
        if animation_enabled && !output.to_ascii_lowercase().ends_with(".mp4") {
            output.push_str(".mp4");
        }

        let config = Self {
            width,
            height,
            seed: args.seed.unwrap_or_else(runtime_seed),
            count: args.count,
            output,
            layers: args.layers,
            antialias: args.antialias,
            preset: args.preset,
            profile: args.profile,
            animation: AnimationConfig {
                enabled: animation_enabled,
                seconds: args.seconds,
                fps: args.fps,
                keep_frames: args.keep_frames,
                motion: args.motion,
            },
            selection: SelectionConfig {
                explore_candidates: args.explore_candidates,
                explore_size: args.explore_size,
            },
        };
        validate_v2_config(&config)?;
        Ok(config)
    }

    /// Build a low-resolution exploration config from this runtime config.
    pub fn low_res_explore_config(&self) -> Option<Self> {
        if !self.selection.enabled() || self.animation.enabled {
            return None;
        }

        let max_dim = self.selection.explore_size.max(64);
        let longest = self.width.max(self.height).max(1);
        let scale = (max_dim as f32 / longest as f32).min(1.0);
        let width = ((self.width as f32 * scale).round() as u32).max(16);
        let height = ((self.height as f32 * scale).round() as u32).max(16);

        Some(Self {
            width,
            height,
            seed: self.seed,
            count: 1,
            output: self.output.clone(),
            layers: self.layers,
            antialias: 1,
            preset: self.preset.clone(),
            profile: self.profile,
            animation: AnimationConfig {
                enabled: false,
                seconds: self.animation.seconds,
                fps: self.animation.fps,
                keep_frames: false,
                motion: self.animation.motion,
            },
            selection: SelectionConfig {
                explore_candidates: 0,
                explore_size: self.selection.explore_size,
            },
        })
    }
}

#[cfg(test)]
fn parse_v2_args(args: Vec<String>) -> Result<V2Args, Box<dyn Error>> {
    let argv = std::iter::once("covergen".to_string()).chain(args);
    let parsed = V2ArgsParser::try_parse_from(argv)?;
    Ok(parsed.args)
}

fn validate_v2_config(config: &V2Config) -> Result<(), Box<dyn Error>> {
    if config.width == 0 || config.height == 0 {
        return Err("width and height must be greater than zero".into());
    }
    if config.count == 0 {
        return Err("count must be at least 1".into());
    }
    if config.layers == 0 {
        return Err("layers must be at least 1".into());
    }
    if config.antialias == 0 || config.antialias > 4 {
        return Err("antialias must be in range 1..=4".into());
    }
    if config.animation.seconds == 0 {
        return Err("animation duration must be at least 1 second".into());
    }
    if config.animation.fps == 0 || config.animation.fps > 120 {
        return Err("fps must be in range 1..=120".into());
    }
    if config.selection.explore_size < 16 {
        return Err("explore-size must be at least 16".into());
    }
    if config.animation.enabled && config.selection.enabled() {
        return Err("explore-candidates cannot be used with animation mode".into());
    }
    Ok(())
}

/// Generate a per-run seed when one is not explicitly supplied.
fn runtime_seed() -> u32 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64;
    let pid = u64::from(std::process::id());
    let mixed = splitmix64(nanos ^ (pid << 32));
    let seed = (mixed as u32) ^ ((mixed >> 32) as u32);
    if seed == 0 {
        0x9e37_79b9
    } else {
        seed
    }
}

fn splitmix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}

#[cfg(test)]
mod tests {
    use super::{AnimationMotion, V2Config, V2Profile};

    #[test]
    fn reels_mode_enables_animation_and_dimensions() {
        let cfg =
            V2Config::parse(vec!["--reels".to_string()]).expect("reels configuration should parse");
        assert_eq!(cfg.width, 1080);
        assert_eq!(cfg.height, 1920);
        assert!(cfg.animation.enabled);
    }

    #[test]
    fn animate_output_defaults_to_mp4_extension() {
        let cfg = V2Config::parse(vec![
            "--animate".to_string(),
            "--output".to_string(),
            "clip".to_string(),
        ])
        .expect("animation configuration should parse");
        assert!(cfg.output.ends_with(".mp4"));
    }

    #[test]
    fn motion_profile_parses_with_alias() {
        let cfg = V2Config::parse(vec!["--motion".to_string(), "soft".to_string()])
            .expect("motion profile should parse");
        assert_eq!(cfg.animation.motion, AnimationMotion::Gentle);
    }

    #[test]
    fn profile_parses_with_alias() {
        let cfg = V2Config::parse(vec!["--profile".to_string(), "perf".to_string()])
            .expect("profile should parse");
        assert_eq!(cfg.profile, V2Profile::Performance);
    }

    #[test]
    fn explicit_seed_is_preserved() {
        let cfg = V2Config::parse(vec!["--seed".to_string(), "12345".to_string()])
            .expect("seeded configuration should parse");
        assert_eq!(cfg.seed, 12345);
    }

    #[test]
    fn omitted_seed_generates_runtime_seed() {
        let cfg = V2Config::parse(Vec::new()).expect("default configuration should parse");
        assert_ne!(cfg.seed, 0);
    }

    #[test]
    fn parse_exploration_flags() {
        let cfg = V2Config::parse(vec![
            "--explore-candidates".to_string(),
            "12".to_string(),
            "--explore-size".to_string(),
            "256".to_string(),
        ])
        .expect("exploration configuration should parse");
        assert_eq!(cfg.selection.explore_candidates, 12);
        assert_eq!(cfg.selection.explore_size, 256);
    }

    #[test]
    fn low_res_explore_config_scales_dimensions() {
        let cfg = V2Config::parse(vec![
            "--width".to_string(),
            "1920".to_string(),
            "--height".to_string(),
            "1080".to_string(),
            "--explore-candidates".to_string(),
            "10".to_string(),
            "--explore-size".to_string(),
            "320".to_string(),
        ])
        .expect("explore configuration should parse");
        let low = cfg
            .low_res_explore_config()
            .expect("low-res explore config should be available");
        assert_eq!(low.width, 320);
        assert_eq!(low.height, 180);
        assert_eq!(low.antialias, 1);
        assert!(!low.selection.enabled());
    }

    #[test]
    fn exploration_rejected_for_animation_mode() {
        let err = V2Config::parse(vec![
            "--animate".to_string(),
            "--explore-candidates".to_string(),
            "8".to_string(),
        ])
        .expect_err("animation+exploration should be rejected");
        assert!(err.to_string().contains("explore-candidates"));
    }
}
