//! Core library of Ipse.
//!
//! Ipse keeps a person's memory (facts, memories, habits, data from their services)
//! in an open, local vault, and exposes it to any AI through MCP with per-category rights.
//!
//! The formats this crate implements are specified in `docs/spec/`:
//! the vault, the raw journal, the connector protocol and the rights model.

pub mod connector;
pub mod error;
pub mod journal;
pub mod slug;
pub mod time;
pub mod vault;

pub use error::{Error, Result};
