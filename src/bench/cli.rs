//! `covergen bench` argument parsing and validation.

use std::error::Error;
use std::path::PathBuf;

use clap::Args;
#[cfg(test)]
use clap::Parser;

use crate::runtime_config::V2Profile;

use super::BenchConfig;

/// Clap-facing arguments for `covergen bench`.
#[derive(Args, Clone, Debug)]
pub(crate) struct BenchArgs {
    /// Benchmark tier label used in reports and threshold files.
    #[arg(long, default_value = "unclassified")]
    tier: String,
    /// Number of still samples per scenario.
    #[arg(long, default_value_t = 8)]
    samples: u32,
    /// Number of animation samples per scenario.
    #[arg(long, default_value_t = 4)]
    animation_samples: u32,
    /// Require V2 still/animation scenarios to be captured.
    #[arg(long)]
    require_v2_scenarios: bool,
    /// Benchmark render size.
    #[arg(long, default_value_t = 1024)]
    size: u32,
    /// Benchmark seed.
    #[arg(long, default_value_t = 0x9E37_79B9)]
    seed: u32,
    /// Output directory for reports/artifacts.
    #[arg(long, default_value = "target/bench")]
    output_dir: PathBuf,
    /// Keep generated media artifacts.
    #[arg(long)]
    keep_artifacts: bool,
    /// V2 preset used during benchmark.
    #[arg(long, default_value = "mask-atlas")]
    preset: String,
    /// V2 profile used during benchmark.
    #[arg(long, default_value = "performance")]
    profile: V2Profile,
    /// Animation duration in seconds.
    #[arg(long, default_value_t = 6)]
    seconds: u32,
    /// Animation frame rate.
    #[arg(long, default_value_t = 24)]
    fps: u32,
    /// Path to threshold file used for validation.
    #[arg(long)]
    thresholds: Option<PathBuf>,
    /// Path where thresholds should be written from current run.
    #[arg(long = "lock-thresholds")]
    lock_thresholds: Option<PathBuf>,
}

#[cfg(test)]
#[derive(Parser, Debug)]
#[command(disable_help_subcommand = true)]
struct BenchArgsParser {
    #[command(flatten)]
    args: BenchArgs,
}

/// Parse `covergen bench` args into validated runtime benchmark config.
#[cfg(test)]
pub(crate) fn parse_bench_config(args: Vec<String>) -> Result<BenchConfig, Box<dyn Error>> {
    let argv = std::iter::once("bench".to_string()).chain(args);
    let parsed = BenchArgsParser::try_parse_from(argv)?;
    bench_config_from_args(parsed.args)
}

/// Convert clap args into validated benchmark config.
pub(crate) fn bench_config_from_args(args: BenchArgs) -> Result<BenchConfig, Box<dyn Error>> {
    let config = BenchConfig {
        tier: args.tier,
        samples: args.samples,
        animation_samples: args.animation_samples,
        require_v2_scenarios: args.require_v2_scenarios,
        size: args.size,
        seed: args.seed,
        out_dir: args.output_dir,
        keep_artifacts: args.keep_artifacts,
        preset: args.preset,
        profile: args.profile,
        animation_seconds: args.seconds,
        animation_fps: args.fps,
        thresholds_path: args.thresholds,
        lock_thresholds_path: args.lock_thresholds,
    };
    validate_bench_config(&config)?;
    Ok(config)
}

fn validate_bench_config(config: &BenchConfig) -> Result<(), Box<dyn Error>> {
    if config.samples == 0 {
        return Err("--samples must be at least 1".into());
    }
    if config.animation_samples == 0 {
        return Err("--animation-samples must be at least 1".into());
    }
    if config.size == 0 {
        return Err("--size must be greater than zero".into());
    }
    if config.animation_seconds == 0 {
        return Err("--seconds must be at least 1".into());
    }
    if config.animation_fps == 0 {
        return Err("--fps must be at least 1".into());
    }
    if config.tier.trim().is_empty() {
        return Err("--tier must not be empty".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::parse_bench_config;
    use crate::runtime_config::V2Profile;

    #[test]
    fn profile_alias_parses() {
        let cfg = parse_bench_config(vec![
            "--profile".to_string(),
            "perf".to_string(),
            "--samples".to_string(),
            "1".to_string(),
        ])
        .expect("bench args should parse");
        assert_eq!(cfg.profile, V2Profile::Performance);
    }

    #[test]
    fn zero_samples_is_rejected() {
        let err = parse_bench_config(vec![
            "--samples".to_string(),
            "0".to_string(),
            "--animation-samples".to_string(),
            "1".to_string(),
        ])
        .expect_err("bench args should reject zero samples");
        assert!(err.to_string().contains("--samples must be at least 1"));
    }
}
