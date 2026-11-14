//! `ipse proposals`: review what AIs propose, from the command line.

use std::fmt::Write as _;
use std::path::PathBuf;

use ipse_core::Result;
use ipse_core::proposals::{Accepted, DiffItem, Preview, Proposal, Proposals, Status, What};
use ipse_core::vault::Vault;

fn open(vault: Option<PathBuf>) -> Result<Vault> {
    Vault::open(&vault.unwrap_or_else(|| PathBuf::from(".")))
}

/// Lists the proposals waiting, or all of them.
pub(crate) fn list(all: bool, vault: Option<PathBuf>) -> Result<()> {
    let vault = open(vault)?;
    let proposals = Proposals::new(&vault).list()?;
    let shown: Vec<&Proposal> = proposals
        .iter()
        .filter(|p| all || matches!(p.status, Status::Waiting | Status::Stale { .. }))
        .collect();
    if shown.is_empty() {
        println!(
            "{}",
            if all {
                "No proposals yet."
            } else {
                "Nothing to review."
            }
        );
        return Ok(());
    }
    for p in shown {
        println!(
            "{}  {:<8}  {}  {}",
            short(&p.id),
            p.status.as_str(),
            String::from(p.by.clone()),
            first_line(&p.reason)
        );
    }
    Ok(())
}

/// Shows one proposal and what it would change now.
pub(crate) fn show(id: &str, vault: Option<PathBuf>) -> Result<()> {
    let vault = open(vault)?;
    let proposals = Proposals::new(&vault);
    let proposal = proposals.get(id)?;
    print!("{}", describe(&proposal, &proposals.preview(&proposal)?));
    Ok(())
}

/// Accepts one proposal.
pub(crate) fn accept(id: &str, vault: Option<PathBuf>) -> Result<()> {
    let vault = open(vault)?;
    match Proposals::new(&vault).accept(id)? {
        Accepted::Applied(_) => {
            println!("Accepted: your memory now says so. `ipse proposals undo` takes it back.");
        }
        Accepted::Stale(reason) => println!(
            "Not applied: {reason} Nothing was changed; this proposal can only be refused now."
        ),
    }
    Ok(())
}

/// Refuses one proposal.
pub(crate) fn refuse(id: &str, comment: Option<&str>, vault: Option<PathBuf>) -> Result<()> {
    let vault = open(vault)?;
    Proposals::new(&vault).refuse(id, comment)?;
    println!("Refused. Nothing was changed.");
    Ok(())
}

/// Undoes one accepted proposal.
pub(crate) fn undo(id: &str, vault: Option<PathBuf>) -> Result<()> {
    let vault = open(vault)?;
    Proposals::new(&vault).undo(id)?;
    println!("Undone: your files are back as they were before this proposal.");
    Ok(())
}

/// A proposal and its changes, in plain words.
pub(crate) fn describe(proposal: &Proposal, preview: &Preview) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "Proposal {} ({}), by {}, {}",
        short(&proposal.id),
        proposal.status.as_str(),
        String::from(proposal.by.clone()),
        proposal.created
    );
    let _ = writeln!(out, "Why: {}", proposal.reason);
    if let Status::Refused {
        comment: Some(comment),
        ..
    } = &proposal.status
    {
        let _ = writeln!(out, "You said: {comment}");
    }
    for file in &preview.files {
        let _ = writeln!(
            out,
            "\n{}{} ({})",
            if file.created { "New: " } else { "" },
            file.name,
            file.file
        );
        for item in &file.items {
            let _ = writeln!(out, "  {}", line(item));
        }
    }
    if let Some(reason) = &preview.stale {
        let _ = writeln!(out, "\nThis proposal no longer applies: {reason}");
    }
    out
}

fn line(item: &DiffItem) -> String {
    let what = match &item.what {
        What::Key(key) => key.clone(),
        What::Fact => "fact".to_owned(),
        What::Relation => "relation".to_owned(),
        What::Notes => "notes".to_owned(),
    };
    match (&item.before, &item.after) {
        (Some(before), Some(after)) => format!("{what}: {before} → {after}"),
        (None, Some(after)) => format!("+ {what}: {after}"),
        (Some(before), None) => format!("- {what}: {before}"),
        (None, None) => what,
    }
}

/// The first 8 characters of an id: enough to name a proposal.
fn short(id: &str) -> &str {
    id.get(..8).unwrap_or(id)
}

fn first_line(text: &str) -> String {
    let line = text.lines().next().unwrap_or_default();
    let mut out: String = line.chars().take(70).collect();
    if line.chars().count() > 70 {
        out.push('…');
    }
    out
}
