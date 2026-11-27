//! The settings screen's API: the memory's language and time zone, where it is, which
//! version of Ipse serves it. The theme and the interface's language are the browser's
//! own, kept by the page.

use ipse_core::rights::Gateway;
use ipse_core::vault::Language;
use ipse_core::{Error, Result};
use serde::{Deserialize, Serialize};

/// What `GET /api/settings` answers, and `POST /api/settings` after a change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct SettingsView {
    /// The language of the words Ipse writes in files: `fr` or `en`.
    language: String,
    /// The memory's time zone, an IANA name.
    time_zone: String,
    /// The memory's folder on this computer.
    folder: String,
    /// Ipse's version.
    version: String,
    /// Whether the disk holding the memory is encrypted (issue #262).
    disk: DiskView,
}

/// Whether the disk holding the memory is encrypted, and on which system, for the
/// steps to turn it on (decision 0009, issue #262).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct DiskView {
    /// `on`, `off` or `unknown`.
    state: String,
    /// `macos`, `windows`, `linux` or `other`.
    system: String,
}

/// The encryption of the disk holding the memory, as the interface shows it. Reads
/// the system's state at most every few minutes.
pub(crate) fn disk(gateway: &Gateway) -> DiskView {
    DiskView {
        state: crate::disk::of(gateway.vault().root()).as_str().to_owned(),
        system: crate::disk::system().to_owned(),
    }
}

/// `POST /api/settings`.
#[derive(Debug, Deserialize)]
struct ChangeBody {
    language: String,
    time_zone: String,
}

/// The settings as they are.
pub(crate) fn view(gateway: &Gateway) -> SettingsView {
    let settings = gateway.settings();
    SettingsView {
        language: settings.language().code().to_owned(),
        time_zone: settings.time_zone_name().to_owned(),
        folder: gateway.vault().root().display().to_string(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        disk: disk(gateway),
    }
}

/// Changes the memory's language and time zone, as the person.
///
/// # Errors
///
/// [`Error::Refused`] when the body cannot be read, or names an unknown language or
/// time zone; errors writing the settings or the history.
pub(crate) fn change(gateway: &mut Gateway, body: &[u8]) -> Result<SettingsView> {
    let body: ChangeBody = serde_json::from_slice(body)
        .map_err(|_| Error::Refused("The request could not be read.".to_owned()))?;
    let language = Language::parse(&body.language)
        .ok_or_else(|| Error::Refused("The language must be en or fr.".to_owned()))?;
    gateway.change_settings(language, &body.time_zone)?;
    Ok(view(gateway))
}
