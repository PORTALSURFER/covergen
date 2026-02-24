//! Top-level command parsing for `covergen`.

use clap::{Parser, Subcommand};

use crate::bench::cli::BenchArgs;
use crate::runtime_config::V2Args;

/// Parsed top-level CLI arguments.
#[derive(Parser, Debug)]
#[command(
    name = "covergen",
    disable_help_subcommand = true,
    args_conflicts_with_subcommands = true,
    subcommand_negates_reqs = true
)]
pub(crate) struct CovergenCli {
    /// Optional top-level subcommand.
    #[command(subcommand)]
    pub command: Option<CovergenCommand>,
    /// Default render args accepted at top-level.
    #[command(flatten)]
    pub run: V2Args,
}

/// Supported top-level subcommands.
#[derive(Subcommand, Debug)]
pub(crate) enum CovergenCommand {
    /// Run benchmark and threshold workflows.
    Bench(BenchArgs),
}

#[cfg(test)]
mod tests {
    use super::{CovergenCli, CovergenCommand};
    use crate::runtime_config::V2Config;
    use clap::Parser;

    #[test]
    fn top_level_accepts_direct_runtime_flags() {
        let cli = CovergenCli::parse_from(["covergen", "--size", "320"]);
        assert!(cli.command.is_none());
        let config = V2Config::from_args(cli.run).expect("runtime config should parse");
        assert_eq!(config.width, 320);
        assert_eq!(config.height, 320);
    }

    #[test]
    fn bench_subcommand_is_parsed() {
        let cli = CovergenCli::parse_from(["covergen", "bench", "--samples", "2"]);
        assert!(matches!(cli.command, Some(CovergenCommand::Bench(_))));
    }
}
