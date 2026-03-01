//! CLI configuration parsing for the default graph runtime.

use std::error::Error;
use std::time::{SystemTime, UNIX_EPOCH};

use clap::{Args, Parser, ValueEnum};
use serde::{Deserialize, Serialize};

use crate::art_direction::{ArtDirectionArgs, ArtDirectionConfig};

const DEFAULT_GUI_TARGET_FPS: u32 = 60;
const DEFAULT_ANIMATION_SECONDS: u32 = 30;
const DEFAULT_ANIMATION_FPS: u32 = 60;

/// Runtime profile used by graph execution and preset generation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
pub enum V2Profile {
    #[default]
    #[value(alias = "q")]
    Quality,
    #[value(alias = "perf", alias = "p")]
    Performance,
}

/// Animation motion profile controlling temporal intensity and seed jitter.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
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

/// GUI present-mode policy for interactive editor rendering.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
pub enum GuiVsync {
    /// Use FIFO present mode for tear-free output.
    #[default]
    On,
    /// Disable sync when the platform supports immediate present.
    Off,
    /// Allow mailbox-style adaptive behavior where supported.
    Adaptive,
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectionConfig {
    /// Number of low-resolution candidates to explore before final rendering.
    pub explore_candidates: u32,
    /// Maximum dimension used for low-resolution candidate scoring renders.
    pub explore_size: u32,
    /// Optional bounded novelty comparison window. `0` keeps full-history novelty scoring.
    #[serde(default)]
    pub novelty_window: u32,
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
    #[arg(long, default_value_t = DEFAULT_ANIMATION_SECONDS)]
    seconds: u32,
    /// Clip frame rate.
    #[arg(long, default_value_t = DEFAULT_ANIMATION_FPS)]
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
    /// Limit novelty comparisons to the N most recent candidates (`0` = full history).
    #[arg(long, default_value_t = 0)]
    explore_novelty_window: u32,
    /// Write a reproducibility manifest JSON containing graph + runtime config.
    #[arg(long)]
    manifest_out: Option<String>,
    /// Replay from a reproducibility manifest JSON instead of generating a new graph.
    #[arg(long)]
    manifest_in: Option<String>,
    /// High-level creative steering controls.
    #[command(flatten)]
    art_direction: ArtDirectionArgs,
    /// Target GUI interaction refresh rate.
    #[arg(long, default_value_t = DEFAULT_GUI_TARGET_FPS)]
    gui_target_fps: u32,
    /// GUI swapchain synchronization mode.
    #[arg(long, default_value = "on")]
    gui_vsync: GuiVsync,
    /// Optional CSV output path for per-frame GUI timing trace.
    #[arg(long)]
    gui_perf_trace: Option<String>,
    /// Run deterministic synthetic drag motions for GUI benchmarking.
    #[arg(long)]
    gui_benchmark_drag: bool,
    /// Stop GUI benchmark mode after this many rendered frames.
    #[arg(long, default_value_t = 0)]
    gui_benchmark_frames: u32,
}

/// Animation settings for clip generation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnimationConfig {
    pub enabled: bool,
    pub seconds: u32,
    pub fps: u32,
    pub keep_frames: bool,
    pub motion: AnimationMotion,
}

/// GUI runtime tuning and telemetry settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuiConfig {
    pub target_fps: u32,
    pub vsync: GuiVsync,
    pub perf_trace: Option<String>,
    pub benchmark_drag: bool,
    pub benchmark_frames: u32,
}

impl Default for GuiConfig {
    fn default() -> Self {
        Self {
            target_fps: DEFAULT_GUI_TARGET_FPS,
            vsync: GuiVsync::On,
            perf_trace: None,
            benchmark_drag: false,
            benchmark_frames: 0,
        }
    }
}

/// Parsed command-line configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    pub manifest_out: Option<String>,
    pub manifest_in: Option<String>,
    pub art_direction: ArtDirectionConfig,
    pub animation: AnimationConfig,
    pub selection: SelectionConfig,
    #[serde(default)]
    pub gui: GuiConfig,
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
            manifest_out: args.manifest_out,
            manifest_in: args.manifest_in,
            art_direction: args.art_direction.into_config(),
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
                novelty_window: args.explore_novelty_window,
            },
            gui: GuiConfig {
                target_fps: args.gui_target_fps,
                vsync: args.gui_vsync,
                perf_trace: args.gui_perf_trace,
                benchmark_drag: args.gui_benchmark_drag,
                benchmark_frames: args.gui_benchmark_frames,
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
            manifest_out: None,
            manifest_in: None,
            art_direction: self.art_direction,
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
                novelty_window: self.selection.novelty_window,
            },
            gui: self.gui.clone(),
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
    if config.gui.target_fps < 30 || config.gui.target_fps > 360 {
        return Err("gui-target-fps must be in range 30..=360".into());
    }
    if config.selection.explore_size < 16 {
        return Err("explore-size must be at least 16".into());
    }
    if config.animation.enabled && config.selection.enabled() {
        return Err("explore-candidates cannot be used with animation mode".into());
    }
    if config.manifest_in.is_some() && config.selection.enabled() {
        return Err("explore-candidates cannot be used with manifest replay mode".into());
    }
    Ok(())
}

/// Generate a per-run seed when one is not explicitly supplied.
pub(crate) fn runtime_seed() -> u32 {
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
