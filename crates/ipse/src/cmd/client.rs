//! `ipse client`: connect, list or remove an AI.

use std::path::PathBuf;

use ipse_core::rights::{Categories, Clients};
use ipse_core::vault::{DateTime, Vault};
use ipse_core::{Error, Result};

/// Connects a new AI and shows its access code, once. `local`: the person says it
/// runs entirely on this computer.
pub(crate) fn add(
    name: &str,
    local: bool,
    propose: &[String],
    vault: Option<PathBuf>,
) -> Result<()> {
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

fn open(vault: Option<PathBuf>) -> Result<Vault> {
    Vault::open(&vault.unwrap_or_else(|| PathBuf::from(".")))
}
