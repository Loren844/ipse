//! Core library of Ipse.
//!
//! Ipse keeps a person's memory (facts, memories, habits, data from their services)
//! in an open, local vault, and exposes it to any AI through MCP with per-category rights.
//!
//! This crate is at its very beginning: the vault format, the raw journal, the index
//! and the rights model are specified in `docs/` before they are implemented here.

pub mod slug;
