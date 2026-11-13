//! `ipse init`: create a memory.

use std::io::{BufRead as _, IsTerminal as _, Write as _};
use std::path::PathBuf;

use ipse_core::vault::{Language, Settings, Vault};
use ipse_core::{Error, Result};

/// Creates a memory in `folder` (the current folder by default).
pub(crate) fn run(
    folder: Option<PathBuf>,
    name: Option<String>,
    language: Option<&str>,
    time_zone: Option<&str>,
) -> Result<()> {
    let folder = folder.unwrap_or_else(|| PathBuf::from("."));
    let settings = settings(language, time_zone)?;
    let name = match name {
        Some(name) => name,
        None => ask_name()?,
    };

    let vault = Vault::init(&folder, &name, settings)?;
    let owner = vault
        .owner_file()
        .strip_prefix(&folder)
        .unwrap_or(vault.owner_file());
    println!("Your memory is ready in {}.", folder.display());
    println!(
        "Your own file is {}: add there what you want your AI to know about you.",
        owner.display()
    );
    Ok(())
}

fn settings(language: Option<&str>, time_zone: Option<&str>) -> Result<Settings> {
    let system = Settings::from_system();
    let language = match language {
        None => system.language(),
        Some(code) => Language::parse(code)
            .ok_or_else(|| Error::Refused("The language must be en or fr.".to_owned()))?,
    };
    let time_zone = time_zone.unwrap_or(system.time_zone_name());
    Settings::new(language, time_zone)
        .ok_or_else(|| Error::Refused("The time zone must be a name like Europe/Paris.".to_owned()))
}

fn ask_name() -> Result<String> {
    let stdin = std::io::stdin();
    if !stdin.is_terminal() {
        return Err(Error::Refused(
            "Give your name with --name, for example: ipse init --name \"Camille Durand\""
                .to_owned(),
        ));
    }
    print!("What is your name? ");
    let io = |source| Error::Io {
        action: "read",
        path: PathBuf::from("<keyboard>"),
        source,
    };
    std::io::stdout().flush().map_err(io)?;
    let mut name = String::new();
    stdin.lock().read_line(&mut name).map_err(io)?;
    Ok(name.trim().to_owned())
}
