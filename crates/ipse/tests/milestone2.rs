//! Milestone 2 acceptance (issue #84): the person asks their AI to tidy up their
//! memory; the AI goes through its tasks and proposes (a merge, facts from notes, the
//! full name of someone known by a nickname, the summary of a day); the person accepts,
//! refuses with a comment, undoes; nothing is written behind their back (decision 0005).
//!
//! Every step runs the `ipse` program as the person and their AI app do. The person's
//! decisions go through `ipse proposals`, the same path as the review inbox, whose
//! clicks `ui/tests/milestone2.spec.ts` replays in a browser. The guides
//! `docs/guide/tidy-up.md` and `docs/guide/review.md` walk the same steps. The
//! scenario's time is printed (`cargo test --test milestone2 -- --nocapture`).

// Test helpers may panic: a failed expectation is a failed test.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::print_stdout)]

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Instant;

use ipse_core::vault::{History, Source};
use rmcp::RoleClient;
use rmcp::ServiceExt;
use rmcp::model::CallToolRequestParams;
use rmcp::service::RunningService;
use rmcp::transport::TokioChildProcess;
use serde_json::{Value, json};

const IPSE: &str = env!("CARGO_BIN_EXE_ipse");

/// Runs `ipse` with `args` and returns what it printed; fails the test if it failed.
fn ipse(args: &[&str]) -> String {
    let out = Command::new(IPSE).args(args).output().unwrap();
    assert!(
        out.status.success(),
        "ipse {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

/// The access code `ipse client add` shows.
fn access_code(printed: &str) -> String {
    printed
        .split_whitespace()
        .find(|w| w.starts_with("ipse_"))
        .expect("an access code is shown")
        .to_owned()
}

/// Starts `ipse mcp` the way the AI app does.
async fn connect(vault: &Path, token: &str) -> RunningService<RoleClient, ()> {
    let mut command = tokio::process::Command::new(IPSE);
    command
        .arg("mcp")
        .arg("--vault")
        .arg(vault)
        .env("IPSE_TOKEN", token);
    let (transport, _) = TokioChildProcess::builder(command)
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    ().serve(transport).await.unwrap()
}

/// Calls `tool`: its JSON answer, or the message of its refusal.
async fn call(
    ai: &RunningService<RoleClient, ()>,
    tool: &str,
    args: Value,
) -> Result<Value, String> {
    let Value::Object(args) = args else {
        panic!("arguments are an object")
    };
    let result = ai
        .call_tool(CallToolRequestParams::new(tool.to_owned()).with_arguments(args))
        .await
        .unwrap();
    if result.is_error == Some(true) {
        let text = result
            .content
            .iter()
            .filter_map(|c| c.as_text().map(|t| t.text.clone()))
            .collect::<String>();
        return Err(text);
    }
    Ok(result.structured_content.unwrap())
}

async fn ask(ai: &RunningService<RoleClient, ()>, tool: &str, args: Value) -> Value {
    call(ai, tool, args)
        .await
        .unwrap_or_else(|e| panic!("{tool}: {e}"))
}

/// The id of the open task of `kind`.
fn task_of(tasks: &Value, kind: &str) -> String {
    tasks["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["kind"] == kind)
        .unwrap_or_else(|| panic!("a {kind} task in {tasks}"))["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

/// A calendar export with a dinner yesterday, in the vault's time zone.
fn calendar(dir: &Path) -> (std::path::PathBuf, String) {
    let yesterday = jiff::Zoned::now()
        .in_tz("Europe/Paris")
        .unwrap()
        .date()
        .yesterday()
        .unwrap();
    let day = yesterday.strftime("%Y%m%d").to_string();
    let text = format!(
        "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Ipse tests//EN\r\nBEGIN:VEVENT\r\nUID:dinner-milestone2@example.org\r\nDTSTAMP:{day}T080000Z\r\nDTSTART:{day}T183000Z\r\nDTEND:{day}T203000Z\r\nSUMMARY:Dîner avec Élodie\r\nATTENDEE;CN=Élodie Petit:mailto:elodie.petit@example.org\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n"
    );
    let path = dir.join("agenda.ics");
    std::fs::write(&path, text).unwrap();
    (path, yesterday.to_string())
}

fn read(vault: &Path, file: &str) -> String {
    std::fs::read_to_string(vault.join(file)).unwrap()
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // one scenario, told in order
async fn the_ai_proposes_and_the_person_decides() {
    let started = Instant::now();
    let dir = tempfile::tempdir().unwrap();
    let vault = dir.path().join("Ipse");
    let v = vault.to_str().unwrap();
    let contacts =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/contacts/milestone2.vcf");
    let (agenda, yesterday) = calendar(dir.path());

    // 1. The memory, the contacts (two of them the same person, one known by a
    //    nickname, notes to read) and the calendar.
    ipse(&[
        "init",
        v,
        "--name",
        "Inès Roux",
        "--language",
        "fr",
        "--time-zone",
        "Europe/Paris",
    ]);
    let printed = ipse(&["import", contacts.to_str().unwrap(), "--vault", v]);
    assert!(printed.contains("6 people added"), "{printed}");
    assert!(printed.contains("1 possible duplicate"), "{printed}");
    ipse(&["import", agenda.to_str().unwrap(), "--vault", v]);
    let before = read(&vault, "people/elodie-petit.md");

    // 2. Two AIs: Claude may propose in people, agenda and summaries; the other reads.
    let claude = access_code(&ipse(&["client", "add", "Claude", "--vault", v]));
    ipse(&[
        "client",
        "set",
        "claude",
        "--propose",
        "people,agenda,summaries",
        "--vault",
        v,
    ]);
    let reader = access_code(&ipse(&["client", "add", "Lecteur", "--vault", v]));

    // 3. "Tidy up my memory": Claude reads its tasks and proposes.
    let ai = connect(&vault, &claude).await;
    let tasks = ask(&ai, "tasks", json!({})).await;
    let extract = task_of(&tasks, "extract");
    let question = task_of(&tasks, "question");
    let summary = task_of(&tasks, "summary");

    let detail = ask(&ai, "task", json!({ "id": extract })).await;
    let texts: Vec<&str> = detail["task"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|i| i["text"].as_str())
        .collect();
    assert!(
        texts.contains(&"Infirmière à Nantes. Sœur de Marc."),
        "{texts:?}"
    );
    let facts = ask(
        &ai,
        "propose",
        json!({
            "task": extract,
            "reason": "Lu dans les notes d'Élodie.",
            "changes": [
                {"kind": "add_fact", "file": "people/elodie-petit.md", "text": "Infirmière à Nantes."},
                {"kind": "add_relation", "file": "people/elodie-petit.md", "word": "sœur", "target": "marc-leroy"}
            ]
        }),
    )
    .await["proposal"]
        .as_str()
        .unwrap()
        .to_owned();
    // Bob's notes say who he is: a merge Ipse could not find on its own.
    let merge = ask(
        &ai,
        "propose",
        json!({
            "reason": "Ses notes disent que Bob, c'est Robert.",
            "changes": [{"kind": "merge", "keep": "people/robert-martin.md", "remove": "people/bob-martin.md", "clues": ["name"]}]
        }),
    )
    .await["proposal"]
        .as_str()
        .unwrap()
        .to_owned();

    let detail = ask(&ai, "task", json!({ "id": question })).await;
    assert_eq!(detail["task"]["items"][0]["name"], "Jo plombier");
    let named = ask(
        &ai,
        "propose",
        json!({
            "task": question,
            "reason": "Inès m'a dit que c'est Jo Lambert, son plombier.",
            "changes": [{"kind": "set_key", "file": "people/jo-plombier.md", "key": "name", "from": "Jo plombier", "to": "Jo Lambert"}]
        }),
    )
    .await["proposal"]
        .as_str()
        .unwrap()
        .to_owned();

    let detail = ask(&ai, "task", json!({ "id": summary })).await;
    assert!(
        detail["task"]["description"]
            .as_str()
            .unwrap()
            .contains(&yesterday),
        "{detail}"
    );
    let day = ask(
        &ai,
        "propose",
        json!({
            "task": summary,
            "reason": "Résumé d'hier.",
            "changes": [{"kind": "write_summary", "period": "day", "start": yesterday, "text": "Dîner avec Élodie."}]
        }),
    )
    .await["proposal"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        read(&vault, "people/elodie-petit.md"),
        before,
        "nothing written before the person decides"
    );

    // 4. The AI that may only read sees no task and can propose nothing.
    let other = connect(&vault, &reader).await;
    assert_eq!(
        ask(&other, "tasks", json!({})).await,
        json!({ "tasks": [], "more": false })
    );
    let refused = call(
        &other,
        "propose",
        json!({ "reason": "x", "changes": [{"kind": "add_fact", "file": "people/jo-plombier.md", "text": "x"}] }),
    )
    .await;
    assert_eq!(
        refused,
        Err("There is no file people/jo-plombier.md.".to_owned())
    );
    other.cancel().await.unwrap();

    // 5. The person reviews: Ipse's duplicate and three of Claude's proposals
    //    accepted, the facts refused with a comment, the merge of Bob undone.
    let waiting = ipse(&["proposals", "list", "--vault", v]);
    assert_eq!(waiting.lines().count(), 5, "{waiting}");
    let duplicate = waiting
        .lines()
        .find(|l| l.contains("Même adresse email"))
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .to_owned();
    for id in [&duplicate, &merge, &named, &day] {
        ipse(&["proposals", "accept", id, "--vault", v]);
    }
    ipse(&[
        "proposals",
        "refuse",
        &facts,
        "--comment",
        "Sage-femme, pas infirmière.",
        "--vault",
        v,
    ]);
    ipse(&["proposals", "undo", &merge, "--vault", v]);

    // 6. The files and the history say exactly that.
    assert_eq!(read(&vault, "people/elodie-petit.md"), before, "refused");
    let jo = read(&vault, "people/jo-plombier.md");
    assert!(
        jo.contains("name: Jo Lambert\n") && jo.contains("  - Jo plombier\n"),
        "{jo}"
    );
    let year = &yesterday[..4];
    let written = read(&vault, &format!("summaries/day/{year}/{yesterday}.md"));
    assert!(written.contains("Dîner avec Élodie."), "{written}");
    assert!(
        !vault.join("people/marc-leroy-pro.md").exists()
            || !vault.join("people/marc-leroy.md").exists(),
        "the two Marc are one"
    );
    assert!(vault.join("people/bob-martin.md").exists(), "undone");
    assert!(!read(&vault, "people/robert-martin.md").contains("tennis"));
    let history = History::open(&vault).unwrap();
    let last: Vec<(Option<Source>, String)> = history
        .entries(5)
        .unwrap()
        .into_iter()
        .map(|e| (e.source, e.summary))
        .collect();
    assert_eq!(
        last[0],
        (Some(Source::Person), "Undo a proposal".to_owned())
    );
    // Each accepted proposal is one change, named after it; nothing else. The list
    // shows Ipse's own proposal by the first characters of its id.
    for ((source, summary), id) in last[1..].iter().zip([&day, &named, &merge, &duplicate]) {
        assert_eq!(summary, "Accept a proposal");
        assert!(
            matches!(source, Some(Source::Proposal(p)) if p.starts_with(id.as_str())),
            "{source:?} for {id}"
        );
    }

    // 7. Claude sees the refusal, and the task open again with the comment.
    let status = ask(&ai, "proposal", json!({ "id": facts })).await;
    assert_eq!(status["proposal"]["status"], "refused");
    assert_eq!(status["proposal"]["comment"], "Sage-femme, pas infirmière.");
    let tasks = ask(&ai, "tasks", json!({ "kind": "extract" })).await;
    assert_eq!(task_of(&tasks, "extract"), extract);
    assert_eq!(tasks["tasks"][0]["comment"], "Sage-femme, pas infirmière.");
    ai.cancel().await.unwrap();

    println!("Milestone 2 scenario: {:.2?}", started.elapsed());
}
