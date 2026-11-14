//! `ipse client`: connect, list or remove an AI.

use std::path::PathBuf;

use ipse_core::rights::{Categories, Clients};
use ipse_core::vault::{DateTime, Vault};
use ipse_core::{Error, Result};

/// Connects a new AI and shows its access code, once. `local`: the person says it
/// runs entirely on this computer.
pub(crate) fn add(name: &str, local: bool, vault: Option<PathBuf>) -> Result<()> {
    let vault = open(vault)?;
    let categories = Categories::of(vault.settings());
    let mut clients = Clients::load(vault.root())?;
    let (client, token) = clients.add(
        name,
        DateTime::now(vault.settings().time_zone()),
        &categories,
        local,
    )?;
    println!(
        "{} can now read your memory: {}.",
        client.name,
        client.read.join(", ")
    );
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
    }
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
