//! `ipse homeassistant`: connect the person's Home Assistant, list and remove it
//! (`docs/connectors/homeassistant.md`), as a [`server`](super::server).

use std::path::PathBuf;

use ipse_core::Result;
use ipse_core::time::SignedDuration;
use ipse_core::vault::Vault;

use super::server::Kind;
use crate::connectors::homeassistant::{ID, SERVICE, TOKEN_SECRET};

/// Home Assistant, followed for one person's presence. It syncs every 5 minutes by
/// default: being at home is current for 15 minutes (`docs/spec/vault.md`, 5.2).
pub(crate) const KIND: Kind = Kind {
    connector: ID,
    service: SERVICE,
    secret: TOKEN_SECRET,
    key: "person",
    prompt: "Long-lived access token",
    token: "the token",
    who: "person",
    nobody: "This Home Assistant has no person. Add yourself in Settings → People, then connect again.",
    default_name: "Home Assistant",
    every: SignedDuration::from_mins(5),
    min_every: SignedDuration::from_mins(1),
};

/// `ipse homeassistant positions <id> [--off]`.
///
/// # Errors
///
/// Errors of [`super::server::set_positions`].
pub(crate) fn positions(id: &str, on: bool, vault: Option<PathBuf>) -> Result<()> {
    let vault = Vault::open(&vault.unwrap_or_else(|| PathBuf::from(".")))?;
    let synced = super::server::set_positions(&vault, id, on, &*crate::keychain::secrets())?;
    match (on, synced) {
        (_, None) => println!(
            "Nothing to change: {id} {} the positions of your phone.",
            if on { "already reads" } else { "does not read" }
        ),
        (true, Some(synced)) => {
            let visits = synced.location.map_or(0, |l| l.visits);
            println!(
                "{id} now reads where your phone has been, from the Home Assistant app on it: \
                 {visits} stays read. They are in the location category, closed to AIs until \
                 you open it."
            );
        }
        (false, Some(_)) => println!(
            "{id} no longer reads your phone's positions. The places and visits already read stay in your memory."
        ),
    }
    Ok(())
}
