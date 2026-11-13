//! The `ipse` program: command line, MCP server and first-party connectors.

// A command-line program talks to the person on standard output.
#![allow(clippy::print_stdout)]

mod cmd;
mod connectors;
mod keychain;
mod mcp;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

/// Your memory, on your machine, for any AI.
#[derive(Debug, Parser)]
#[command(name = "ipse", version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Create a new memory in a folder
    Init {
        /// Folder to create the memory in (default: the current folder)
        folder: Option<PathBuf>,
        /// Your name, for your own file (asked if not given)
        #[arg(long)]
        name: Option<String>,
        /// Language of the words Ipse writes in files: en or fr (default: the computer's)
        #[arg(long)]
        language: Option<String>,
        /// Time zone, like Europe/Paris (default: the computer's)
        #[arg(long)]
        time_zone: Option<String>,
    },
    /// Import a file into the memory: contacts (.vcf)
    Import {
        /// The file to import
        file: PathBuf,
        /// Folder of the memory (default: the current folder)
        #[arg(long)]
        vault: Option<PathBuf>,
    },
    /// Subscribe to a calendar's link, list or remove subscriptions
    Calendar {
        #[command(subcommand)]
        action: CalendarAction,
    },
    /// Sync the calendars you subscribed to
    Sync {
        /// Only those whose interval has passed: for a job the system runs every few minutes
        #[arg(long)]
        due: bool,
        /// Fetch everything again, not only what changed
        #[arg(long)]
        full: bool,
        /// Folder of the memory (default: the current folder)
        #[arg(long)]
        vault: Option<PathBuf>,
    },
    /// Connect, list or remove an AI
    Client {
        #[command(subcommand)]
        action: ClientAction,
    },
    /// Serve the memory to an AI app over MCP; the AI app starts it with an access code
    Mcp {
        /// Folder of the memory (default: the current folder)
        #[arg(long)]
        vault: Option<PathBuf>,
    },
    /// Run a built-in connector; started by Ipse itself
    #[command(hide = true)]
    Connector {
        /// The connector to run, like `contacts`
        name: String,
    },
}

#[derive(Debug, Subcommand)]
enum CalendarAction {
    /// Follow a calendar's link (https:// or webcal://) and keep it in sync
    Subscribe {
        /// The calendar's address in iCal format, from your calendar app; kept in your system's keychain
        link: String,
        /// A name for this calendar, like "Perso"
        #[arg(long)]
        name: Option<String>,
        /// How often to sync it, like 15m or 1h (default: 15m)
        #[arg(long)]
        every: Option<String>,
        /// Folder of the memory (default: the current folder)
        #[arg(long)]
        vault: Option<PathBuf>,
    },
    /// List the calendars you subscribed to
    List {
        /// Folder of the memory (default: the current folder)
        #[arg(long)]
        vault: Option<PathBuf>,
    },
    /// Stop syncing a calendar; its events stay in your memory
    Unsubscribe {
        /// The calendar's id, as `ipse calendar list` shows it
        id: String,
        /// Folder of the memory (default: the current folder)
        #[arg(long)]
        vault: Option<PathBuf>,
    },
}

#[derive(Debug, Subcommand)]
enum ClientAction {
    /// Connect an AI and show its access code, once
    Add {
        /// A name for this AI, like "Claude"
        name: String,
        /// Folder of the memory (default: the current folder)
        #[arg(long)]
        vault: Option<PathBuf>,
    },
    /// List the connected AIs
    List {
        /// Folder of the memory (default: the current folder)
        #[arg(long)]
        vault: Option<PathBuf>,
    },
    /// Disconnect an AI: its access code stops working
    Remove {
        /// The AI's id, as `ipse client list` shows it
        id: String,
        /// Folder of the memory (default: the current folder)
        #[arg(long)]
        vault: Option<PathBuf>,
    },
}

fn main() -> ExitCode {
    let result = match Cli::parse().command {
        Command::Init {
            folder,
            name,
            language,
            time_zone,
        } => cmd::init::run(folder, name, language.as_deref(), time_zone.as_deref()),
        Command::Import { file, vault } => cmd::import::run(&file, vault),
        Command::Calendar { action } => match action {
            CalendarAction::Subscribe {
                link,
                name,
                every,
                vault,
            } => cmd::calendar::subscribe(&link, name.as_deref(), every.as_deref(), vault),
            CalendarAction::List { vault } => cmd::calendar::list(vault),
            CalendarAction::Unsubscribe { id, vault } => cmd::calendar::unsubscribe(&id, vault),
        },
        Command::Sync { due, full, vault } => cmd::sync::run(due, full, vault),
        Command::Client { action } => match action {
            ClientAction::Add { name, vault } => cmd::client::add(&name, vault),
            ClientAction::List { vault } => cmd::client::list(vault),
            ClientAction::Remove { id, vault } => cmd::client::remove(&id, vault),
        },
        Command::Mcp { vault } => mcp::run(vault),
        Command::Connector { name } => connectors::run(&name),
    };
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
