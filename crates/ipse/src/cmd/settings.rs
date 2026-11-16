//! `ipse settings`: show or change the memory's language and time zone.

use std::path::PathBuf;

use ipse_core::vault::{Language, Vault};
use ipse_core::{Error, Result};

/// Shows the settings of `vault`; with `language` or `time_zone`, changes them first,
/// as one change of the history.
pub(crate) fn run(
    language: Option<&str>,
    time_zone: Option<&str>,
    vault: Option<PathBuf>,
) -> Result<()> {
    let mut vault = Vault::open(&vault.unwrap_or_else(|| PathBuf::from(".")))?;
    if language.is_some() || time_zone.is_some() {
        let language = match language {
            Some(code) => Language::parse(code)
                .ok_or_else(|| Error::Refused("The language must be en or fr.".to_owned()))?,
            None => vault.settings().language(),
        };
        let time_zone = time_zone.map_or_else(
            || vault.settings().time_zone_name().to_owned(),
            str::to_owned,
        );
        vault.change_settings(language, &time_zone)?;
        println!(
            "Settings changed. Files already written keep their words; Ipse reads both languages."
        );
    }
    let settings = vault.settings();
    println!(
        "Language of the words Ipse writes in files: {}",
        settings.language()
    );
    println!("Time zone: {}", settings.time_zone_name());
    Ok(())
}
