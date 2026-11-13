//! The messages exchanged during a connector session (`docs/spec/connectors.md`, section 5).

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::{Record, Secret};

/// How a connector runs (`docs/spec/connectors.md`, section 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// Reads files the person picked, then exits.
    Import,
    /// Sends what changed since its cursor, then exits.
    Sync,
    /// Stays up and streams observations.
    Watch,
}

/// A message from the core to a connector.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CoreMessage {
    /// Opens the session.
    Hello {
        /// Protocol version the core speaks.
        protocol: u32,
        /// The instance this session is for: `calendar`, `calendar-2`.
        instance: String,
        /// The mode asked for.
        mode: Mode,
        /// The language the person reads messages in: the interface's, not necessarily
        /// the vault's. `en` for the command line.
        language: String,
        /// The vault's IANA time zone: `Europe/Paris`.
        time_zone: String,
        /// Where to resume, as returned last time; `null` on the first run.
        cursor: Option<Value>,
        /// The instance's settings.
        #[serde(default)]
        config: Map<String, Value>,
        /// Secrets read from the system keychain for this session only.
        #[serde(default)]
        secrets: BTreeMap<String, Secret>,
        /// Files the person picked, for `import` mode.
        #[serde(default)]
        files: Vec<PathBuf>,
    },
    /// Asks the connector to start sending.
    Start {
        /// Send everything, ignoring the cursor.
        full: bool,
    },
    /// Confirms that a batch is safely written.
    Ack {
        /// The batch's sequence number.
        seq: u64,
        /// Positions (0-based) of records that were malformed and not written.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        rejected: Vec<usize>,
    },
    /// Ends the session; the connector exits within 5 seconds.
    Bye,
}

/// A message from a connector to the core.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ConnectorMessage {
    /// Answers `hello`.
    Ready {
        /// Protocol version the connector speaks.
        protocol: u32,
        /// The connector's identifier.
        connector: String,
        /// The connector's version.
        version: String,
    },
    /// A batch of records.
    Batch {
        /// Starts at 1, grows by 1 per batch in the session.
        seq: u64,
        /// At most [`crate::MAX_BATCH_RECORDS`] records.
        records: Vec<Record>,
        /// Where to resume if everything up to this batch is kept.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cursor: Option<Value>,
    },
    /// Progress to show the person.
    Progress {
        /// Items done so far.
        done: u64,
        /// Items in total, when known.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        total: Option<u64>,
        /// What is happening, in the session's language.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
    /// The connector cannot go on without the person.
    Need {
        /// What is needed.
        what: NeedKind,
        /// What to tell the person, in the session's language.
        message: String,
        /// For [`NeedKind::File`]: direct link to the export page.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        url: Option<String>,
    },
    /// The connector has sent everything.
    Done {
        /// Counts of what was sent.
        summary: DoneSummary,
    },
    /// Something went wrong.
    Error {
        /// What kind of failure, which decides whether and when to retry.
        code: ErrorCode,
        /// For [`ErrorCode::RateLimited`]: how long to wait, e.g. `10m`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        retry_after: Option<String>,
        /// What to tell the person, in the session's language, without personal data.
        message: String,
    },
    /// A `watch` connector is still running and has nothing to send.
    Alive,
}

/// What a connector needs from the person.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NeedKind {
    /// Access expired or was revoked: reconnect.
    Reauth,
    /// A file export is needed.
    File,
    /// A setting is missing or wrong.
    Config,
}

/// Why a connector failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// The source asks to slow down.
    RateLimited,
    /// The source cannot be reached.
    Unavailable,
    /// Credentials are refused.
    Auth,
    /// The picked file or the source's data cannot be read.
    BadInput,
    /// A message type the connector does not support.
    Unsupported,
    /// A protocol version the connector does not support.
    UnsupportedProtocol,
    /// A bug in the connector.
    Internal,
}

/// Counts reported in `done`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DoneSummary {
    /// Records with `op: upsert`.
    #[serde(default)]
    pub upserts: u64,
    /// Records with `op: delete`.
    #[serde(default)]
    pub deletes: u64,
    /// Records with `op: observe`.
    #[serde(default)]
    pub observes: u64,
}
