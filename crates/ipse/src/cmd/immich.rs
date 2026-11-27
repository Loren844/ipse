//! `ipse immich`: connect the person's Immich, list and remove it
//! (`docs/connectors/immich.md`), as a [`server`](super::server).

use ipse_core::time::SignedDuration;

use super::server::Kind;
use crate::connectors::immich::{ID, KEY_SECRET, SERVICE};

/// Immich, read for the people of the photos of the user its API key belongs to. It
/// syncs every 6 hours by default.
pub(crate) const KIND: Kind = Kind {
    connector: ID,
    service: SERVICE,
    secret: KEY_SECRET,
    key: "user",
    prompt: "API key",
    token: "the API key",
    who: "user",
    nobody: "This API key belongs to no user of Immich.",
    default_name: "Immich",
    every: SignedDuration::from_hours(6),
    min_every: SignedDuration::from_mins(30),
};
