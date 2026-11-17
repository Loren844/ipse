//! `ipse client`: connect, list or remove an AI.

use std::path::PathBuf;

use ipse_core::rights::{Categories, Clients};
use ipse_core::vault::{DateTime, Vault};
use ipse_core::{Error, Result};

use crate::install::{self, App, Places, System};

/// Connects a new AI and shows its access code, once; or, with `install`, writes it
/// into that app's MCP configuration and never shows it. `local`: the person says it
/// runs entirely on this computer (an app that runs local models is).
pub(crate) fn add(
    name: &str,
    local: bool,
    propose: &[String],
    install: Option<&str>,
    vault: Option<PathBuf>,
) -> Result<()> {
    let app = install.map(App::parse).transpose()?;
    let local = local || app.is_some_and(App::is_local);
    let vault = open(vault)?;
    let categories = Categories::of(vault.settings());
    let mut clients = Clients::load(vault.root())?;
    // Checked before the AI is added, so that a mistake adds nothing.
    if let Some(name) = propose
        .iter()
        .find(|c| !categories.default_read().contains(c))
    {
        return Err(Error::Refused(format!(
            "An AI can only propose changes in a category it reads, and a new AI reads no sensitive or unknown category: `{name}` cannot be given here. Use `ipse client set` after."
        )));
    }
    let (mut client, token) = clients.add(
        name,
        DateTime::now(vault.settings().time_zone()),
        &categories,
        local,
    )?;
    if !propose.is_empty() {
        client = clients.set_rights(&client.id, None, Some(propose.to_vec()), &categories)?;
    }
    println!(
        "{} can now read your memory: {}.",
        client.name,
        client.read.join(", ")
    );
    if !client.propose.is_empty() {
        println!(
            "It can propose changes in: {}. Nothing is written until you accept them (`ipse proposals list`).",
            client.propose.join(", ")
        );
    }
    println!(
        "Sensitive categories stay closed: {}.",
        categories.sensitive().join(", ")
    );
    if local {
        println!(
            "Marked as running entirely on this computer. Ipse cannot check it; to open a sensitive category to it, add the category to its `read` list in .ipse/clients.toml."
        );
    }
    if let Some(app) = app {
        return install_into(app, &vault, &mut clients, &client.id, token.expose());
    }
    println!();
    println!("Its access code, shown only this once:");
    println!();
    println!("    {}", token.expose());
    println!();
    println!("Give it to your AI app as IPSE_TOKEN (docs/guide/getting-started.md shows how).");
    Ok(())
}

/// Lists the connected AIs.
pub(crate) fn list(vault: Option<PathBuf>) -> Result<()> {
    let vault = open(vault)?;
    let clients = Clients::load(vault.root())?;
    if clients.list().is_empty() {
        println!("No AI is connected yet. Connect one with: ipse client add \"Claude\"");
        return Ok(());
    }
    for client in clients.list() {
        println!(
            "{} ({}){}, connected {}: reads {}",
            client.name,
            client.id,
            if client.local {
                ", on this computer"
            } else {
                ""
            },
            client.created,
            client.read.join(", ")
        );
        if !client.propose.is_empty() {
            println!("  proposes in {}", client.propose.join(", "));
        }
    }
    Ok(())
}

/// Changes what an AI may read and propose on.
pub(crate) fn set(
    id: &str,
    read: Option<Vec<String>>,
    propose: Option<Vec<String>>,
    vault: Option<PathBuf>,
) -> Result<()> {
    if read.is_none() && propose.is_none() {
        return Err(Error::Refused(
            "Say what to change: --read people,agenda or --propose people (empty to remove all)."
                .to_owned(),
        ));
    }
    let vault = open(vault)?;
    let categories = Categories::of(vault.settings());
    let mut clients = Clients::load(vault.root())?;
    let client = clients.set_rights(id, read, propose, &categories)?;
    println!(
        "{} reads {}.",
        client.name,
        if client.read.is_empty() {
            "nothing".to_owned()
        } else {
            client.read.join(", ")
        }
    );
    if client.propose.is_empty() {
        println!("It cannot propose changes.");
    } else {
        println!("It proposes changes in {}.", client.propose.join(", "));
    }
    let sensitive: Vec<&str> = categories
        .sensitive()
        .into_iter()
        .filter(|c| client.read.iter().any(|r| r == c))
        .collect();
    if !sensitive.is_empty() {
        println!("Sensitive categories open to it: {}.", sensitive.join(", "));
    }
    println!("This applies from its next request.");
    Ok(())
}

/// Disconnects an AI: its access code stops working at once.
pub(crate) fn remove(id: &str, vault: Option<PathBuf>) -> Result<()> {
    let vault = open(vault)?;
    let mut clients = Clients::load(vault.root())?;
    if clients.remove(id)? {
        println!("{id} is disconnected: its access code no longer works.");
        Ok(())
    } else {
        Err(Error::Refused(format!(
            "No connected AI is called {id}. See them with: ipse client list"
        )))
    }
}

/// Writes the AI `id` into `app`'s configuration with its code `token`, which is not
/// shown. If the file cannot be written, the AI is disconnected again: nothing would
/// use its code.
fn install_into(
    app: App,
    vault: &Vault,
    clients: &mut Clients,
    id: &str,
    token: &str,
) -> Result<()> {
    let places = Places::current().ok_or_else(|| {
        Error::Refused(
            "Ipse cannot find your home folder, so it cannot find the app's configuration. Connect it by hand (docs/guide/connecting.md)."
                .to_owned(),
        )
    })?;
    let path = app.config_path(System::current(), &places);
    let program = std::env::current_exe()
        .and_then(|p| p.canonicalize())
        .unwrap_or_else(|_| PathBuf::from("ipse"));
    let folder = vault
        .root()
        .canonicalize()
        .unwrap_or_else(|_| vault.root().to_owned());
    let done = match install::install(app, &path, &program, &folder, token) {
        Ok(done) => done,
        Err(e) => {
            clients.remove(id)?;
            return Err(e);
        }
    };
    println!();
    println!(
        "Ipse is set up in {}, with its access code: {}",
        app.label(),
        done.path.display()
    );
    if let Some(backup) = &done.backup {
        println!("The file as it was is kept in {}.", backup.display());
    }
    if let Some(old) = done
        .replaced
        .as_deref()
        .and_then(|t| clients.authenticate(t))
        .filter(|c| c.id != id)
    {
        println!(
            "It replaces the entry of \"{}\" ({}), whose code no longer works in {}. If nothing else uses it: ipse client remove {}",
            old.name,
            old.id,
            app.label(),
            old.id
        );
    }
    println!("{}", app.next_step());
    Ok(())
}

fn open(vault: Option<PathBuf>) -> Result<Vault> {
    Vault::open(&vault.unwrap_or_else(|| PathBuf::from(".")))
}
