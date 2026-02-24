//! CLI configuration parsing for the V2 graph runtime.

use std::error::Error;
use std::time::{SystemTime, UNIX_EPOCH};

/// Runtime profile used by V2 execution and preset generation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum V2Profile {
    Quality,
    Performance,
}

impl V2Profile {
    fn parse(value: &str) -> Result<Self, Box<dyn Error>> {
        match value.trim().to_ascii_lowercase().as_str() {
            "quality" | "q" => Ok(Self::Quality),
            "performance" | "perf" | "p" => Ok(Self::Performance),
            _ => Err(format!("invalid profile '{value}', expected quality|performance").into()),
        }
    }
}

/// Animation motion profile controlling temporal intensity and seed jitter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnimationMotion {
    /// Slow, low-amplitude modulation with stable per-clip seed.
    Gentle,
    /// Balanced modulation with stable per-clip seed.
    Normal,
    /// High modulation and per-frame seed jitter for aggressive motion.
    Wild,
}

impl AnimationMotion {
    fn parse(value: &str) -> Result<Self, Box<dyn Error>> {
        match value.trim().to_ascii_lowercase().as_str() {
            "gentle" | "soft" | "calm" => Ok(Self::Gentle),
            "normal" | "medium" | "balanced" => Ok(Self::Normal),
            "wild" | "intense" | "high" => Ok(Self::Wild),
            _ => Err(format!("invalid motion '{value}', expected gentle|normal|wild").into()),
        }
    }

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

/// Animation settings for V2 clip generation.
#[derive(Debug, Clone)]
pub struct AnimationConfig {
    pub enabled: bool,
    pub seconds: u32,
    pub fps: u32,
    pub keep_frames: bool,
    pub reels: bool,
    pub motion: AnimationMotion,
}

/// Parsed V2 command-line configuration.
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
}

impl V2Config {
    /// Parse `covergen v2` arguments.
    pub fn parse(args: Vec<String>) -> Result<Self, Box<dyn Error>> {
        let mut cfg = Self {
            width: 1024,
            height: 1024,
            seed: 0,
            count: 1,
            output: "covergen_v2.png".to_string(),
            layers: 4,
            antialias: 1,
            preset: "hybrid-stack".to_string(),
            profile: V2Profile::Quality,
            animation: AnimationConfig {
                enabled: false,
                seconds: 30,
                fps: 30,
                keep_frames: false,
                reels: false,
                motion: AnimationMotion::Normal,
            },
        };
        let mut explicit_seed = false;

        let mut iter = args.into_iter();
        while let Some(arg) = iter.next() {
            match arg.as_str() {
                "--size" => {
                    let value = iter.next().ok_or("missing value for --size")?;
                    let size: u32 = value.parse()?;
                    cfg.width = size;
                    cfg.height = size;
                }
                "--width" => {
                    let value = iter.next().ok_or("missing value for --width")?;
                    cfg.width = value.parse()?;
                }
                "--height" => {
                    let value = iter.next().ok_or("missing value for --height")?;
                    cfg.height = value.parse()?;
                }
                "--seed" => {
                    let value = iter.next().ok_or("missing value for --seed")?;
                    cfg.seed = value.parse()?;
                    explicit_seed = true;
                }
                "--count" | "-n" => {
                    let value = iter.next().ok_or("missing value for --count")?;
                    cfg.count = value.parse()?;
                }
                "--output" | "-o" => {
                    cfg.output = iter.next().ok_or("missing value for --output")?;
                }
                "--layers" => {
                    let value = iter.next().ok_or("missing value for --layers")?;
                    cfg.layers = value.parse()?;
                }
                "--antialias" | "--aa" => {
                    let value = iter.next().ok_or("missing value for --antialias")?;
                    cfg.antialias = value.parse()?;
                }
                "--preset" => {
                    cfg.preset = iter.next().ok_or("missing value for --preset")?;
                }
                "--profile" => {
                    let value = iter.next().ok_or("missing value for --profile")?;
                    cfg.profile = V2Profile::parse(&value)?;
                }
                "--animate" => {
                    cfg.animation.enabled = true;
                }
                "--seconds" => {
                    let value = iter.next().ok_or("missing value for --seconds")?;
                    cfg.animation.seconds = value.parse()?;
                }
                "--fps" => {
                    let value = iter.next().ok_or("missing value for --fps")?;
                    cfg.animation.fps = value.parse()?;
                }
                "--keep-frames" => {
                    cfg.animation.keep_frames = true;
                }
                "--reels" => {
                    cfg.animation.reels = true;
                }
                "--motion" => {
                    let value = iter.next().ok_or("missing value for --motion")?;
                    cfg.animation.motion = AnimationMotion::parse(&value)?;
                }
                _ => return Err(format!("unknown v2 argument: {arg}").into()),
            }
        }

        if cfg.width == 0 || cfg.height == 0 {
            return Err("v2 width and height must be greater than zero".into());
        }
        if cfg.count == 0 {
            return Err("v2 count must be at least 1".into());
        }
        if cfg.layers == 0 {
            return Err("v2 layers must be at least 1".into());
        }
        if cfg.antialias == 0 || cfg.antialias > 4 {
            return Err("v2 antialias must be in range 1..=4".into());
        }
        if cfg.animation.seconds == 0 {
            return Err("v2 animation duration must be at least 1 second".into());
        }
        if cfg.animation.fps == 0 || cfg.animation.fps > 120 {
            return Err("v2 fps must be in range 1..=120".into());
        }

        if cfg.animation.reels {
            cfg.width = 1080;
            cfg.height = 1920;
            cfg.animation.enabled = true;
        }

        if cfg.animation.enabled && !cfg.output.to_ascii_lowercase().ends_with(".mp4") {
            cfg.output.push_str(".mp4");
        }
        if !explicit_seed {
            cfg.seed = runtime_seed();
        }

        Ok(cfg)
    }
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
    if seed == 0 { 0x9e37_79b9 } else { seed }
}

fn splitmix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}

#[cfg(test)]
mod tests {
    use super::{AnimationMotion, V2Config};

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
    fn motion_profile_parses() {
        let cfg = V2Config::parse(vec!["--motion".to_string(), "gentle".to_string()])
            .expect("motion profile should parse");
        assert_eq!(cfg.animation.motion, AnimationMotion::Gentle);
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
}
