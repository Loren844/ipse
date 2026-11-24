//! `ipse jellyfin`: connect the person's Jellyfin, list and remove it
//! (`docs/connectors/jellyfin.md`), as a [`server`](super::server).

use ipse_core::time::SignedDuration;

use super::server::Kind;
use crate::connectors::jellyfin::{ID, KEY_SECRET, SERVICE};

/// Jellyfin, followed for one user's films and episodes. It syncs every hour by
/// default: what is being watched is current for 30 days (`docs/spec/vault.md`, 5.2).
pub(crate) const KIND: Kind = Kind {
    connector: ID,
    service: SERVICE,
    secret: KEY_SECRET,
    key: "user",
    prompt: "API key",
    token: "the API key",
    who: "user",
    nobody: "This Jellyfin has no user. Create yours in Dashboard → Users, then connect again.",
    default_name: "Jellyfin",
    every: SignedDuration::from_hours(1),
    min_every: SignedDuration::from_mins(5),
};
