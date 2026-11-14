//! The connector's side of a protocol session (`docs/spec/connectors.md`, sections 5
//! and 8): a helper to write a connector in Rust.
//!
//! A connector reads the core's messages on its standard input and writes its own on
//! its standard output. [`Session`] keeps the protocol's order and rules, so that a
//! connector only produces records:
//!
//! ```no_run
//! use std::io::{stdin, stdout};
//! use ipse_protocol::connector::Session;
//! use ipse_protocol::{DoneSummary, Record};
//!
//! # fn records() -> impl Iterator<Item = Record> { std::iter::empty() }
//! # fn main() -> ipse_protocol::connector::SessionResult<()> {
//! let (mut session, _hello) = Session::open(stdin().lock(), stdout().lock(), "example", "0.1.0")?;
//! session.wait_start()?;
//! let sent = session.send(records(), None)?;
//! session.done(DoneSummary { upserts: sent.accepted, ..DoneSummary::default() })?;
//! session.finish()
//! # }
//! ```
//!
//! Records go in batches of [`MAX_BATCH_RECORDS`], with up to
//! [`MAX_BATCHES_IN_FLIGHT`] batches sent before their acknowledgment, so that the
//! connector keeps reading its source while the core writes to disk.

use std::collections::{BTreeMap, VecDeque};
use std::fmt;
use std::io::{BufRead, Write};
use std::path::PathBuf;

use crate::{
    ConnectorMessage, CoreMessage, DoneSummary, ErrorCode, MAX_BATCH_RECORDS,
    MAX_BATCHES_IN_FLIGHT, Mode, PROTOCOL_VERSION, Record, Secret, from_line, to_line,
};
use serde_json::Value;

/// Why a session stopped early. Never holds personal data.
#[derive(Debug)]
#[non_exhaustive]
pub enum SessionError {
    /// Reading from the core or writing to it failed.
    Io(std::io::Error),
    /// The core broke the protocol.
    Protocol(&'static str),
}

impl fmt::Display for SessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "connector session: {e}"),
            Self::Protocol(what) => write!(f, "connector session: {what}"),
        }
    }
}

impl From<std::io::Error> for SessionError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

/// The result of a session step.
pub type SessionResult<T> = Result<T, SessionError>;

impl std::error::Error for SessionError {}

/// What the core kept of the records sent.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Sent {
    /// Records written to the journal.
    pub accepted: u64,
    /// Records the core rejected as malformed.
    pub rejected: u64,
}

/// What `hello` told the connector.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Hello {
    /// The mode asked for.
    pub mode: Mode,
    /// The language the person reads messages in.
    pub language: String,
    /// The vault's IANA time zone.
    pub time_zone: String,
    /// Where the last session stopped, as this connector returned it.
    pub cursor: Option<Value>,
    /// The instance's settings.
    pub config: serde_json::Map<String, Value>,
    /// Secrets read from the keychain for this session only; never written anywhere.
    pub secrets: BTreeMap<String, Secret>,
    /// Files the person picked, for `import` mode.
    pub files: Vec<PathBuf>,
}

/// An open session with the core.
pub struct Session<R, W> {
    input: R,
    output: W,
    line: Vec<u8>,
    seq: u64,
    /// Batches sent and not acknowledged yet: their `seq` and number of records.
    waiting: VecDeque<(u64, u64)>,
    sent: Sent,
}

impl<R: BufRead, W: Write> Session<R, W> {
    /// Reads `hello` and answers `ready`. Answers `unsupported_protocol` and stops when
    /// the core speaks another protocol version.
    ///
    /// # Errors
    ///
    /// [`SessionError::Protocol`] when the first message is not `hello` or another version
    /// is spoken; [`SessionError::Io`] when the core cannot be read or written.
    pub fn open(
        input: R,
        output: W,
        connector: &str,
        version: &str,
    ) -> SessionResult<(Self, Hello)> {
        let mut session = Self {
            input,
            output,
            line: Vec::with_capacity(4096),
            seq: 0,
            waiting: VecDeque::with_capacity(MAX_BATCHES_IN_FLIGHT),
            sent: Sent::default(),
        };
        let hello = match session.read()? {
            Some(CoreMessage::Hello {
                protocol,
                mode,
                language,
                time_zone,
                cursor,
                config,
                secrets,
                files,
                ..
            }) => {
                if protocol != PROTOCOL_VERSION {
                    session.send_message(&ConnectorMessage::Error {
                        code: ErrorCode::UnsupportedProtocol,
                        retry_after: None,
                        message: format!("This connector speaks protocol {PROTOCOL_VERSION}."),
                    })?;
                    return Err(SessionError::Protocol("unsupported protocol version"));
                }
                Hello {
                    mode,
                    language,
                    time_zone,
                    cursor,
                    config,
                    secrets,
                    files,
                }
            }
            _ => return Err(SessionError::Protocol("the first message must be `hello`")),
        };
        session.send_message(&ConnectorMessage::Ready {
            protocol: PROTOCOL_VERSION,
            connector: connector.to_owned(),
            version: version.to_owned(),
        })?;
        Ok((session, hello))
    }

    /// Waits for `start`. Returns whether a full run was asked for.
    ///
    /// # Errors
    ///
    /// [`SessionError`] when the core cannot be read or written, or breaks the
    /// protocol.
    pub fn wait_start(&mut self) -> SessionResult<bool> {
        loop {
            match self.read()? {
                Some(CoreMessage::Start { full }) => return Ok(full),
                Some(CoreMessage::Bye) | None => {
                    return Err(SessionError::Protocol("the core ended before `start`"));
                }
                Some(_) => {}
            }
        }
    }

    /// Sends `records` in batches, `cursor` with the last one, and waits until the core
    /// acknowledged them all. Returns what the core kept of them, with what earlier
    /// [`Session::push`] calls sent since the last wait.
    ///
    /// # Errors
    ///
    /// [`SessionError`] when the core cannot be written to or ends the session.
    pub fn send(
        &mut self,
        records: impl IntoIterator<Item = Record>,
        cursor: Option<Value>,
    ) -> SessionResult<Sent> {
        let mut records = records.into_iter().peekable();
        let mut batch = Vec::with_capacity(MAX_BATCH_RECORDS);
        while let Some(record) = records.next() {
            batch.push(record);
            // A full batch goes now, unless it is the last: the cursor goes with that one.
            if batch.len() == MAX_BATCH_RECORDS && records.peek().is_some() {
                self.push(
                    std::mem::replace(&mut batch, Vec::with_capacity(MAX_BATCH_RECORDS)),
                    None,
                )?;
            }
        }
        if !batch.is_empty() || cursor.is_some() {
            self.push(batch, cursor)?;
        }
        self.flush()
    }

    /// Sends one batch of at most [`MAX_BATCH_RECORDS`] records (more are split)
    /// without waiting for its `ack`, unless [`MAX_BATCHES_IN_FLIGHT`] batches already
    /// wait: then it waits for the oldest first.
    ///
    /// # Errors
    ///
    /// As [`Session::send`].
    pub fn push(
        &mut self,
        mut records: Vec<Record>,
        mut cursor: Option<Value>,
    ) -> SessionResult<()> {
        loop {
            let rest = records.split_off(records.len().min(MAX_BATCH_RECORDS));
            let last = rest.is_empty();
            while self.waiting.len() >= MAX_BATCHES_IN_FLIGHT {
                self.wait_oldest()?;
            }
            self.seq += 1;
            let count = records.len() as u64;
            self.send_message(&ConnectorMessage::Batch {
                seq: self.seq,
                records,
                cursor: if last { cursor.take() } else { None },
            })?;
            self.waiting.push_back((self.seq, count));
            if last {
                return Ok(());
            }
            records = rest;
        }
    }

    /// Waits until every batch sent is acknowledged. Returns what the core kept since
    /// the last wait.
    ///
    /// # Errors
    ///
    /// As [`Session::send`].
    pub fn flush(&mut self) -> SessionResult<Sent> {
        while !self.waiting.is_empty() {
            self.wait_oldest()?;
        }
        Ok(std::mem::take(&mut self.sent))
    }

    /// Sends a batch and waits for its `ack`, with every batch sent before it. Returns
    /// the positions the core rejected in this batch.
    ///
    /// # Errors
    ///
    /// As [`Session::send`].
    pub fn batch(
        &mut self,
        records: Vec<Record>,
        cursor: Option<Value>,
    ) -> SessionResult<Vec<usize>> {
        let records = records.into_iter().take(MAX_BATCH_RECORDS).collect();
        while !self.waiting.is_empty() {
            self.wait_oldest()?;
        }
        self.push(records, cursor)?;
        let seq = self.seq;
        self.waiting.pop_back();
        let rejected = self.wait_for(seq)?;
        Ok(rejected)
    }

    fn wait_oldest(&mut self) -> SessionResult<()> {
        let Some((seq, count)) = self.waiting.pop_front() else {
            return Ok(());
        };
        let rejected = self.wait_for(seq)?.len() as u64;
        self.sent.accepted += count.saturating_sub(rejected);
        self.sent.rejected += rejected;
        Ok(())
    }

    /// Reads until the `ack` of `seq`; returns its rejected positions.
    fn wait_for(&mut self, seq: u64) -> SessionResult<Vec<usize>> {
        loop {
            match self.read()? {
                Some(CoreMessage::Ack {
                    seq: acked,
                    rejected,
                }) if acked == seq => return Ok(rejected),
                Some(CoreMessage::Bye) | None => {
                    return Err(SessionError::Protocol(
                        "the core ended before acknowledging",
                    ));
                }
                Some(_) => {}
            }
        }
    }

    /// Tells the core how far the work is.
    ///
    /// # Errors
    ///
    /// [`SessionError`] when the core cannot be read or written, or breaks the
    /// protocol.
    pub fn progress(
        &mut self,
        done: u64,
        total: Option<u64>,
        message: Option<String>,
    ) -> SessionResult<()> {
        self.send_message(&ConnectorMessage::Progress {
            done,
            total,
            message,
        })
    }

    /// Reports a failure, in words for the person.
    ///
    /// # Errors
    ///
    /// [`SessionError`] when the core cannot be read or written, or breaks the
    /// protocol.
    pub fn error(&mut self, code: ErrorCode, message: String) -> SessionResult<()> {
        self.error_retry(code, None, message)
    }

    /// Reports a failure the source asks to retry after `retry_after`, like `10m`.
    ///
    /// # Errors
    ///
    /// [`SessionError`] when the core cannot be read or written, or breaks the
    /// protocol.
    pub fn error_retry(
        &mut self,
        code: ErrorCode,
        retry_after: Option<String>,
        message: String,
    ) -> SessionResult<()> {
        self.send_message(&ConnectorMessage::Error {
            code,
            retry_after,
            message,
        })
    }

    /// Waits for every batch to be acknowledged, then says everything was sent.
    ///
    /// # Errors
    ///
    /// As [`Session::send`].
    pub fn done(&mut self, summary: DoneSummary) -> SessionResult<()> {
        self.flush()?;
        self.send_message(&ConnectorMessage::Done { summary })
    }

    /// Waits for `bye`, or for the core to close the session.
    ///
    /// # Errors
    ///
    /// [`SessionError`] when the core cannot be read or written, or breaks the
    /// protocol.
    pub fn finish(mut self) -> SessionResult<()> {
        while let Some(message) = self.read()? {
            if message == CoreMessage::Bye {
                break;
            }
        }
        Ok(())
    }

    fn send_message(&mut self, message: &ConnectorMessage) -> SessionResult<()> {
        let line = to_line(message)
            .map_err(|_| SessionError::Protocol("a message could not be encoded"))?;
        self.output.write_all(&line)?;
        self.output.flush()?;
        Ok(())
    }

    /// The next message, or `None` when the core closed the session. A message of an
    /// unknown type is answered with `unsupported` and skipped.
    fn read(&mut self) -> SessionResult<Option<CoreMessage>> {
        loop {
            self.line.clear();
            if self.input.read_until(b'\n', &mut self.line)? == 0 {
                return Ok(None);
            }
            if self.line.iter().all(u8::is_ascii_whitespace) {
                continue;
            }
            if let Ok(message) = from_line::<CoreMessage>(&self.line) {
                return Ok(Some(message));
            }
            let has_type = serde_json::from_slice::<Value>(&self.line)
                .ok()
                .is_some_and(|v| v.get("type").is_some_and(Value::is_string));
            if !has_type {
                return Err(SessionError::Protocol("the core sent a malformed message"));
            }
            self.error(
                ErrorCode::Unsupported,
                "This connector does not know this message.".to_owned(),
            )?;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HELLO: &str = r#"{"type":"hello","protocol":1,"instance":"x","mode":"import","language":"fr","time_zone":"UTC","cursor":null}"#;

    fn run(input: &str, body: impl FnOnce(&mut Session<&[u8], &mut Vec<u8>>)) -> Vec<Value> {
        let mut output = Vec::new();
        let (mut session, _) = Session::open(input.as_bytes(), &mut output, "x", "1").unwrap();
        body(&mut session);
        drop(session);
        output
            .split(|b| *b == b'\n')
            .filter(|l| !l.is_empty())
            .map(|l| serde_json::from_slice(l).unwrap())
            .collect()
    }

    #[test]
    fn handshake_batch_and_done() {
        let input = format!(
            "{HELLO}\n{{\"type\":\"start\",\"full\":false}}\n{{\"type\":\"ack\",\"seq\":1,\"rejected\":[0]}}\n{{\"type\":\"bye\"}}\n"
        );
        let sent = run(&input, |s| {
            assert!(!s.wait_start().unwrap());
            assert_eq!(s.batch(vec![], None).unwrap(), [0]);
            s.done(DoneSummary::default()).unwrap();
        });
        let types: Vec<_> = sent.iter().map(|m| m["type"].as_str().unwrap()).collect();
        assert_eq!(types, ["ready", "batch", "done"]);
        assert_eq!(sent[1]["seq"], 1);
    }

    #[test]
    fn unknown_messages_are_answered_and_skipped() {
        let input =
            format!("{HELLO}\n{{\"type\":\"teleport\"}}\n{{\"type\":\"start\",\"full\":true}}\n");
        let sent = run(&input, |s| assert!(s.wait_start().unwrap()));
        assert_eq!(sent[1]["type"], "error");
        assert_eq!(sent[1]["code"], "unsupported");
    }

    #[test]
    fn other_protocol_versions_are_refused() {
        let input = HELLO.replace("\"protocol\":1", "\"protocol\":9");
        let mut output = Vec::new();
        assert!(Session::open(input.as_bytes(), &mut output, "x", "1").is_err());
        assert!(
            String::from_utf8(output)
                .unwrap()
                .contains("unsupported_protocol")
        );
    }

    /// A core that answers each `ack` only when the connector waits for it, and counts
    /// how many batches were ever waiting at once.
    #[derive(Default)]
    struct FakeCore {
        to_send: VecDeque<u8>,
        started: bool,
        sent: u64,
        acked: u64,
        most_waiting: u64,
        records: u64,
    }

    struct Input(std::rc::Rc<std::cell::RefCell<FakeCore>>);
    struct Output(std::rc::Rc<std::cell::RefCell<FakeCore>>);

    impl std::io::Read for Input {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let mut core = self.0.borrow_mut();
            if core.to_send.is_empty() {
                let line = if !core.started {
                    core.started = true;
                    format!("{HELLO}\n{{\"type\":\"start\",\"full\":false}}\n")
                } else if core.acked < core.sent {
                    core.acked += 1;
                    format!("{{\"type\":\"ack\",\"seq\":{}}}\n", core.acked)
                } else {
                    "{\"type\":\"bye\"}\n".to_owned()
                };
                core.to_send.extend(line.bytes());
            }
            let n = buf.len().min(core.to_send.len());
            for (i, b) in core.to_send.drain(..n).enumerate() {
                buf[i] = b;
            }
            Ok(n)
        }
    }

    impl Write for Output {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            let message: Value = serde_json::from_slice(buf).unwrap_or(Value::Null);
            if message["type"] == "batch" {
                let mut core = self.0.borrow_mut();
                core.sent += 1;
                core.records += message["records"].as_array().map_or(0, Vec::len) as u64;
                core.most_waiting = core.most_waiting.max(core.sent - core.acked);
            }
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn record(i: usize) -> Record {
        Record {
            kind: "x.item".into(),
            op: crate::Op::Upsert,
            external_id: Some(format!("item-{i}")),
            observed_at: jiff::Timestamp::UNIX_EPOCH,
            data: Some(serde_json::json!({ "i": i })),
        }
    }

    #[test]
    fn five_batches_with_at_most_four_waiting() {
        let core = std::rc::Rc::new(std::cell::RefCell::new(FakeCore::default()));
        let input = std::io::BufReader::new(Input(std::rc::Rc::clone(&core)));
        let (mut session, _) =
            Session::open(input, Output(std::rc::Rc::clone(&core)), "x", "1").unwrap();
        assert!(!session.wait_start().unwrap());
        let sent = session
            .send((0..5 * MAX_BATCH_RECORDS).map(record), None)
            .unwrap();
        assert_eq!(
            sent,
            Sent {
                accepted: 5000,
                rejected: 0
            }
        );
        session.done(DoneSummary::default()).unwrap();
        let core = core.borrow();
        assert_eq!((core.sent, core.acked, core.records), (5, 5, 5000));
        assert_eq!(
            core.most_waiting, MAX_BATCHES_IN_FLIGHT as u64,
            "four sent before the first ack, the fifth after it"
        );
    }

    #[test]
    fn a_cursor_goes_with_the_last_batch_even_when_empty() {
        let input = format!(
            "{HELLO}\n{{\"type\":\"start\",\"full\":false}}\n{{\"type\":\"ack\",\"seq\":1}}\n{{\"type\":\"ack\",\"seq\":2}}\n{{\"type\":\"bye\"}}\n"
        );
        let sent = run(&input, |s| {
            s.wait_start().unwrap();
            s.send(
                (0..MAX_BATCH_RECORDS).map(record),
                Some(serde_json::json!({"etag": "v1"})),
            )
            .unwrap();
            s.send(std::iter::empty(), Some(serde_json::json!({"etag": "v2"})))
                .unwrap();
        });
        assert_eq!(sent[1]["cursor"]["etag"], "v1");
        assert_eq!(sent[2]["records"], serde_json::json!([]));
        assert_eq!(sent[2]["cursor"]["etag"], "v2");
    }

    #[test]
    fn hello_must_come_first() {
        let mut output = Vec::new();
        assert!(Session::open(&b"{\"type\":\"bye\"}\n"[..], &mut output, "x", "1").is_err());
    }
}
