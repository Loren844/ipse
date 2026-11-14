//! Benchmark of the contacts connector on a 5 000-contact file, run as the core runs it:
//! a separate process reading the file and sending its records.
#![allow(missing_docs, clippy::unwrap_used)]

use std::fmt::Write as _;
use std::io::Write as _;
use std::path::Path;
use std::process::{Command, Stdio};

use criterion::{Criterion, Throughput, criterion_group, criterion_main};

const CONTACTS: usize = 5_000;

/// An export mixing vCard 3 (Google, Apple) and vCard 2.1 (Outlook) cards.
fn contacts() -> String {
    let mut out = String::new();
    for i in 0..CONTACTS {
        if i % 5 == 0 {
            let _ = write!(
                out,
                "BEGIN:VCARD\r\nVERSION:2.1\r\nN;CHARSET=UTF-8;ENCODING=QUOTED-PRINTABLE:Num=C3=A9ro {i};Personne\r\n\
                 FN;CHARSET=UTF-8;ENCODING=QUOTED-PRINTABLE:Personne Num=C3=A9ro {i}\r\n\
                 TEL;WORK;VOICE:04 78 {:02} {:02} {:02}\r\n\
                 ADR;WORK;ENCODING=QUOTED-PRINTABLE;CHARSET=UTF-8:;;{i} Mont=C3=A9e de la Butte;Lyon;;69001;France\r\n\
                 NOTE;ENCODING=QUOTED-PRINTABLE;CHARSET=UTF-8:Rencontr=C3=A9e au club, un long commentaire qui conti=\r\n\
                 nue sur la ligne suivante.\r\nEND:VCARD\r\n",
                i % 100,
                (i / 100) % 100,
                i % 97,
            );
        } else {
            let _ = write!(
                out,
                "BEGIN:VCARD\r\nVERSION:3.0\r\nFN:Personne Numéro {i}\r\nN:Numéro {i};Personne;;;\r\n\
                 NICKNAME:Surnom {i}\r\nEMAIL;TYPE=INTERNET:personne{i}@example.org\r\n\
                 TEL;TYPE=CELL:06 {:02} {:02} {:02} {:02}\r\nBDAY:1990-{:02}-{:02}\r\n\
                 ORG:Boulangerie du Coin\r\nTITLE:Vendeuse\r\nUID:card-{i}\r\n\
                 item1.X-ABRELATEDNAMES:Personne Numéro {}\r\nitem1.X-ABLabel:_$!<Friend>!$_\r\n\
                 NOTE:Rencontrée au club d\\'escalade\\, le mardi.\r\nEND:VCARD\r\n",
                i % 100,
                (i / 100) % 100,
                i % 89,
                i % 83,
                i % 12 + 1,
                i % 28 + 1,
                (i + 1) % CONTACTS,
            );
        }
    }
    out
}

fn import(path: &Path) {
    let mut input = format!(
        "{{\"type\":\"hello\",\"protocol\":1,\"instance\":\"contacts\",\"mode\":\"import\",\"language\":\"en\",\"time_zone\":\"Europe/Paris\",\"cursor\":null,\"files\":[{}]}}\n{{\"type\":\"start\",\"full\":false}}\n",
        serde_json::to_string(path).unwrap()
    );
    for seq in 1..=10 {
        let _ = writeln!(input, "{{\"type\":\"ack\",\"seq\":{seq}}}");
    }
    input.push_str("{\"type\":\"bye\"}\n");
    let mut child = Command::new(env!("CARGO_BIN_EXE_ipse"))
        .args(["connector", "contacts"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    assert!(out.stdout.ends_with(b"\n"));
}

fn bench_contacts(c: &mut Criterion) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("contacts.vcf");
    std::fs::write(&path, contacts()).unwrap();

    let mut group = c.benchmark_group("contacts");
    group.sample_size(20);
    group.throughput(Throughput::Elements(CONTACTS as u64));
    group.bench_function("import_5000_contacts", |b| b.iter(|| import(&path)));
    group.finish();
}

criterion_group!(benches, bench_contacts);
criterion_main!(benches);
