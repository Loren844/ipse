//! The first steps on the home screen and the notes shown the first time a screen
//! opens (issue #172).
//!
//! The steps are read from the memory as it is, never ticked by hand: a source was
//! added, an AI was connected, an AI read the memory, the person answered a proposal of
//! an AI. What the person put away (the steps, a screen's note) is kept in
//! `.ipse/guide.json`, which is not memory: it stays out of history.

use std::path::Path;

use ipse_core::connector::{Instances, runs};
use ipse_core::proposals::{Author, Proposals, Status};
use ipse_core::rights::{Clients, Gateway, access_days};
use ipse_core::vault::write_atomic;
use ipse_core::{Error, Result};
use serde::{Deserialize, Serialize};

/// Where the guide's state is kept, from the vault's root.
pub(crate) const FILE: &str = ".ipse/guide.json";

/// The screens that have a note the first time they open.
const SCREENS: [&str; 6] = ["sources", "ais", "review", "agenda", "map", "memory"];

/// What `.ipse/guide.json` holds.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct Saved {
    /// An invented memory, made by `ipse init --demo`.
    #[serde(default)]
    demo: bool,
    /// The person put the first steps away.
    #[serde(default)]
    hidden: bool,
    /// The screens whose note the person read.
    #[serde(default)]
    seen: Vec<String>,
}

impl Saved {
    /// The state as saved; none yet, or a file that cannot be read, is a fresh start.
    fn load(root: &Path) -> Self {
        std::fs::read(root.join(FILE))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    fn save(&self, root: &Path) -> Result<()> {
        let bytes = serde_json::to_vec_pretty(self).map_err(|e| Error::Refused(e.to_string()))?;
        write_atomic(&root.join(FILE), &bytes)
    }
}

/// One first step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct StepView {
    /// `sources`, `ai`, `question` or `review`.
    id: String,
    /// Done, as the memory shows it.
    done: bool,
}

/// The first steps, on the home screen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct GuideView {
    /// An invented memory: the home screen says so, and how to make one's own.
    demo: bool,
    /// In order.
    steps: Vec<StepView>,
}

/// The first steps to show on the home screen: none once the person put them away, or
/// once every step is done.
///
/// # Errors
///
/// Errors reading the sources, the connected AIs, the access log or the proposals.
pub(crate) fn home(gateway: &Gateway) -> Result<Option<GuideView>> {
    let root = gateway.vault().root();
    let saved = Saved::load(root);
    if saved.hidden {
        return Ok(None);
    }
    if saved.demo {
        return Ok(Some(GuideView {
            demo: true,
            steps: Vec::new(),
        }));
    }
    let sources = !Instances::load(root)?.list().is_empty() || !runs(root, 1)?.is_empty();
    let ai = !Clients::load(root)?.list().is_empty();
    let question = !access_days(root, "0000-01-01")?.is_empty();
    let review = Proposals::new(gateway.vault()).list()?.iter().any(|p| {
        matches!(p.by, Author::Client(_))
            && !p.automatic
            && !matches!(p.status, Status::Waiting | Status::Stale { .. })
    });
    let steps: Vec<StepView> = [
        ("sources", sources),
        ("ai", ai),
        ("question", question),
        ("review", review),
    ]
    .into_iter()
    .map(|(id, done)| StepView {
        id: id.to_owned(),
        done,
    })
    .collect();
    if steps.iter().all(|s| s.done) {
        return Ok(None);
    }
    Ok(Some(GuideView { demo: false, steps }))
}

/// The screens whose note the person read.
pub(crate) fn seen(root: &Path) -> Vec<String> {
    Saved::load(root).seen
}

/// Marks a memory as invented, for `ipse init --demo`.
///
/// # Errors
///
/// Errors writing `.ipse/guide.json`.
pub(crate) fn mark_demo(root: &Path) -> Result<()> {
    let mut saved = Saved::load(root);
    saved.demo = true;
    saved.save(root)
}

/// `POST /api/guide`: `{"hide": true}` puts the first steps away; `{"seen": "review"}`
/// says the person read that screen's note.
#[derive(Debug, Deserialize)]
struct ChangeBody {
    #[serde(default)]
    hide: bool,
    #[serde(default)]
    seen: Option<String>,
}

/// What `POST /api/guide` answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct NotesView {
    /// The screens whose note the person read.
    seen: Vec<String>,
}

/// Puts the first steps away, or a screen's note.
///
/// # Errors
///
/// [`Error::Refused`] when the body cannot be read or names no known screen; errors
/// writing `.ipse/guide.json`.
pub(crate) fn change(gateway: &Gateway, body: &[u8]) -> Result<NotesView> {
    let body: ChangeBody = serde_json::from_slice(body)
        .map_err(|_| Error::Refused("The request could not be read.".to_owned()))?;
    let root = gateway.vault().root();
    let mut saved = Saved::load(root);
    if body.hide {
        saved.hidden = true;
    }
    if let Some(screen) = body.seen {
        if !SCREENS.contains(&screen.as_str()) {
            return Err(Error::Refused("There is no such screen.".to_owned()));
        }
        if !saved.seen.contains(&screen) {
            saved.seen.push(screen);
        }
    }
    saved.save(root)?;
    Ok(NotesView { seen: saved.seen })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_or_broken_file_is_a_fresh_start() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(Saved::load(dir.path()), Saved::default());
        std::fs::create_dir_all(dir.path().join(".ipse")).unwrap();
        std::fs::write(dir.path().join(FILE), b"not json").unwrap();
        assert_eq!(Saved::load(dir.path()), Saved::default());
    }

    #[test]
    fn what_was_put_away_is_kept() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".ipse")).unwrap();
        mark_demo(dir.path()).unwrap();
        let mut saved = Saved::load(dir.path());
        assert!(saved.demo);
        saved.seen.push("review".to_owned());
        saved.save(dir.path()).unwrap();
        assert_eq!(seen(dir.path()), vec!["review".to_owned()]);
        assert!(Saved::load(dir.path()).demo);
    }
}
