//! CLI configuration parsing for the V2 graph runtime.

use std::error::Error;

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
}

impl V2Config {
    /// Parse `covergen v2` arguments.
    pub fn parse(args: Vec<String>) -> Result<Self, Box<dyn Error>> {
        let mut cfg = Self {
            width: 1024,
            height: 1024,
            seed: 0x9e37_79b9,
            count: 1,
            output: "covergen_v2.png".to_string(),
            layers: 4,
            antialias: 1,
            preset: "hybrid-stack".to_string(),
            profile: V2Profile::Quality,
        };

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

        Ok(cfg)
    }
}
