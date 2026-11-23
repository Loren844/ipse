//! The `ipse` command the app ships with, and the server it runs for the window.

use std::io::{BufRead as _, BufReader, Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use serde::Deserialize;

/// How long `ipse ui` may take to say where it listens. Opening a memory reads its
/// index; a large one takes a few seconds on a slow disk.
const STARTING: Duration = Duration::from_mins(1);

/// The `ipse` command: next to the app's own program, where the installers put it;
/// `IPSE_BIN` when set (development, tests); in a development build, the workspace's
/// own build.
pub(crate) fn program() -> Option<PathBuf> {
    let name = format!("ipse{}", std::env::consts::EXE_SUFFIX);
    let beside = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join(&name)));
    let candidates = beside
        .into_iter()
        .chain(std::env::var_os("IPSE_BIN").map(PathBuf::from))
        .chain(cfg!(debug_assertions).then(|| {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../target/debug")
                .join(&name)
        }));
    candidates.into_iter().find(|path| path.is_file())
}

/// A command for `ipse`, without a console window on Windows.
pub(crate) fn command(program: &Path) -> Command {
    #[allow(unused_mut)] // changed on Windows only
    let mut command = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        /// `CREATE_NO_WINDOW`: no console flashes open behind the app.
        const NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(NO_WINDOW);
    }
    command
}

/// `ipse ui` serving one memory to the window.
pub(crate) struct Server {
    child: Child,
    /// Held open while the server runs: `ipse ui --app` stops when it closes.
    input: Option<ChildStdin>,
    /// The port it listens on, on `127.0.0.1`.
    pub(crate) port: u16,
}

/// What `ipse ui --app` tells the app while it runs, one JSON object per line after
/// the address (`crates/ipse/src/ui/background.rs`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub(crate) enum Event {
    /// More proposals and questions wait for the person than before.
    Waiting {
        /// How many more.
        new: usize,
    },
    /// A followed source failed to sync.
    SyncFailed {
        /// The name the person gave it.
        source: String,
    },
}

impl Event {
    /// The event on one line of `ipse`'s output; `None` for anything else, such as an
    /// event of a later version.
    pub(crate) fn read(line: &str) -> Option<Event> {
        serde_json::from_str(line.trim()).ok()
    }
}

/// Why a memory could not be opened.
#[derive(Debug)]
pub(crate) enum Failure {
    /// The `ipse` command is not where the app looks for it.
    Missing,
    /// `ipse` could not be started.
    Start(std::io::Error),
    /// `ipse` stopped or said nothing: what it wrote to its error output, in English.
    Said(String),
}

impl Server {
    /// Starts `ipse ui --app` on the memory in `folder`; returns it with the one-time
    /// address for the window. `told` receives each [`Event`] it sends afterwards, on
    /// a thread of its own.
    pub(crate) fn start(
        folder: &Path,
        told: impl Fn(Event) + Send + 'static,
    ) -> Result<(Server, String), Failure> {
        let program = program().ok_or(Failure::Missing)?;
        let mut child = command(&program)
            .arg("ui")
            .arg("--app")
            .arg("--vault")
            .arg(folder)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(Failure::Start)?;
        let input = child.stdin.take();
        let (Some(stdout), Some(stderr)) = (child.stdout.take(), child.stderr.take()) else {
            let _ = child.kill();
            return Err(Failure::Said(String::new()));
        };
        // The first line is the address; read on its own thread, so that a server that
        // hangs cannot freeze the app.
        let (sent, received) = mpsc::channel();
        std::thread::spawn(move || {
            let mut line = String::new();
            let mut lines = BufReader::new(stdout);
            let _ = sent.send(lines.read_line(&mut line).map(|_| line));
            // Then events, until the server stops.
            for line in lines.lines() {
                let Ok(line) = line else { break };
                if let Some(event) = Event::read(&line) {
                    told(event);
                }
            }
        });
        let address = received
            .recv_timeout(STARTING)
            .ok()
            .and_then(Result::ok)
            .map(|line| line.trim().to_owned())
            .filter(|line| line.starts_with("http://127.0.0.1:"));
        let Some(address) = address else {
            let _ = child.kill();
            let _ = child.wait();
            let mut said = String::new();
            let _ = BufReader::new(stderr)
                .take(16 * 1024)
                .read_to_string(&mut said);
            return Err(Failure::Said(said.trim().to_owned()));
        };
        std::thread::spawn(move || {
            let _ = std::io::copy(&mut BufReader::new(stderr), &mut std::io::sink());
        });
        let port = port_of(&address).ok_or_else(|| Failure::Said(String::new()))?;
        Ok((Server { child, input, port }, address))
    }

    /// Asks the server to sync every followed source now. Returns whether it could be
    /// asked.
    pub(crate) fn sync_now(&mut self) -> bool {
        self.input
            .as_mut()
            .is_some_and(|input| writeln!(input, "sync").and_then(|()| input.flush()).is_ok())
    }

    /// Stops the server and waits for it.
    pub(crate) fn stop(mut self) {
        drop(self.input.take());
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The port of `http://127.0.0.1:<port>/…`.
fn port_of(address: &str) -> Option<u16> {
    let rest = address.strip_prefix("http://127.0.0.1:")?;
    let end = rest.find('/').unwrap_or(rest.len());
    rest[..end].parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_port_is_read_from_the_address() {
        assert_eq!(port_of("http://127.0.0.1:4100/login?code=ab"), Some(4100));
        assert_eq!(port_of("http://127.0.0.1:x/"), None);
        assert_eq!(port_of("http://example.org:80/"), None);
    }

    #[test]
    fn events_are_read_from_their_line() {
        assert_eq!(
            Event::read(r#"{"event":"waiting","count":4,"new":1}"#),
            Some(Event::Waiting { new: 1 })
        );
        assert_eq!(
            Event::read("{\"event\":\"sync_failed\",\"source\":\"Perso\"}\n"),
            Some(Event::SyncFailed {
                source: "Perso".to_owned()
            })
        );
        assert_eq!(Event::read(r#"{"event":"later","x":1}"#), None);
        assert_eq!(Event::read("http://127.0.0.1:4100/login?code=ab"), None);
    }
}
