//! `ipse tasks`: what is left for the person's AI to do, for scripts and agents.

use std::path::PathBuf;

use ipse_core::Result;
use ipse_core::search::SearchIndex;
use ipse_core::summaries;
use ipse_core::tasks::{TaskStatus, Tasks};
use ipse_core::vault::Vault;

fn open(vault: Option<PathBuf>) -> Result<Vault> {
    Vault::open(&vault.unwrap_or_else(|| PathBuf::from(".")))
}

/// Lists the open tasks, or all of them, oldest first.
pub(crate) fn list(all: bool, vault: Option<PathBuf>) -> Result<()> {
    let vault = open(vault)?;
    // The days that ended since the last look are due now, as for an AI.
    let index = SearchIndex::load(vault.root())?;
    Tasks::new(&vault).plan_summaries(&index, summaries::today(&vault))?;
    let tasks = Tasks::new(&vault).list()?;
    let shown: Vec<_> = tasks
        .iter()
        .filter(|t| all || t.status == TaskStatus::Open)
        .collect();
    if shown.is_empty() {
        println!(
            "{}",
            if all {
                "No tasks yet."
            } else {
                "Nothing left to do."
            }
        );
        return Ok(());
    }
    for task in shown {
        println!(
            "{}  {:<8}  {:<8}  {}",
            super::proposals::short(&task.id),
            task.kind.as_str(),
            task.status.as_str(),
            task.description
        );
    }
    Ok(())
}

/// Shows one task in full: what is expected, its items, an example answer.
pub(crate) fn show(id: &str, vault: Option<PathBuf>) -> Result<()> {
    let vault = open(vault)?;
    let detail = Tasks::new(&vault).detail(id)?;
    let task = &detail.task;
    println!(
        "Task {} ({}, {}): {}",
        task.id,
        task.kind.as_str(),
        task.status.as_str(),
        task.description
    );
    if let Some(comment) = &task.comment {
        println!("Last answer refused: {comment}");
    }
    println!("\n{}", detail.expected);
    for item in &detail.items {
        println!("\n{} ({}, {})", item.name, item.file, item.category);
        match &item.text {
            Some(text) => {
                for line in text.lines() {
                    println!("  {line}");
                }
            }
            None => println!("  (no longer in the file)"),
        }
    }
    println!("\nExample of a good proposal:\n{}", detail.example);
    Ok(())
}
