//! The `ipse` program: command line, MCP server and first-party connectors.

use std::process::ExitCode;

use clap::{Parser, Subcommand};
use ipse_core::Error;

/// Your memory, on your machine, for any AI.
#[derive(Debug, Parser)]
#[command(name = "ipse", version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Create a new memory in a folder (not yet available)
    Init,
    /// Import a file: contacts (.vcf), calendar (.ics) (not yet available)
    Import,
    /// Connect, list or remove an AI (not yet available)
    Client,
    /// Serve the memory to an AI over MCP (not yet available)
    Mcp,
    /// Run a built-in connector; started by Ipse itself (not yet available)
    #[command(hide = true)]
    Connector,
}

impl Command {
    fn name(&self) -> &'static str {
        match self {
            Self::Init => "`ipse init`",
            Self::Import => "`ipse import`",
            Self::Client => "`ipse client`",
            Self::Mcp => "`ipse mcp`",
            Self::Connector => "`ipse connector`",
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result: Result<(), Error> = Err(Error::NotYetAvailable(cli.command.name()));
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("{err}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    use super::Cli;

    #[test]
    fn command_line_definition_is_valid() {
        Cli::command().debug_assert();
    }
}
