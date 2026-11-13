//! Running connectors: the core's side of the connector protocol
//! (`docs/spec/connectors.md`, sections 4, 5 and 7).
//!
//! A connector is a separate program (decision 0002). The core starts it with a clean
//! environment, talks to it in JSON Lines on its standard input and output, writes each
//! batch to the journal, and acknowledges a batch only once it is on disk.

mod host;

pub use host::{Launch, Outcome, Report, StopReason, run_import};
