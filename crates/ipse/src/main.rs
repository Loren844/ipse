//! The `ipse` program: command line, MCP server and first-party connectors.

// A command-line program talks to the person on standard output.
#![allow(clippy::print_stdout)]

mod cmd;
mod connectors;
mod install;
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
        /// Make an invented memory to try Ipse without your own data
        #[arg(long, conflicts_with = "name")]
        demo: bool,
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
    /// Connect a calendars and contacts account (CalDAV, CardDAV), list or remove accounts
    Dav {
        #[command(subcommand)]
        action: DavAction,
    },
    /// Connect your Home Assistant, to know when you are at home and in which zones; list or remove it
    #[command(name = "homeassistant")]
    HomeAssistant {
        #[command(subcommand)]
        action: HomeAssistantAction,
    },
    /// Connect your Jellyfin, to know the films and series you watch; list or remove it
    Jellyfin {
        #[command(subcommand)]
        action: JellyfinAction,
    },
    /// Sync the calendars and accounts you follow
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
    /// Show or change the memory's language and time zone
    Settings {
        /// Language of the words Ipse writes in files: en or fr
        #[arg(long)]
        language: Option<String>,
        /// Time zone, like Europe/Paris
        #[arg(long)]
        time_zone: Option<String>,
        /// Folder of the memory (default: the current folder)
        #[arg(long)]
        vault: Option<PathBuf>,
    },
    /// Look for people who may be the same, and events copied into two calendars, and
    /// propose to merge them; nothing is merged until you accept
    Duplicates {
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
        /// Started by the desktop app: print only the address, and stop when the app
        /// closes this command's input
        #[arg(long, hide = true)]
        app: bool,
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
enum DavAction {
    /// Connect an account (iCloud, Fastmail, Nextcloud…) and follow its calendars and address books; asks its app password
    Add {
        /// The server's address, like cloud.example.org or caldav.icloud.com
        address: String,
        /// Your user name on the server
        #[arg(long)]
        user: String,
        /// A name for this account, like "iCloud"
        #[arg(long)]
        name: Option<String>,
        /// How often to sync it, like 15m or 1h (default: 15m)
        #[arg(long)]
        every: Option<String>,
        /// Folder of the memory (default: the current folder)
        #[arg(long)]
        vault: Option<PathBuf>,
    },
    /// List the accounts you connected
    List {
        /// Folder of the memory (default: the current folder)
        #[arg(long)]
        vault: Option<PathBuf>,
    },
    /// Stop syncing an account; its events and contacts stay in your memory
    Remove {
        /// The account's id, as `ipse dav list` shows it
        id: String,
        /// Folder of the memory (default: the current folder)
        #[arg(long)]
        vault: Option<PathBuf>,
    },
}

#[derive(Debug, Subcommand)]
enum HomeAssistantAction {
    /// Connect Home Assistant and follow a person's presence; asks a long-lived access token
    Add {
        /// Its address, like homeassistant.local:8123 or ha.example.org
        address: String,
        /// Whom to follow, like person.ines (default: the only person of Home Assistant)
        #[arg(long)]
        person: Option<String>,
        /// A name for this source, like "Maison"
        #[arg(long)]
        name: Option<String>,
        /// How often to sync it, like 5m or 15m (default: 5m)
        #[arg(long)]
        every: Option<String>,
        /// Folder of the memory (default: the current folder)
        #[arg(long)]
        vault: Option<PathBuf>,
    },
    /// List the Home Assistants you connected
    List {
        /// Folder of the memory (default: the current folder)
        #[arg(long)]
        vault: Option<PathBuf>,
    },
    /// Stop syncing Home Assistant; what it brought stays in your memory
    Remove {
        /// The source's id, as `ipse homeassistant list` shows it
        id: String,
        /// Folder of the memory (default: the current folder)
        #[arg(long)]
        vault: Option<PathBuf>,
    },
}

#[derive(Debug, Subcommand)]
enum JellyfinAction {
    /// Connect Jellyfin and follow a user's films and episodes; asks an API key
    Add {
        /// Its address, like nas.local:8096 or media.example.org
        address: String,
        /// Whom to follow, by user id (default: the only user of Jellyfin)
        #[arg(long)]
        user: Option<String>,
        /// A name for this source, like "Films"
        #[arg(long)]
        name: Option<String>,
        /// How often to sync it, like 1h or 30m (default: 1h)
        #[arg(long)]
        every: Option<String>,
        /// Folder of the memory (default: the current folder)
        #[arg(long)]
        vault: Option<PathBuf>,
    },
    /// List the Jellyfins you connected
    List {
        /// Folder of the memory (default: the current folder)
        #[arg(long)]
        vault: Option<PathBuf>,
    },
    /// Stop syncing Jellyfin; what it brought stays in your memory
    Remove {
        /// The source's id, as `ipse jellyfin list` shows it
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
        /// Write Ipse into this app's MCP configuration, the code included, instead of
        /// showing the code: claude-desktop, claude-code or lm-studio
        #[arg(long, value_name = "APP")]
        install: Option<String>,
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
            demo,
        } => cmd::init::run(
            folder,
            name,
            language.as_deref(),
            time_zone.as_deref(),
            demo,
        ),
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
        Command::HomeAssistant { action } => home_assistant(action),
        Command::Jellyfin { action } => jellyfin(action),
        Command::Dav { action } => match action {
            DavAction::Add {
                address,
                user,
                name,
                every,
                vault,
            } => cmd::dav::add(&address, &user, name.as_deref(), every.as_deref(), vault),
            DavAction::List { vault } => cmd::dav::list(vault),
            DavAction::Remove { id, vault } => cmd::dav::remove(&id, vault),
        },
        Command::Sync { due, full, vault } => cmd::sync::run(due, full, vault),
        Command::Check { vault } => cmd::check::run(vault),
        Command::Duplicates { vault } => cmd::duplicates::run(vault),
        Command::Settings {
            language,
            time_zone,
            vault,
        } => cmd::settings::run(language.as_deref(), time_zone.as_deref(), vault),
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
                install,
                vault,
            } => cmd::client::add(&name, local, &propose, install.as_deref(), vault),
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
        Command::Ui {
            no_open,
            app,
            vault,
        } => ui::run(
            vault,
            if app {
                ui::Start::App
            } else if no_open {
                ui::Start::Print
            } else {
                ui::Start::Browser
            },
        ),
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

/// `ipse homeassistant`.
fn home_assistant(action: HomeAssistantAction) -> ipse_core::Result<()> {
    match action {
        HomeAssistantAction::Add {
            address,
            person,
            name,
            every,
            vault,
        } => cmd::server::add(
            &cmd::homeassistant::KIND,
            &address,
            person.as_deref(),
            name.as_deref(),
            every.as_deref(),
            vault,
        ),
        HomeAssistantAction::List { vault } => cmd::server::list(&cmd::homeassistant::KIND, vault),
        HomeAssistantAction::Remove { id, vault } => {
            cmd::server::remove(&cmd::homeassistant::KIND, &id, vault)
        }
    }
}

/// `ipse jellyfin`.
fn jellyfin(action: JellyfinAction) -> ipse_core::Result<()> {
    let kind = &cmd::jellyfin::KIND;
    match action {
        JellyfinAction::Add {
            address,
            user,
            name,
            every,
            vault,
        } => cmd::server::add(
            kind,
            &address,
            user.as_deref(),
            name.as_deref(),
            every.as_deref(),
            vault,
        ),
        JellyfinAction::List { vault } => cmd::server::list(kind, vault),
        JellyfinAction::Remove { id, vault } => cmd::server::remove(kind, &id, vault),
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
