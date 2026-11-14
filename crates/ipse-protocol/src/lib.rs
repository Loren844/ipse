//! Messages of the Ipse connector protocol, version 1.
//!
//! A connector is a separate program that talks to the Ipse core through JSON Lines
//! on its standard input and output (`docs/spec/connectors.md`). This crate holds the
//! message types and the line encoding, shared by the core and by connectors written
//! in Rust. It does not depend on the core.

pub mod connector;
mod manifest;
mod message;
mod record;
mod secret;

pub use manifest::{Access, FileAccess, Localized, Manifest, SyncSettings};
pub use message::{ConnectorMessage, CoreMessage, DoneSummary, ErrorCode, Mode, NeedKind};
pub use record::{Op, Record};
pub use secret::Secret;

/// Version of the protocol this crate implements.
pub const PROTOCOL_VERSION: u32 = 1;

/// Largest message allowed on the wire, in bytes, newline included (8 MiB).
pub const MAX_MESSAGE_BYTES: usize = 8 * 1024 * 1024;

/// Largest number of records in one batch.
pub const MAX_BATCH_RECORDS: usize = 1_000;

/// Largest number of batches a connector may send before receiving their `ack`.
pub const MAX_BATCHES_IN_FLIGHT: usize = 4;

/// Encodes a message as one JSON line, ending with `\n`.
///
/// # Errors
///
/// Returns an error if the message cannot be serialized (a map with non-string keys
/// in `data`, for example). Never fails for messages built from this crate's types
/// with JSON values.
pub fn to_line<M: serde::Serialize>(message: &M) -> serde_json::Result<Vec<u8>> {
    let mut line = serde_json::to_vec(message)?;
    line.push(b'\n');
    Ok(line)
}

/// Decodes one JSON line, with or without its trailing `\n` or `\r\n`.
///
/// # Errors
///
/// Returns an error if the line is not valid JSON, if its `type` is unknown, or if a
/// required field is missing or has the wrong type. Unknown fields are ignored.
pub fn from_line<'a, M: serde::Deserialize<'a>>(line: &'a [u8]) -> serde_json::Result<M> {
    let line = line.strip_suffix(b"\n").unwrap_or(line);
    let line = line.strip_suffix(b"\r").unwrap_or(line);
    serde_json::from_slice(line)
}
