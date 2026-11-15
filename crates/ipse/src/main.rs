//! The `ipse` program: command line, MCP server and first-party connectors.

// A command-line program talks to the person on standard output.
#![allow(clippy::print_stdout)]

mod cmd;
mod connectors;
mod keychain;
mod mcp;
mod ui;

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
    /// Import a file into the memory: contacts (.vcf) or a calendar (.ics)
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
    /// Say which files of the memory need fixing; changes nothing
    Check {
        /// Folder of the memory (default: the current folder)
        #[arg(long)]
        vault: Option<PathBuf>,
    },
    /// Review what AIs propose to change in your memory
    Proposals {
        #[command(subcommand)]
        action: ProposalsAction,
    },
    /// What is left for your AI to do: notes to read, people to tell apart
    Tasks {
        #[command(subcommand)]
        action: TasksAction,
    },
    /// Connect, list or remove an AI
    Client {
        #[command(subcommand)]
        action: ClientAction,
    },
    /// Serve the memory to an AI app over MCP; the AI app starts it with an access code
    Mcp {
        /// Serve over HTTP on this computer instead, at this port (default: 7420)
        #[arg(long, value_name = "PORT", num_args = 0..=1, default_missing_value = "7420")]
        http: Option<String>,
        /// Folder of the memory (default: the current folder)
        #[arg(long)]
        vault: Option<PathBuf>,
    },
    /// Open your memory in your browser, on this computer only
    Ui {
        /// Print the address instead of opening the browser
        #[arg(long)]
        no_open: bool,
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
enum ProposalsAction {
    /// List the proposals waiting for you (all of them with --all)
    List {
        /// Also those already accepted, refused or undone
        #[arg(long)]
        all: bool,
        /// Folder of the memory (default: the current folder)
        #[arg(long)]
        vault: Option<PathBuf>,
    },
    /// Show what a proposal would change
    Show {
        /// The proposal's id, or its first characters
        id: String,
        /// Folder of the memory (default: the current folder)
        #[arg(long)]
        vault: Option<PathBuf>,
    },
    /// Accept a proposal: your memory changes as it says
    Accept {
        /// The proposal's id, or its first characters
        id: String,
        /// Folder of the memory (default: the current folder)
        #[arg(long)]
        vault: Option<PathBuf>,
    },
    /// Refuse a proposal; nothing changes
    Refuse {
        /// The proposal's id, or its first characters
        id: String,
        /// Why, for the AI to do better next time
        #[arg(long)]
        comment: Option<String>,
        /// Folder of the memory (default: the current folder)
        #[arg(long)]
        vault: Option<PathBuf>,
    },
    /// Undo a proposal you accepted
    Undo {
        /// The proposal's id, or its first characters
        id: String,
        /// Folder of the memory (default: the current folder)
        #[arg(long)]
        vault: Option<PathBuf>,
    },
}

#[derive(Debug, Subcommand)]
enum TasksAction {
    /// List the open tasks (all of them with --all)
    List {
        /// Also those proposed or done
        #[arg(long)]
        all: bool,
        /// Folder of the memory (default: the current folder)
        #[arg(long)]
        vault: Option<PathBuf>,
    },
    /// Show a task in full, as an AI reads it
    Show {
        /// The task's id, or its first characters
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
        /// This AI runs entirely on this computer (a local model): recorded and shown
        #[arg(long)]
        local: bool,
        /// Categories where it may propose changes, like people,agenda (none by default)
        #[arg(long, value_delimiter = ',')]
        propose: Vec<String>,
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
    /// Change what an AI may read and propose changes in
    Set {
        /// The AI's id, as `ipse client list` shows it
        id: String,
        /// Every category it may read, like identity,people,agenda (replaces the list)
        #[arg(long, value_delimiter = ',', num_args = 0..=1, default_missing_value = "")]
        read: Option<Vec<String>>,
        /// Every category where it may propose changes (replaces the list; empty for none)
        #[arg(long, value_delimiter = ',', num_args = 0..=1, default_missing_value = "")]
        propose: Option<Vec<String>>,
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
        Command::Check { vault } => cmd::check::run(vault),
        Command::Proposals { action } => match action {
            ProposalsAction::List { all, vault } => cmd::proposals::list(all, vault),
            ProposalsAction::Show { id, vault } => cmd::proposals::show(&id, vault),
            ProposalsAction::Accept { id, vault } => cmd::proposals::accept(&id, vault),
            ProposalsAction::Refuse { id, comment, vault } => {
                cmd::proposals::refuse(&id, comment.as_deref(), vault)
            }
            ProposalsAction::Undo { id, vault } => cmd::proposals::undo(&id, vault),
        },
        Command::Tasks { action } => match action {
            TasksAction::List { all, vault } => cmd::tasks::list(all, vault),
            TasksAction::Show { id, vault } => cmd::tasks::show(&id, vault),
        },
        Command::Client { action } => match action {
            ClientAction::Add {
                name,
                local,
                propose,
                vault,
            } => cmd::client::add(&name, local, &propose, vault),
            ClientAction::Set {
                id,
                read,
                propose,
                vault,
            } => cmd::client::set(&id, read, propose, vault),
            ClientAction::List { vault } => cmd::client::list(vault),
            ClientAction::Remove { id, vault } => cmd::client::remove(&id, vault),
        },
        Command::Mcp { http, vault } => mcp::run(vault, http.as_deref()),
        Command::Ui { no_open, vault } => ui::run(vault, !no_open),
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
