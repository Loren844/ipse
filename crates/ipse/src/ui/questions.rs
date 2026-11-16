//! Questions in the review inbox: people known only by a first name or a nickname,
//! whom only the person can name (`docs/spec/vault.md`, section 13). The person
//! answers here or to their AI; either way the question is asked once.

use ipse_core::rights::Gateway;
use ipse_core::summaries;
use ipse_core::tasks::{Tasks, question};
use ipse_core::{Error, Result};
use serde::{Deserialize, Serialize};

/// An event the person attended with someone, to help recognize them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct SeenAt {
    title: String,
    /// `YYYY-MM-DD`.
    day: String,
}

/// One question: who is this?
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct QuestionView {
    /// The task, to answer it.
    id: String,
    /// The person's file id, to open their page.
    person: String,
    /// The name as saved: "Jo plombier".
    name: String,
    phones: Vec<String>,
    emails: Vec<String>,
    /// The most recent events with them.
    events: Vec<SeenAt>,
}

/// What `GET /api/questions` answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(
    test,
    derive(ts_rs::TS),
    ts(export, export_to = "../../../ui/src/lib/types/")
)]
pub(crate) struct QuestionsView {
    questions: Vec<QuestionView>,
}

/// `POST /api/questions/<id>/answer`.
#[derive(Debug, Deserialize)]
struct AnswerBody {
    name: String,
    /// How the person knows them: "plombier", "sœur".
    #[serde(default)]
    word: Option<String>,
}

/// The questions waiting, after making the ones due today.
///
/// # Errors
///
/// Errors reading the tasks or the index, or saving new tasks.
pub(crate) fn list(gateway: &mut Gateway) -> Result<QuestionsView> {
    plan(gateway)?;
    let vault = gateway.vault();
    let index = gateway.index();
    let tasks = Tasks::new(vault);
    let mut questions = Vec::new();
    for task in question::open(vault)? {
        // Gone, or named since: nothing to ask any more.
        let Some(known) =
            question::known(vault, index, &task).filter(|k| question::is_partial(&k.name))
        else {
            tasks.dismiss(&task.id)?;
            continue;
        };
        questions.push(QuestionView {
            id: task.id,
            person: known.id,
            name: known.name,
            phones: known.phones,
            emails: known.emails,
            events: known
                .events
                .into_iter()
                .map(|(title, day)| SeenAt { title, day })
                .collect(),
        });
    }
    Ok(QuestionsView { questions })
}

/// Makes the questions due today, from an index up to date with the files.
///
/// # Errors
///
/// Errors reading the tasks or the index, or saving new tasks.
pub(crate) fn plan(gateway: &mut Gateway) -> Result<()> {
    gateway.fresh_owner()?;
    let vault = gateway.vault();
    Tasks::new(vault).plan_questions(gateway.index(), summaries::today(vault))?;
    Ok(())
}

/// How many questions wait, for the inbox's count.
///
/// # Errors
///
/// Errors reading the tasks.
pub(crate) fn waiting(gateway: &Gateway) -> Result<usize> {
    Ok(question::open(gateway.vault())?.len())
}

/// `POST /api/questions/<id>/answer` with `{"name": …, "word": …}`, or
/// `POST /api/questions/<id>/skip`: answers the remaining questions. `None` for an
/// address that names no action.
///
/// # Errors
///
/// [`Error::Refused`] when the body cannot be read, the task is not an open question
/// or the name is empty; errors writing the files or the history.
pub(crate) fn decide(
    gateway: &mut Gateway,
    path: &str,
    body: &[u8],
) -> Result<Option<QuestionsView>> {
    let Some((id, action)) = path
        .strip_prefix("questions/")
        .and_then(|rest| rest.split_once('/'))
    else {
        return Ok(None);
    };
    match action {
        "answer" => {
            let body: AnswerBody = serde_json::from_slice(body)
                .map_err(|_| Error::Refused("The request could not be read.".to_owned()))?;
            question::answer(gateway.vault(), id, &body.name, body.word.as_deref())?;
        }
        "skip" => question::skip(gateway.vault(), id)?,
        _ => return Ok(None),
    }
    list(gateway).map(Some)
}
