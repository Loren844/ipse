//! The connector's side of a protocol session (`docs/spec/connectors.md`, section 5).
//!
//! A built-in connector reads the core's messages on its standard input and writes its
//! own on its standard output. This type keeps the protocol's order and rules so that
//! each connector only produces records.

use std::collections::BTreeMap;
use std::fmt;
use std::io::{BufRead, Write};
use std::path::PathBuf;

use ipse_protocol::{
    ConnectorMessage, CoreMessage, DoneSummary, ErrorCode, Mode, PROTOCOL_VERSION, Record, Secret,
    from_line, to_line,
};
use serde_json::Value;

/// Why a session stopped early. Never holds personal data.
#[derive(Debug)]
pub(crate) enum SessionError {
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

pub(crate) type SessionResult<T> = Result<T, SessionError>;

/// What `hello` told the connector.
#[derive(Debug, Clone)]
pub(crate) struct Hello {
    pub(crate) mode: Mode,
    pub(crate) language: String,
    /// The vault's IANA time zone.
    pub(crate) time_zone: String,
    /// Where the last session stopped, as this connector returned it.
    pub(crate) cursor: Option<Value>,
    /// Secrets read from the keychain for this session only; never written anywhere.
    pub(crate) secrets: BTreeMap<String, Secret>,
    pub(crate) files: Vec<PathBuf>,
}

/// An open session with the core.
pub(crate) struct Session<R, W> {
    input: R,
    output: W,
    line: Vec<u8>,
    seq: u64,
}

impl<R: BufRead, W: Write> Session<R, W> {
    /// Reads `hello` and answers `ready`. Answers `unsupported_protocol` and stops when
    /// the core speaks another protocol version.
    pub(crate) fn open(
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
        };
        let hello = match session.read()? {
            Some(CoreMessage::Hello {
                protocol,
                mode,
                language,
                time_zone,
                cursor,
                secrets,
                files,
                ..
            }) => {
                if protocol != PROTOCOL_VERSION {
                    session.send(&ConnectorMessage::Error {
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
                    secrets,
                    files,
                }
            }
            _ => return Err(SessionError::Protocol("the first message must be `hello`")),
        };
        session.send(&ConnectorMessage::Ready {
            protocol: PROTOCOL_VERSION,
            connector: connector.to_owned(),
            version: version.to_owned(),
        })?;
        Ok((session, hello))
    }

    /// Waits for `start`. Returns whether a full run was asked for.
    pub(crate) fn wait_start(&mut self) -> SessionResult<bool> {
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

    /// Sends a batch and waits for its `ack`. Returns the positions the core rejected.
    pub(crate) fn batch(
        &mut self,
        records: Vec<Record>,
        cursor: Option<Value>,
    ) -> SessionResult<Vec<usize>> {
        self.seq += 1;
        let seq = self.seq;
        self.send(&ConnectorMessage::Batch {
            seq,
            records,
            cursor,
        })?;
        loop {
            match self.read()? {
                Some(CoreMessage::Ack {
                    seq: acked,
                    rejected,
                }) if acked == seq => {
                    return Ok(rejected);
                }
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
    pub(crate) fn progress(
        &mut self,
        done: u64,
        total: Option<u64>,
        message: Option<String>,
    ) -> SessionResult<()> {
        self.send(&ConnectorMessage::Progress {
            done,
            total,
            message,
        })
    }

    /// Reports a failure, in words for the person.
    pub(crate) fn error(&mut self, code: ErrorCode, message: String) -> SessionResult<()> {
        self.error_retry(code, None, message)
    }

    /// Reports a failure the source asks to retry after `retry_after`, like `10m`.
    pub(crate) fn error_retry(
        &mut self,
        code: ErrorCode,
        retry_after: Option<String>,
        message: String,
    ) -> SessionResult<()> {
        self.send(&ConnectorMessage::Error {
            code,
            retry_after,
            message,
        })
    }

    /// Says everything was sent.
    pub(crate) fn done(&mut self, summary: DoneSummary) -> SessionResult<()> {
        self.send(&ConnectorMessage::Done { summary })
    }

    /// Waits for `bye`, or for the core to close the session.
    pub(crate) fn finish(mut self) -> SessionResult<()> {
        while let Some(message) = self.read()? {
            if message == CoreMessage::Bye {
                break;
            }
        }
        Ok(())
    }

    fn send(&mut self, message: &ConnectorMessage) -> SessionResult<()> {
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

    #[test]
    fn hello_must_come_first() {
        let mut output = Vec::new();
        assert!(Session::open(&b"{\"type\":\"bye\"}\n"[..], &mut output, "x", "1").is_err());
    }
}
