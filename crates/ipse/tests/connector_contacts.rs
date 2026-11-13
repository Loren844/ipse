//! The contacts connector, run as the core runs it: a separate process speaking the
//! protocol on its standard input and output.

// Test helpers may panic: a failed expectation is a failed test.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::format_push_string)]

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::{Value, json};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/contacts")
        .join(name)
}

/// Runs a session with the given core messages and returns the connector's messages.
/// The core's answers are written in advance: the connector reads them in order.
fn session(files: &[PathBuf], acks: u64, language: &str) -> Vec<Value> {
    let mut input = String::new();
    let hello = json!({
        "type": "hello", "protocol": 1, "instance": "contacts", "mode": "import",
        "language": language, "time_zone": "Europe/Paris", "cursor": null, "files": files,
    });
    input.push_str(&format!("{hello}\n{{\"type\":\"start\",\"full\":false}}\n"));
    for seq in 1..=acks {
        input.push_str(&format!("{{\"type\":\"ack\",\"seq\":{seq}}}\n"));
    }
    input.push_str("{\"type\":\"bye\"}\n");

    let mut child = Command::new(env!("CARGO_BIN_EXE_ipse"))
        .args(["connector", "contacts"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

fn types(messages: &[Value]) -> Vec<&str> {
    messages
        .iter()
        .map(|m| m["type"].as_str().unwrap())
        .collect()
}

fn records(messages: &[Value]) -> Vec<&Value> {
    messages
        .iter()
        .filter(|m| m["type"] == "batch")
        .flat_map(|m| m["records"].as_array().unwrap())
        .collect()
}

#[test]
fn imports_google_and_apple_exports() {
    let files = [fixture("google-v3.vcf"), fixture("apple-v4.vcf")];
    let messages = session(&files, 1, "fr");
    assert_eq!(types(&messages), ["ready", "batch", "progress", "done"]);
    assert_eq!(messages[0]["connector"], "contacts");

    let records = records(&messages);
    let names: Vec<_> = records
        .iter()
        .map(|r| r["data"]["full_name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        ["Élodie Martin", "Marc Leroy", "Ana Silva", "Camille Durand"]
    );
    for r in &records {
        assert_eq!(r["kind"], "contact.card");
        assert_eq!(r["op"], "upsert");
    }

    let elodie = &records[0];
    assert_eq!(
        elodie["external_id"],
        "3f6c1a2e-0b5d-4e8f-9a17-2c4d6e8f0a1b"
    );
    assert_eq!(elodie["data"]["birthday"], "1990-04-02");
    assert_eq!(
        elodie["data"]["note"],
        "Rencontrée au club d'escalade, le mardi."
    );

    let marc = &records[1];
    assert_eq!(
        marc["data"]["emails"],
        json!(["marc.leroy@example.org", "m.leroy@example.com"])
    );
    assert!(
        marc["data"]["note"]
            .as_str()
            .unwrap()
            .contains("plusieurs lignes"),
        "folded"
    );
    assert!(
        marc["external_id"].as_str().unwrap().starts_with("b3:"),
        "no UID: hash"
    );

    let ana = &records[2];
    assert_eq!(
        ana["data"]["birthday"], "--07-21",
        "Apple's stand-in year dropped"
    );
    assert_eq!(ana["data"]["phones"], json!(["+351-912-345-678"]));
    assert_eq!(ana["data"]["nicknames"], json!(["Aninha", "Ana S."]));
    assert_eq!(
        ana["external_id"],
        "urn:uuid:7d1e2f30-4a5b-4c6d-8e9f-0a1b2c3d4e5f"
    );

    assert_eq!(
        messages[2]["message"],
        "1 fiche(s) illisible(s) ignorée(s)."
    );
    assert_eq!(messages[3]["summary"]["upserts"], 4);
}

#[test]
fn same_file_gives_same_ids() {
    let files = [fixture("google-v3.vcf")];
    let ids = |messages: Vec<Value>| -> Vec<Value> {
        records(&messages)
            .into_iter()
            .map(|r| r["external_id"].clone())
            .collect()
    };
    assert_eq!(ids(session(&files, 1, "en")), ids(session(&files, 1, "en")));
}

#[test]
fn large_files_go_in_batches_of_1000() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("many.vcf");
    let mut text = String::new();
    for i in 0..2_500 {
        text.push_str(&format!(
            "BEGIN:VCARD\nVERSION:4.0\nFN:Personne {i}\nUID:p-{i}\nEND:VCARD\n"
        ));
    }
    std::fs::write(&path, text).unwrap();

    let messages = session(&[path], 3, "en");
    let sizes: Vec<_> = messages
        .iter()
        .filter(|m| m["type"] == "batch")
        .map(|m| {
            (
                m["seq"].as_u64().unwrap(),
                m["records"].as_array().unwrap().len(),
            )
        })
        .collect();
    assert_eq!(sizes, [(1, 1000), (2, 1000), (3, 500)]);
    assert_eq!(messages.last().unwrap()["summary"]["upserts"], 2500);
}

#[test]
fn a_missing_file_is_reported_in_plain_words() {
    let messages = session(&[PathBuf::from("/no/such/contacts.vcf")], 0, "en");
    assert_eq!(types(&messages), ["ready", "error"]);
    assert_eq!(messages[1]["code"], "bad_input");
    let message = messages[1]["message"].as_str().unwrap();
    assert!(
        message.contains(".vcf") && !message.contains("/no/such"),
        "{message}"
    );
}

#[test]
fn an_empty_file_sends_nothing_but_done() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("empty.vcf");
    std::fs::write(&path, "").unwrap();
    let messages = session(&[path], 0, "en");
    assert_eq!(types(&messages), ["ready", "done"]);
}

#[test]
fn the_core_runs_it_and_fills_the_journal() {
    use ipse_core::connector::{Launch, Outcome, run_session};
    use ipse_core::journal::Journal;

    let vault = tempfile::tempdir().unwrap();
    let mut journal = Journal::new(vault.path());
    let mut launch = Launch::new(
        vec![
            env!("CARGO_BIN_EXE_ipse").into(),
            "connector".into(),
            "contacts".into(),
        ],
        "contacts",
        "fr",
        "Europe/Paris",
    );
    launch.files = vec![fixture("google-v3.vcf"), fixture("apple-v4.vcf")];
    launch.kinds = Some(vec!["contact.card".into()]);

    let report = run_session(&mut journal, &launch).unwrap();
    assert!(
        matches!(report.outcome, Outcome::Done(s) if s.upserts == 4),
        "{report:?}"
    );
    assert_eq!((report.written, report.rejected), (4, 0));
    assert_eq!(report.progress, ["1 fiche(s) illisible(s) ignorée(s)."]);

    let names: Vec<String> = journal
        .read("contacts", Some(&["contact.card"]))
        .unwrap()
        .map(|r| {
            r.unwrap().data.unwrap()["full_name"]
                .as_str()
                .unwrap()
                .to_owned()
        })
        .collect();
    assert_eq!(
        names,
        ["Élodie Martin", "Marc Leroy", "Ana Silva", "Camille Durand"]
    );
}
