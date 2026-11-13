//! Running one import session with a connector process.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read as _, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use ipse_protocol::{
    ConnectorMessage, CoreMessage, DoneSummary, ErrorCode, MAX_BATCH_RECORDS, MAX_MESSAGE_BYTES,
    Mode, NeedKind, PROTOCOL_VERSION, Record, from_line, to_line,
};
use serde_json::{Map, Value};

use crate::journal::Journal;
use crate::{Error, Result};

/// Lines of the connector's standard error kept for diagnosis.
const STDERR_LINES: usize = 1_000;

/// Environment variables a connector inherits; everything else is cleared.
const INHERITED_ENV: [&str; 3] = ["PATH", "HOME", "LANG"];

/// Windows cannot start most programs without these.
#[cfg(windows)]
const WINDOWS_ENV: [&str; 7] = [
    "SYSTEMROOT",
    "WINDIR",
    "TEMP",
    "TMP",
    "USERPROFILE",
    "APPDATA",
    "LOCALAPPDATA",
];

/// How to start a connector for one import.
#[derive(Debug, Clone)]
pub struct Launch {
    /// The program and its arguments.
    pub command: Vec<String>,
    /// The instance, which is also the journal source: `contacts`, `contacts-2`.
    pub instance: String,
    /// The vault's language, for messages shown to the person.
    pub language: String,
    /// The vault's IANA time zone.
    pub time_zone: String,
    /// The files the person picked.
    pub files: Vec<PathBuf>,
    /// Record kinds the connector may send; others are rejected. `None` accepts any.
    pub kinds: Option<Vec<String>>,
    /// How long the connector has to answer `hello` (10 s in the spec).
    pub ready_timeout: Duration,
    /// How long the connector has to exit after `bye` (5 s in the spec).
    pub bye_timeout: Duration,
    /// How long the connector may stay silent before it is stopped.
    pub idle_timeout: Duration,
}

impl Launch {
    /// A launch with the spec's timeouts.
    #[must_use]
    pub fn new(command: Vec<String>, instance: &str, language: &str, time_zone: &str) -> Self {
        Self {
            command,
            instance: instance.to_owned(),
            language: language.to_owned(),
            time_zone: time_zone.to_owned(),
            files: Vec::new(),
            kinds: None,
            ready_timeout: Duration::from_secs(10),
            bye_timeout: Duration::from_secs(5),
            idle_timeout: Duration::from_mins(5),
        }
    }
}

/// How a session ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The connector sent everything.
    Done(DoneSummary),
    /// The connector reported a failure, in words for the person.
    Failed {
        /// What kind of failure.
        code: ErrorCode,
        /// The connector's message.
        message: String,
        /// When to try again, for `rate_limited`.
        retry_after: Option<String>,
    },
    /// The connector needs the person.
    Needs {
        /// What it needs.
        what: NeedKind,
        /// The connector's message.
        message: String,
        /// For a file export: the export page.
        url: Option<String>,
    },
    /// The core stopped the connector.
    Stopped(StopReason),
}

/// Why the core stopped a connector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    /// No `ready` in time.
    NoReady,
    /// It broke the protocol (invalid JSON, unexpected message…).
    BrokeProtocol(&'static str),
    /// It ended before `done`.
    Ended,
    /// It stayed silent too long.
    Silent,
}

impl std::fmt::Display for StopReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoReady => f.write_str("the connector did not answer in time"),
            Self::BrokeProtocol(what) => write!(f, "the connector misbehaved: {what}"),
            Self::Ended => f.write_str("the connector stopped before finishing"),
            Self::Silent => f.write_str("the connector stayed silent too long"),
        }
    }
}

/// What a session did.
#[derive(Debug, Clone)]
pub struct Report {
    /// How it ended.
    pub outcome: Outcome,
    /// Records written to the journal.
    pub written: u64,
    /// Records rejected as malformed.
    pub rejected: u64,
    /// Progress messages, in order.
    pub progress: Vec<String>,
    /// The last lines the connector wrote to its standard error.
    pub diagnostics: Vec<String>,
}

/// Runs a connector in import mode and writes what it sends to `journal`.
///
/// # Errors
///
/// [`Error::Io`] when the connector cannot be started; journal errors when a batch
/// cannot be written (the connector is then stopped). Everything the connector does
/// wrong is reported in the [`Report`], not as an error.
pub fn run_import(journal: &mut Journal, launch: &Launch) -> Result<Report> {
    let (program, args) = launch
        .command
        .split_first()
        .ok_or_else(|| Error::Refused("A connector needs a command to start.".to_owned()))?;
    let mut command = Command::new(program);
    command
        .args(args)
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for key in INHERITED_ENV {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    #[cfg(windows)]
    for key in WINDOWS_ENV {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    let mut child = command.spawn().map_err(|source| Error::Io {
        action: "start",
        path: PathBuf::from(program),
        source,
    })?;

    let stderr = Arc::new(Mutex::new(VecDeque::with_capacity(64)));
    if let Some(pipe) = child.stderr.take() {
        let lines = Arc::clone(&stderr);
        thread::spawn(move || keep_last_lines(pipe, &lines));
    }
    let messages = child.stdout.take().map(spawn_reader);
    let stdin = child.stdin.take();

    let mut session = Host {
        child,
        stdin,
        messages,
        written: 0,
        rejected: 0,
        progress: Vec::new(),
    };
    let outcome = session.run(journal, launch);
    session.stop(launch.bye_timeout);

    let diagnostics = stderr
        .lock()
        .map(|lines| lines.iter().cloned().collect())
        .unwrap_or_default();
    Ok(Report {
        outcome: outcome?,
        written: session.written,
        rejected: session.rejected,
        progress: session.progress,
        diagnostics,
    })
}

/// One line read from the connector.
enum Line {
    Message(Vec<u8>),
    TooLong,
    Closed,
}

struct Host {
    child: Child,
    stdin: Option<ChildStdin>,
    messages: Option<Receiver<Line>>,
    written: u64,
    rejected: u64,
    progress: Vec<String>,
}

impl Host {
    fn run(&mut self, journal: &mut Journal, launch: &Launch) -> Result<Outcome> {
        let hello = CoreMessage::Hello {
            protocol: PROTOCOL_VERSION,
            instance: launch.instance.clone(),
            mode: Mode::Import,
            language: launch.language.clone(),
            time_zone: launch.time_zone.clone(),
            cursor: journal.cursor(&launch.instance)?,
            config: Map::new(),
            secrets: std::collections::BTreeMap::new(),
            files: launch.files.clone(),
        };
        if !self.send(&hello) {
            return Ok(Outcome::Stopped(StopReason::Ended));
        }

        match self.next(launch.ready_timeout) {
            Ok(ConnectorMessage::Ready { protocol, .. }) if protocol == PROTOCOL_VERSION => {}
            Ok(ConnectorMessage::Error {
                code,
                message,
                retry_after,
            }) => {
                return Ok(Outcome::Failed {
                    code,
                    message,
                    retry_after,
                });
            }
            Ok(_) => {
                return Ok(Outcome::Stopped(StopReason::BrokeProtocol(
                    "the first message must be `ready` with protocol 1",
                )));
            }
            Err(StopReason::Silent) => return Ok(Outcome::Stopped(StopReason::NoReady)),
            Err(reason) => return Ok(Outcome::Stopped(reason)),
        }
        if !self.send(&CoreMessage::Start { full: false }) {
            return Ok(Outcome::Stopped(StopReason::Ended));
        }

        let mut expected_seq = 1;
        loop {
            let message = match self.next(launch.idle_timeout) {
                Ok(message) => message,
                Err(reason) => return Ok(Outcome::Stopped(reason)),
            };
            match message {
                ConnectorMessage::Batch {
                    seq,
                    records,
                    cursor,
                } => {
                    if seq != expected_seq {
                        return Ok(Outcome::Stopped(StopReason::BrokeProtocol(
                            "batches must be numbered 1, 2, 3…",
                        )));
                    }
                    if records.len() > MAX_BATCH_RECORDS {
                        return Ok(Outcome::Stopped(StopReason::BrokeProtocol(
                            "a batch holds at most 1 000 records",
                        )));
                    }
                    expected_seq += 1;
                    let rejected = self.write_batch(journal, launch, records, cursor.as_ref())?;
                    if !self.send(&CoreMessage::Ack { seq, rejected }) {
                        return Ok(Outcome::Stopped(StopReason::Ended));
                    }
                }
                ConnectorMessage::Progress { message, .. } => {
                    self.progress.extend(message);
                }
                ConnectorMessage::Done { summary } => return Ok(Outcome::Done(summary)),
                ConnectorMessage::Error {
                    code,
                    message,
                    retry_after,
                } => {
                    return Ok(Outcome::Failed {
                        code,
                        message,
                        retry_after,
                    });
                }
                ConnectorMessage::Need { what, message, url } => {
                    return Ok(Outcome::Needs { what, message, url });
                }
                ConnectorMessage::Alive => {}
                ConnectorMessage::Ready { .. } => {
                    return Ok(Outcome::Stopped(StopReason::BrokeProtocol(
                        "`ready` was sent twice",
                    )));
                }
            }
        }
    }

    /// Writes the valid records and returns the positions of the others.
    fn write_batch(
        &mut self,
        journal: &mut Journal,
        launch: &Launch,
        records: Vec<Record>,
        cursor: Option<&Value>,
    ) -> Result<Vec<usize>> {
        let mut rejected = Vec::new();
        let mut valid = Vec::with_capacity(records.len());
        for (i, record) in records.into_iter().enumerate() {
            let allowed = launch
                .kinds
                .as_ref()
                .is_none_or(|kinds| kinds.contains(&record.kind));
            if allowed && record.validate().is_ok() {
                valid.push(record);
            } else {
                rejected.push(i);
            }
        }
        let written = journal.append(&launch.instance, &valid, cursor)?;
        self.written += written.len() as u64;
        self.rejected += rejected.len() as u64;
        Ok(rejected)
    }

    /// The next message, or why there is none.
    fn next(&mut self, timeout: Duration) -> std::result::Result<ConnectorMessage, StopReason> {
        let Some(messages) = &self.messages else {
            return Err(StopReason::Ended);
        };
        match messages.recv_timeout(timeout) {
            Ok(Line::Message(bytes)) => from_line(&bytes).map_err(|_| {
                StopReason::BrokeProtocol("it sent something that is not a protocol message")
            }),
            Ok(Line::TooLong) => Err(StopReason::BrokeProtocol("a message is larger than 8 MiB")),
            Ok(Line::Closed) | Err(RecvTimeoutError::Disconnected) => Err(StopReason::Ended),
            Err(RecvTimeoutError::Timeout) => Err(StopReason::Silent),
        }
    }

    /// Sends a message; `false` when the connector is gone.
    fn send(&mut self, message: &CoreMessage) -> bool {
        let Some(stdin) = &mut self.stdin else {
            return false;
        };
        let Ok(line) = to_line(message) else {
            return false;
        };
        stdin.write_all(&line).and_then(|()| stdin.flush()).is_ok()
    }

    /// Says `bye`, waits up to `timeout`, then stops the connector if it is still there.
    fn stop(&mut self, timeout: Duration) {
        let _ = self.send(&CoreMessage::Bye);
        self.stdin = None;
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if matches!(self.child.try_wait(), Ok(Some(_))) {
                return;
            }
            thread::sleep(Duration::from_millis(20));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Reads the connector's standard output line by line on its own thread, so that the
/// session can wait for a message with a timeout.
fn spawn_reader(stdout: std::process::ChildStdout) -> Receiver<Line> {
    let (sender, receiver) = mpsc::sync_channel(8);
    thread::spawn(move || {
        let mut reader = BufReader::with_capacity(64 * 1024, stdout);
        loop {
            let mut line = Vec::new();
            let limit = u64::try_from(MAX_MESSAGE_BYTES).unwrap_or(u64::MAX);
            let read = (&mut reader).take(limit + 1).read_until(b'\n', &mut line);
            let item = match read {
                Ok(0) | Err(_) => Line::Closed,
                Ok(_) if line.len() > MAX_MESSAGE_BYTES => Line::TooLong,
                Ok(_) if line.iter().all(u8::is_ascii_whitespace) => continue,
                Ok(_) => Line::Message(line),
            };
            let last = !matches!(item, Line::Message(_));
            if sender.send(item).is_err() || last {
                break;
            }
        }
    });
    receiver
}

/// Keeps the last lines of the connector's standard error.
fn keep_last_lines(pipe: std::process::ChildStderr, lines: &Mutex<VecDeque<String>>) {
    let reader = BufReader::new(pipe);
    for line in reader.split(b'\n') {
        let Ok(line) = line else { break };
        let Ok(mut kept) = lines.lock() else { break };
        if kept.len() == STDERR_LINES {
            kept.pop_front();
        }
        kept.push_back(String::from_utf8_lossy(&line).trim_end().to_owned());
    }
}
