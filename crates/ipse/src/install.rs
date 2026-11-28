//! Writing Ipse into an AI app's MCP configuration (issues #105 and #266), so that
//! connecting an AI is one command, or one button in the interface, instead of a block
//! to paste.
//!
//! Most apps keep their MCP servers in a JSON file, under a key of their own
//! (`mcpServers`, `servers`, `context_servers`); Goose keeps them in a YAML file, under
//! `extensions`. Ipse adds or replaces its own entry, `ipse`, and leaves everything else
//! as it was. The previous file is kept next to it (`<file>.before-ipse`). A file Ipse
//! cannot read is left alone, with a plain explanation; comments at the top of a JSON
//! file, as Zed writes them, are kept as they are. The access code is written in the
//! file and nowhere else: never printed, never logged.

use std::path::{Path, PathBuf};

use ipse_core::{Error, Result};
use serde_json::{Map, Value, json};

/// The name of Ipse's entry in an app's MCP servers.
pub(crate) const ENTRY: &str = "ipse";

/// An AI app whose MCP configuration Ipse can write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum App {
    /// Claude Desktop: `claude_desktop_config.json`.
    ClaudeDesktop,
    /// Claude Code, for every project: `~/.claude.json`.
    ClaudeCode,
    /// LM Studio, 0.3.17 or later: `~/.lmstudio/mcp.json`.
    LmStudio,
    /// Cursor, for every project: `~/.cursor/mcp.json`.
    Cursor,
    /// VS Code (Copilot in agent mode), for the user: `Code/User/mcp.json`.
    VsCode,
    /// Windsurf: `~/.codeium/windsurf/mcp_config.json`.
    Windsurf,
    /// Zed: its `settings.json`, under `context_servers`.
    Zed,
    /// Goose: `goose/config.yaml`, under `extensions`.
    Goose,
}

/// The system a configuration path is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum System {
    MacOs,
    Windows,
    Linux,
}

impl System {
    /// The system Ipse runs on.
    pub(crate) fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::MacOs
        } else if cfg!(windows) {
            Self::Windows
        } else {
            Self::Linux
        }
    }
}

/// Where the person's folders are: their home, and on Windows their roaming
/// application data (`%APPDATA%`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Places {
    pub(crate) home: PathBuf,
    pub(crate) app_data: Option<PathBuf>,
}

impl Places {
    /// The person's folders, from the environment.
    pub(crate) fn current() -> Option<Self> {
        Some(Self {
            home: std::env::home_dir()?,
            app_data: std::env::var_os("APPDATA").map(PathBuf::from),
        })
    }

    /// `%APPDATA%`, or where it usually is.
    fn roaming(&self) -> PathBuf {
        self.app_data
            .clone()
            .unwrap_or_else(|| self.home.join("AppData").join("Roaming"))
    }
}

/// How an app's file holds its MCP servers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Format {
    /// A JSON object whose key holds one entry per server.
    Json(&'static str),
    /// Goose's YAML, one entry per extension under `extensions`.
    GooseYaml,
}

impl App {
    /// Every app, in the order they are offered.
    pub(crate) const ALL: [Self; 8] = [
        Self::ClaudeDesktop,
        Self::ClaudeCode,
        Self::LmStudio,
        Self::Cursor,
        Self::VsCode,
        Self::Windsurf,
        Self::Zed,
        Self::Goose,
    ];

    /// Its name on the command line: `claude-desktop`, `vs-code`…
    pub(crate) fn id(self) -> &'static str {
        match self {
            Self::ClaudeDesktop => "claude-desktop",
            Self::ClaudeCode => "claude-code",
            Self::LmStudio => "lm-studio",
            Self::Cursor => "cursor",
            Self::VsCode => "vs-code",
            Self::Windsurf => "windsurf",
            Self::Zed => "zed",
            Self::Goose => "goose",
        }
    }

    /// Its name for people.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::ClaudeDesktop => "Claude Desktop",
            Self::ClaudeCode => "Claude Code",
            Self::LmStudio => "LM Studio",
            Self::Cursor => "Cursor",
            Self::VsCode => "VS Code",
            Self::Windsurf => "Windsurf",
            Self::Zed => "Zed",
            Self::Goose => "Goose",
        }
    }

    /// The ids of every app, as a sentence: `claude-desktop, claude-code… or goose`.
    pub(crate) fn ids() -> String {
        let ids: Vec<&str> = Self::ALL.iter().map(|a| a.id()).collect();
        let (last, rest) = ids.split_last().unwrap_or((&"", &[]));
        format!("{} or {last}", rest.join(", "))
    }

    /// The app named `text` on the command line.
    ///
    /// # Errors
    ///
    /// [`Error::Refused`] naming the apps Ipse knows.
    pub(crate) fn parse(text: &str) -> Result<Self> {
        Self::ALL
            .into_iter()
            .find(|a| a.id() == text.trim())
            .ok_or_else(|| {
                Error::Refused(format!(
                    "Ipse cannot set up `{}`: use {}. For another app, connect it by hand (docs/guide/connecting.md).",
                    text.trim(),
                    Self::ids()
                ))
            })
    }

    fn format(self) -> Format {
        match self {
            Self::VsCode => Format::Json("servers"),
            Self::Zed => Format::Json("context_servers"),
            Self::Goose => Format::GooseYaml,
            _ => Format::Json("mcpServers"),
        }
    }

    /// The file this app reads its MCP servers from, on `system`.
    pub(crate) fn config_path(self, system: System, places: &Places) -> PathBuf {
        let home = &places.home;
        // Where an app keeps its settings: `~/Library/Application Support` on macOS,
        // `%APPDATA%` on Windows, `~/.config` on Linux.
        let settings = match system {
            System::MacOs => home.join("Library").join("Application Support"),
            System::Windows => places.roaming(),
            System::Linux => home.join(".config"),
        };
        match self {
            Self::ClaudeDesktop => settings.join("Claude").join("claude_desktop_config.json"),
            Self::ClaudeCode => home.join(".claude.json"),
            Self::LmStudio => home.join(".lmstudio").join("mcp.json"),
            Self::Cursor => home.join(".cursor").join("mcp.json"),
            Self::VsCode => settings.join("Code").join("User").join("mcp.json"),
            Self::Windsurf => home
                .join(".codeium")
                .join("windsurf")
                .join("mcp_config.json"),
            Self::Zed => match system {
                System::Windows => places.roaming().join("Zed").join("settings.json"),
                _ => home.join(".config").join("zed").join("settings.json"),
            },
            Self::Goose => match system {
                System::Windows => places
                    .roaming()
                    .join("Block")
                    .join("goose")
                    .join("config")
                    .join("config.yaml"),
                _ => home.join(".config").join("goose").join("config.yaml"),
            },
        }
    }

    /// True when the app seems installed for this person: the folder of its
    /// configuration exists (or, for Claude Code, its file).
    pub(crate) fn found(self, system: System, places: &Places) -> bool {
        let path = self.config_path(system, places);
        match self {
            Self::ClaudeCode => path.is_file() || places.home.join(".claude").is_dir(),
            _ => path.parent().is_some_and(Path::is_dir),
        }
    }

    /// True for an app that runs models on this computer.
    pub(crate) fn is_local(self) -> bool {
        self == Self::LmStudio
    }

    /// What to do once the file is written, for the person.
    pub(crate) fn next_step(self) -> &'static str {
        match self {
            Self::ClaudeDesktop => {
                "Quit Claude Desktop completely and open it again: Ipse appears among its connectors."
            }
            Self::ClaudeCode => {
                "Start a new Claude Code session: `/mcp` lists Ipse in every project."
            }
            Self::LmStudio => {
                "LM Studio reloads its MCP servers: Ipse appears in the Program tab. Pick a model that supports tools."
            }
            Self::Cursor => {
                "In Cursor, open Settings, then MCP: Ipse is listed there, turn it on if it is off."
            }
            Self::VsCode => {
                "In VS Code, open Chat in Agent mode: Ipse is among the tools, and VS Code asks once to start it."
            }
            Self::Windsurf => {
                "In Windsurf, open Cascade's MCP servers and refresh them: Ipse is listed there."
            }
            Self::Zed => {
                "Zed reloads its settings: Ipse is listed in the Agent panel's settings, among the context servers."
            }
            Self::Goose => "Start a new Goose session: Ipse is among its extensions.",
        }
    }

    /// Ipse's entry for this app: it starts `ipse mcp` with the access code in its
    /// environment.
    fn entry(self, program: &Path, vault: &Path, token: &str) -> Value {
        let mut entry = json!({
            "command": plain(program),
            "args": ["mcp", "--vault", plain(vault)],
            "env": { "IPSE_TOKEN": token },
        });
        if let Some(map) = entry.as_object_mut() {
            match self {
                Self::ClaudeCode | Self::VsCode => {
                    map.insert("type".to_owned(), Value::from("stdio"));
                }
                Self::Zed => {
                    map.insert("source".to_owned(), Value::from("custom"));
                }
                _ => {}
            }
        }
        entry
    }

    /// What the person pastes in this app's file to add Ipse by hand, the access code
    /// filled in.
    pub(crate) fn snippet(self, program: &Path, vault: &Path, token: &str) -> String {
        match self.format() {
            Format::Json(key) => {
                let block = json!({ key: { ENTRY: self.entry(program, vault, token) } });
                serde_json::to_string_pretty(&block).unwrap_or_default()
            }
            Format::GooseYaml => format!("extensions:\n{}", goose_entry(program, vault, token)),
        }
    }
}

/// What any other app needs, to paste by hand: the usual `mcpServers` block.
pub(crate) fn any_app_snippet(program: &Path, vault: &Path, token: &str) -> String {
    App::ClaudeDesktop.snippet(program, vault, token)
}

/// The command any app starts, for one that asks for it rather than a file.
pub(crate) fn command(program: &Path, vault: &Path) -> String {
    format!("\"{}\" mcp --vault \"{}\"", plain(program), plain(vault))
}

/// A path as written in a configuration: without Windows' `\\?\` prefix.
fn plain(path: &Path) -> String {
    let text = path.to_string_lossy();
    text.strip_prefix(r"\\?\").unwrap_or(&text).to_owned()
}

/// What installing did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Installed {
    /// The file written.
    pub(crate) path: PathBuf,
    /// The copy of the file as it was, when there was one.
    pub(crate) backup: Option<PathBuf>,
    /// The access code of the entry it replaced, if any: to tell which AI it was.
    pub(crate) replaced: Option<String>,
}

/// The copy kept of a configuration before Ipse changes it.
pub(crate) fn backup_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".before-ipse");
    path.with_file_name(name)
}

/// Adds or replaces Ipse's entry in `app`'s configuration at `path`: `program` (this
/// `ipse`) serves the memory at `vault` with the access code `token`.
///
/// # Errors
///
/// [`Error::Refused`] when the file is not JSON Ipse can read, saying what to do; it
/// is left as it is. [`Error::Io`] when it cannot be read, copied or written.
pub(crate) fn install(
    app: App,
    path: &Path,
    program: &Path,
    vault: &Path,
    token: &str,
) -> Result<Installed> {
    let io = |action: &'static str, path: &Path| {
        let path = path.to_owned();
        move |source| Error::Io {
            action,
            path,
            source,
        }
    };
    let before = match std::fs::read(path) {
        Ok(bytes) => Some(bytes),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(io("read", path)(e)),
    };
    let unreadable = || {
        Error::Refused(format!(
            "{} is not a file Ipse can read, so it was left as it is. Fix it in {}, or add Ipse to it by hand (docs/guide/connecting.md).",
            path.display(),
            app.label()
        ))
    };
    let old = match &before {
        None => "",
        Some(bytes) => std::str::from_utf8(bytes).map_err(|_| unreadable())?,
    };
    let (text, replaced) = match app.format() {
        Format::Json(key) => with_json_entry(old, key, app.entry(program, vault, token)),
        Format::GooseYaml => with_goose_entry(old, program, vault, token),
    }
    .ok_or_else(unreadable)?;

    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(io("create", dir))?;
    }
    let backup = match &before {
        Some(bytes) => {
            let copy = backup_path(path);
            write_private(&copy, bytes, path)?;
            Some(copy)
        }
        None => None,
    };
    write_private(path, text.as_bytes(), path)?;
    Ok(Installed {
        path: path.to_owned(),
        backup,
        replaced,
    })
}

/// `old`, a JSON configuration, with `entry` as Ipse's under `key`; and the access
/// code of the entry it replaced. Comments at the top, before the object, are kept as
/// they are; `None` for anything else Ipse cannot read as a JSON object.
fn with_json_entry(old: &str, key: &str, entry: Value) -> Option<(String, Option<String>)> {
    let (header, body) = split_header(old);
    let mut config: Map<String, Value> = if body.trim().is_empty() {
        Map::new()
    } else {
        match serde_json::from_str::<Value>(body).ok()? {
            Value::Object(map) => map,
            _ => return None,
        }
    };
    let Value::Object(servers) = config
        .entry(key)
        .or_insert_with(|| Value::Object(Map::new()))
    else {
        return None;
    };
    let replaced = servers
        .get(ENTRY)
        .and_then(|e| e.pointer("/env/IPSE_TOKEN"))
        .and_then(Value::as_str)
        .map(str::to_owned);
    servers.insert(ENTRY.to_owned(), entry);
    let text = serde_json::to_string_pretty(&Value::Object(config)).ok()?;
    Some((format!("{header}{text}\n"), replaced))
}

/// The blank and `//` lines at the top of `text`, and the rest.
fn split_header(text: &str) -> (&str, &str) {
    let mut at = 0;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("//") {
            at += line.len();
        } else {
            break;
        }
    }
    // Only comments: none of it is a header to keep before a new object.
    if text[..at].trim().is_empty() {
        ("", text)
    } else {
        text.split_at(at)
    }
}

/// Ipse's extension for Goose, indented by `indent` spaces under `extensions:`, and
/// by as many at each level below, as the file is. Its strings are written as JSON
/// strings, which YAML reads as they are.
fn goose_entry_at(program: &Path, vault: &Path, token: &str, indent: usize) -> String {
    let quote = |text: &str| serde_json::to_string(text).unwrap_or_default();
    let pad = " ".repeat(indent);
    let inner = " ".repeat(indent * 2);
    let deeper = " ".repeat(indent * 3);
    let lines = [
        format!("{pad}{ENTRY}:"),
        format!("{inner}name: Ipse"),
        format!("{inner}description: {}", quote("Your memory, from Ipse")),
        format!("{inner}type: stdio"),
        format!("{inner}cmd: {}", quote(&plain(program))),
        format!(
            "{inner}args: [\"mcp\", \"--vault\", {}]",
            quote(&plain(vault))
        ),
        format!("{inner}envs:"),
        format!("{deeper}IPSE_TOKEN: {}", quote(token)),
        format!("{inner}enabled: true"),
        format!("{inner}timeout: 300"),
    ];
    let mut text = lines.join("\n");
    text.push('\n');
    text
}

fn goose_entry(program: &Path, vault: &Path, token: &str) -> String {
    goose_entry_at(program, vault, token, 2)
}

/// How many spaces start `line`.
fn indent_of(line: &str) -> usize {
    line.len() - line.trim_start_matches(' ').len()
}

/// True for a line that holds YAML content: not blank, not a comment.
fn content(line: &str) -> bool {
    let trimmed = line.trim();
    !trimmed.is_empty() && !trimmed.starts_with('#')
}

/// `old`, Goose's `config.yaml`, with Ipse's extension under `extensions`; and the
/// access code of the entry it replaced. Edited line by line, so that the rest stays
/// exactly as it was; `None` when `extensions` is not written in a way Ipse can follow
/// (on one line, or with tabs).
fn with_goose_entry(
    old: &str,
    program: &Path,
    vault: &Path,
    token: &str,
) -> Option<(String, Option<String>)> {
    if old.contains('\t') {
        return None;
    }
    let mut lines: Vec<String> = old.lines().map(str::to_owned).collect();
    let header = lines.iter().position(|l| {
        l.strip_prefix("extensions:")
            .is_some_and(|rest| !content(rest) || rest.trim() == "{}")
    });
    let Some(header) = header else {
        if lines.iter().any(|l| l.starts_with("extensions")) {
            return None;
        }
        let mut text = old.to_owned();
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        text.push_str("extensions:\n");
        text.push_str(&goose_entry(program, vault, token));
        return Some((text, None));
    };
    "extensions:".clone_into(&mut lines[header]);
    // The extensions: every line after, up to the next key at the top.
    let end = lines[header + 1..]
        .iter()
        .position(|l| content(l) && indent_of(l) == 0)
        .map_or(lines.len(), |i| header + 1 + i);
    let indent = lines[header + 1..end]
        .iter()
        .find(|l| content(l))
        .map_or(2, |l| indent_of(l));
    let mut replaced = None;
    let mine = (header + 1..end).find(|&i| {
        indent_of(&lines[i]) == indent && lines[i].trim_start().starts_with(&format!("{ENTRY}:"))
    });
    let at = if let Some(start) = mine {
        let stop = (start + 1..end)
            .find(|&i| content(&lines[i]) && indent_of(&lines[i]) <= indent)
            .unwrap_or(end);
        replaced = lines[start..stop].iter().find_map(|l| {
            let value = l.trim().strip_prefix("IPSE_TOKEN:")?.trim();
            Some(value.trim_matches(['"', '\'']).to_owned())
        });
        lines.drain(start..stop);
        start
    } else {
        header + 1
    };
    let entry = goose_entry_at(program, vault, token, indent);
    lines.splice(at..at, entry.lines().map(str::to_owned));
    let mut text = lines.join("\n");
    text.push('\n');
    Some((text, replaced))
}

/// Writes `bytes` to `path` atomically, readable by its owner only: the file holds an
/// access code, and often other apps' secrets. `like` keeps the permissions of an
/// existing configuration when they are as strict.
fn write_private(path: &Path, bytes: &[u8], like: &Path) -> Result<()> {
    ipse_core::vault::write_atomic(path, bytes)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode =
            std::fs::metadata(like).map_or(0o600, |m| m.permissions().mode() & 0o777) & 0o700;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode.max(0o600))).map_err(
            |source| Error::Io {
                action: "protect",
                path: path.to_owned(),
                source,
            },
        )?;
    }
    #[cfg(not(unix))]
    let _ = like;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn places(home: &Path) -> Places {
        Places {
            home: home.to_owned(),
            app_data: Some(home.join("AppData").join("Roaming")),
        }
    }

    #[test]
    #[allow(clippy::too_many_lines)] // one table: each app on each system
    fn each_app_has_its_file_on_each_system() {
        let home = Path::new("/home/ines");
        let p = places(home);
        let cases = [
            (
                App::ClaudeDesktop,
                System::MacOs,
                "/home/ines/Library/Application Support/Claude/claude_desktop_config.json",
            ),
            (
                App::ClaudeDesktop,
                System::Windows,
                "/home/ines/AppData/Roaming/Claude/claude_desktop_config.json",
            ),
            (
                App::ClaudeDesktop,
                System::Linux,
                "/home/ines/.config/Claude/claude_desktop_config.json",
            ),
            (App::ClaudeCode, System::MacOs, "/home/ines/.claude.json"),
            (App::ClaudeCode, System::Windows, "/home/ines/.claude.json"),
            (App::ClaudeCode, System::Linux, "/home/ines/.claude.json"),
            (
                App::LmStudio,
                System::MacOs,
                "/home/ines/.lmstudio/mcp.json",
            ),
            (
                App::LmStudio,
                System::Windows,
                "/home/ines/.lmstudio/mcp.json",
            ),
            (
                App::LmStudio,
                System::Linux,
                "/home/ines/.lmstudio/mcp.json",
            ),
            (App::Cursor, System::MacOs, "/home/ines/.cursor/mcp.json"),
            (App::Cursor, System::Windows, "/home/ines/.cursor/mcp.json"),
            (App::Cursor, System::Linux, "/home/ines/.cursor/mcp.json"),
            (
                App::VsCode,
                System::MacOs,
                "/home/ines/Library/Application Support/Code/User/mcp.json",
            ),
            (
                App::VsCode,
                System::Windows,
                "/home/ines/AppData/Roaming/Code/User/mcp.json",
            ),
            (
                App::VsCode,
                System::Linux,
                "/home/ines/.config/Code/User/mcp.json",
            ),
            (
                App::Windsurf,
                System::MacOs,
                "/home/ines/.codeium/windsurf/mcp_config.json",
            ),
            (
                App::Windsurf,
                System::Windows,
                "/home/ines/.codeium/windsurf/mcp_config.json",
            ),
            (
                App::Windsurf,
                System::Linux,
                "/home/ines/.codeium/windsurf/mcp_config.json",
            ),
            (
                App::Zed,
                System::MacOs,
                "/home/ines/.config/zed/settings.json",
            ),
            (
                App::Zed,
                System::Windows,
                "/home/ines/AppData/Roaming/Zed/settings.json",
            ),
            (
                App::Zed,
                System::Linux,
                "/home/ines/.config/zed/settings.json",
            ),
            (
                App::Goose,
                System::MacOs,
                "/home/ines/.config/goose/config.yaml",
            ),
            (
                App::Goose,
                System::Windows,
                "/home/ines/AppData/Roaming/Block/goose/config/config.yaml",
            ),
            (
                App::Goose,
                System::Linux,
                "/home/ines/.config/goose/config.yaml",
            ),
        ];
        for (app, system, expected) in cases {
            assert_eq!(
                app.config_path(system, &p),
                PathBuf::from(expected),
                "{app:?} on {system:?}"
            );
        }
        let no_app_data = Places {
            app_data: None,
            ..p
        };
        assert_eq!(
            App::ClaudeDesktop.config_path(System::Windows, &no_app_data),
            PathBuf::from("/home/ines/AppData/Roaming/Claude/claude_desktop_config.json")
        );
        assert_eq!(App::parse(" lm-studio ").unwrap(), App::LmStudio);
        let Err(Error::Refused(message)) = App::parse("chatgpt") else {
            panic!("parsed")
        };
        assert!(
            message.contains(
                "claude-desktop, claude-code, lm-studio, cursor, vs-code, windsurf, zed or goose"
            ),
            "{message}"
        );
        for app in App::ALL {
            assert_eq!(App::parse(app.id()).unwrap(), app);
        }
    }

    #[test]
    fn apps_are_found_by_their_folders() {
        let dir = tempfile::tempdir().unwrap();
        let p = places(dir.path());
        for app in App::ALL {
            assert!(!app.found(System::Linux, &p), "{app:?}");
        }
        std::fs::create_dir_all(dir.path().join(".config/Claude")).unwrap();
        std::fs::create_dir_all(dir.path().join(".lmstudio")).unwrap();
        std::fs::write(dir.path().join(".claude.json"), "{}").unwrap();
        for folder in [
            ".cursor",
            ".config/Code/User",
            ".codeium/windsurf",
            ".config/zed",
            ".config/goose",
        ] {
            std::fs::create_dir_all(dir.path().join(folder)).unwrap();
        }
        for app in App::ALL {
            assert!(app.found(System::Linux, &p), "{app:?}");
        }
    }

    const PROGRAM: &str = "/home/ines/.ipse/bin/ipse";
    const VAULT: &str = "/home/ines/Ipse";

    fn read(path: &Path) -> Value {
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
    }

    #[test]
    fn a_new_file_gets_ipse_only() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Claude/claude_desktop_config.json");
        let done = install(
            App::ClaudeDesktop,
            &path,
            Path::new(PROGRAM),
            Path::new(VAULT),
            "ipse_secret",
        )
        .unwrap();
        assert_eq!(done.backup, None);
        assert_eq!(done.replaced, None);
        assert_eq!(
            read(&path),
            json!({"mcpServers": {"ipse": {
                "command": PROGRAM,
                "args": ["mcp", "--vault", VAULT],
                "env": {"IPSE_TOKEN": "ipse_secret"}
            }}})
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "the file holds a secret");
        }
    }

    #[test]
    fn other_servers_and_keys_are_kept_and_ipse_is_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".claude.json");
        let before = json!({
            "numStartups": 12,
            "projects": {"/home/ines/dev": {"mcpServers": {}}},
            "mcpServers": {
                "github": {"type": "http", "url": "https://example.org/mcp"},
                "ipse": {"command": "/old/ipse", "args": [], "env": {"IPSE_TOKEN": "ipse_old"}}
            }
        });
        let text = serde_json::to_string_pretty(&before).unwrap();
        std::fs::write(&path, &text).unwrap();
        let done = install(
            App::ClaudeCode,
            &path,
            Path::new(PROGRAM),
            Path::new(VAULT),
            "ipse_new",
        )
        .unwrap();
        assert_eq!(done.replaced.as_deref(), Some("ipse_old"));
        let backup = done.backup.unwrap();
        assert!(backup.ends_with(".claude.json.before-ipse"));
        assert_eq!(std::fs::read_to_string(&backup).unwrap(), text);
        let after = read(&path);
        assert_eq!(after["numStartups"], 12);
        assert_eq!(after["projects"], before["projects"]);
        assert_eq!(
            after["mcpServers"]["github"],
            before["mcpServers"]["github"]
        );
        assert_eq!(after["mcpServers"]["ipse"]["type"], "stdio");
        assert_eq!(after["mcpServers"]["ipse"]["env"]["IPSE_TOKEN"], "ipse_new");
        assert_eq!(after["mcpServers"].as_object().unwrap().len(), 2);

        // Again: still one entry.
        install(
            App::ClaudeCode,
            &path,
            Path::new(PROGRAM),
            Path::new(VAULT),
            "ipse_third",
        )
        .unwrap();
        let after = read(&path);
        assert_eq!(after["mcpServers"].as_object().unwrap().len(), 2);
        assert_eq!(
            after["mcpServers"]["ipse"]["env"]["IPSE_TOKEN"],
            "ipse_third"
        );
    }

    #[test]
    fn a_file_ipse_cannot_read_is_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        for (i, text) in [
            "{ \"mcpServers\": { // a comment\n } }",
            "[1, 2]",
            "{\"mcpServers\": [\"ipse\"]}",
        ]
        .into_iter()
        .enumerate()
        {
            let path = dir.path().join(format!("mcp-{i}.json"));
            std::fs::write(&path, text).unwrap();
            let Err(Error::Refused(message)) = install(
                App::LmStudio,
                &path,
                Path::new(PROGRAM),
                Path::new(VAULT),
                "ipse_secret",
            ) else {
                panic!("written: {text}")
            };
            assert!(message.contains("left as it is"), "{message}");
            assert!(!message.contains("ipse_secret"));
            assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
            assert!(!backup_path(&path).exists());
        }
        // An empty file is a new one.
        let path = dir.path().join("empty.json");
        std::fs::write(&path, "\n").unwrap();
        install(
            App::LmStudio,
            &path,
            Path::new(PROGRAM),
            Path::new(VAULT),
            "ipse_secret",
        )
        .unwrap();
        assert_eq!(read(&path)["mcpServers"]["ipse"]["command"], PROGRAM);
    }

    fn install_at(app: App, path: &Path, token: &str) -> Installed {
        install(app, path, Path::new(PROGRAM), Path::new(VAULT), token).unwrap()
    }

    #[test]
    fn each_app_gets_its_own_shape_of_entry() {
        let dir = tempfile::tempdir().unwrap();
        for app in [App::Cursor, App::Windsurf] {
            let path = dir.path().join(format!("{}.json", app.id()));
            install_at(app, &path, "ipse_secret");
            assert_eq!(
                read(&path),
                json!({"mcpServers": {"ipse": {
                    "command": PROGRAM,
                    "args": ["mcp", "--vault", VAULT],
                    "env": {"IPSE_TOKEN": "ipse_secret"}
                }}}),
                "{app:?}"
            );
        }
        // VS Code: under `servers`, beside its `inputs`.
        let path = dir.path().join("mcp.json");
        std::fs::write(
            &path,
            r#"{"inputs": [{"id": "key"}], "servers": {"other": {}}}"#,
        )
        .unwrap();
        install_at(App::VsCode, &path, "ipse_secret");
        let after = read(&path);
        assert_eq!(after["inputs"][0]["id"], "key");
        assert!(after["servers"]["other"].is_object());
        assert_eq!(after["servers"]["ipse"]["type"], "stdio");
        assert_eq!(after["servers"]["ipse"]["env"]["IPSE_TOKEN"], "ipse_secret");
        assert!(after.get("mcpServers").is_none());
    }

    #[test]
    fn zed_keeps_the_comments_at_the_top_of_its_settings() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let header = "// Zed settings\n//\n// For information on how to configure Zed, see the Zed\n// documentation: https://zed.dev/docs/configuring-zed\n";
        let before =
            format!("{header}{{\n  \"ui_font_size\": 16,\n  \"theme\": \"One Dark\"\n}}\n");
        std::fs::write(&path, &before).unwrap();
        let done = install_at(App::Zed, &path, "ipse_secret");
        assert_eq!(
            std::fs::read_to_string(done.backup.unwrap()).unwrap(),
            before
        );
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.starts_with(header), "{text}");
        let after: Value = serde_json::from_str(&text[header.len()..]).unwrap();
        assert_eq!(after["ui_font_size"], 16);
        let entry = &after["context_servers"]["ipse"];
        assert_eq!(entry["source"], "custom");
        assert_eq!(entry["command"], PROGRAM);
        assert_eq!(entry["env"]["IPSE_TOKEN"], "ipse_secret");

        // A comment inside the object: left alone.
        let inside = "{\n  // my font\n  \"ui_font_size\": 16\n}\n";
        std::fs::write(&path, inside).unwrap();
        assert!(install(App::Zed, &path, Path::new(PROGRAM), Path::new(VAULT), "x").is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), inside);
    }

    #[test]
    fn goose_gets_an_extension_and_the_rest_of_its_file_stays() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        // A new file.
        install_at(App::Goose, &path, "ipse_one");
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(
            text.starts_with("extensions:\n  ipse:\n    name: Ipse\n"),
            "{text}"
        );
        assert!(
            text.contains(&format!("    cmd: \"{PROGRAM}\"\n")),
            "{text}"
        );
        assert!(text.contains(&format!("    args: [\"mcp\", \"--vault\", \"{VAULT}\"]\n")));
        assert!(text.contains("      IPSE_TOKEN: \"ipse_one\"\n"), "{text}");

        // Goose's own file, with another extension: kept line for line.
        let before = "GOOSE_PROVIDER: anthropic\nextensions:\n    developer:\n        bundled: true\n        enabled: true\n        name: developer\n        type: builtin\nGOOSE_MODE: smart_approve\n";
        std::fs::write(&path, before).unwrap();
        let done = install_at(App::Goose, &path, "ipse_two");
        assert_eq!(done.replaced, None);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(
            text.starts_with(
                "GOOSE_PROVIDER: anthropic\nextensions:\n    ipse:\n        name: Ipse\n"
            ),
            "{text}"
        );
        assert!(
            text.contains("    developer:\n        bundled: true\n"),
            "{text}"
        );
        assert!(
            text.contains("          IPSE_TOKEN: \"ipse_two\"\n"),
            "{text}"
        );
        assert!(text.ends_with("GOOSE_MODE: smart_approve\n"), "{text}");

        // Again: replaced, not added twice, and the code it replaced is told.
        let done = install_at(App::Goose, &path, "ipse_three");
        assert_eq!(done.replaced.as_deref(), Some("ipse_two"));
        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(text.matches("    ipse:\n").count(), 1, "{text}");
        assert!(!text.contains("ipse_two"));
        assert!(text.contains("    developer:\n        bundled: true\n        enabled: true\n        name: developer\n        type: builtin\nGOOSE_MODE"), "{text}");

        // `extensions: {}` is empty; extensions on one line are left alone.
        std::fs::write(&path, "extensions: {}\n").unwrap();
        install_at(App::Goose, &path, "ipse_four");
        assert!(
            std::fs::read_to_string(&path)
                .unwrap()
                .starts_with("extensions:\n  ipse:\n")
        );
        let flow = "extensions: {developer: {enabled: true}}\n";
        std::fs::write(&path, flow).unwrap();
        assert!(install(App::Goose, &path, Path::new(PROGRAM), Path::new(VAULT), "x").is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), flow);
    }

    #[test]
    fn what_to_paste_is_written_for_each_app() {
        let (program, vault) = (Path::new(PROGRAM), Path::new(VAULT));
        let vs: Value =
            serde_json::from_str(&App::VsCode.snippet(program, vault, "ipse_s")).unwrap();
        assert_eq!(vs["servers"]["ipse"]["env"]["IPSE_TOKEN"], "ipse_s");
        let zed: Value = serde_json::from_str(&App::Zed.snippet(program, vault, "ipse_s")).unwrap();
        assert_eq!(zed["context_servers"]["ipse"]["command"], PROGRAM);
        let any: Value = serde_json::from_str(&any_app_snippet(program, vault, "ipse_s")).unwrap();
        assert_eq!(any["mcpServers"]["ipse"]["args"][2], VAULT);
        let goose = App::Goose.snippet(program, vault, "ipse_s");
        assert!(goose.starts_with("extensions:\n  ipse:\n"), "{goose}");
        assert_eq!(
            command(program, vault),
            format!("\"{PROGRAM}\" mcp --vault \"{VAULT}\"")
        );
    }
}
