//! `ipse homeassistant`: connect the person's Home Assistant, list and remove it
//! (`docs/connectors/homeassistant.md`), as a [`server`](super::server).

use ipse_core::time::SignedDuration;

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
