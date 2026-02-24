//! Top-level command parsing for `covergen`.

use clap::{Parser, Subcommand};

use crate::bench::cli::BenchArgs;
use crate::v2::cli::V2Args;

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
    /// Default V2 args accepted directly without the `v2` subcommand.
    #[command(flatten)]
    pub v2: V2Args,
}

/// Supported top-level subcommands.
#[derive(Subcommand, Debug)]
pub(crate) enum CovergenCommand {
    /// Run V2 explicitly.
    V2(V2Args),
    /// Run benchmark and threshold workflows.
    Bench(BenchArgs),
    /// Legacy V1 command path (removed).
    V1,
}

#[cfg(test)]
mod tests {
    use super::{CovergenCli, CovergenCommand};
    use crate::v2::cli::V2Config;
    use clap::Parser;

    #[test]
    fn top_level_accepts_direct_v2_flags() {
        let cli = CovergenCli::parse_from(["covergen", "--size", "320"]);
        assert!(cli.command.is_none());
        let config = V2Config::from_args(cli.v2).expect("v2 config should parse");
        assert_eq!(config.width, 320);
        assert_eq!(config.height, 320);
    }

    #[test]
    fn bench_subcommand_is_parsed() {
        let cli = CovergenCli::parse_from(["covergen", "bench", "--samples", "2"]);
        assert!(matches!(cli.command, Some(CovergenCommand::Bench(_))));
    }
}
