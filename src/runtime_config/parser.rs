use std::error::Error;

use clap::Parser;

use super::V2Args;

#[derive(Parser, Debug)]
#[command(disable_help_subcommand = true)]
struct V2ArgsParser {
    #[command(flatten)]
    args: V2Args,
}

/// Parse CLI tokens into [`V2Args`] for test-only runtime-config coverage.
#[cfg(test)]
pub(super) fn parse_v2_args(args: Vec<String>) -> Result<V2Args, Box<dyn Error>> {
    let argv = std::iter::once("covergen".to_string()).chain(args);
    let parsed = V2ArgsParser::try_parse_from(argv)?;
    Ok(parsed.args)
}
