//! Running connectors: the core's side of the connector protocol
//! (`docs/spec/connectors.md`, sections 4, 5 and 7).
//!
//! A connector is a separate program (decision 0002). The core starts it with a clean
//! environment, talks to it in JSON Lines on its standard input and output, writes each
//! batch to the journal, and acknowledges a batch only once it is on disk.

mod host;
mod instances;

pub use host::{Launch, Outcome, Report, StopReason, run_session};
pub use instances::{
    INSTANCES_FILE, Instance, Instances, MemorySecrets, Run, STATE_FILE, SecretStore, SyncState,
    duration, secret_key,
};
