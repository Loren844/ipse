//! Writing Ipse into an AI app's MCP configuration (issue #105), so that connecting an
//! AI is one command, or one button in the interface, instead of a JSON block to paste.
//!
//! Each app keeps its MCP servers in a JSON file under `mcpServers`. Ipse adds or
//! replaces its own entry, `ipse`, and leaves everything else as it was. The previous
//! file is kept next to it (`<file>.before-ipse`). A file Ipse cannot read as JSON is
//! left alone, with a plain explanation. The access code is written in the file and
//! nowhere else: never printed, never logged.

use std::path::{Path, PathBuf};

use ipse_core::{Error, Result};
use serde_json::{Map, Value, json};

/// The name of Ipse's entry in an app's `mcpServers`.
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
}

impl App {
    /// Every app, in the order they are offered.
    pub(crate) const ALL: [Self; 3] = [Self::ClaudeDesktop, Self::ClaudeCode, Self::LmStudio];

    /// Its name on the command line: `claude-desktop`, `claude-code`, `lm-studio`.
    pub(crate) fn id(self) -> &'static str {
        match self {
            Self::ClaudeDesktop => "claude-desktop",
            Self::ClaudeCode => "claude-code",
            Self::LmStudio => "lm-studio",
        }
    }

    /// Its name for people.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::ClaudeDesktop => "Claude Desktop",
            Self::ClaudeCode => "Claude Code",
            Self::LmStudio => "LM Studio",
        }
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
                    "Ipse cannot set up `{}`: use claude-desktop, claude-code or lm-studio. For another app, connect it by hand (docs/guide/connecting.md).",
                    text.trim()
                ))
            })
    }

    /// The file this app reads its MCP servers from, on `system`.
    pub(crate) fn config_path(self, system: System, places: &Places) -> PathBuf {
        let home = &places.home;
        match self {
            Self::ClaudeDesktop => match system {
                System::MacOs => home
                    .join("Library")
                    .join("Application Support")
                    .join("Claude")
                    .join("claude_desktop_config.json"),
                System::Windows => places
                    .app_data
                    .clone()
                    .unwrap_or_else(|| home.join("AppData").join("Roaming"))
                    .join("Claude")
                    .join("claude_desktop_config.json"),
                System::Linux => home
                    .join(".config")
                    .join("Claude")
                    .join("claude_desktop_config.json"),
            },
            Self::ClaudeCode => home.join(".claude.json"),
            Self::LmStudio => home.join(".lmstudio").join("mcp.json"),
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
        if self == Self::ClaudeCode
            && let Some(map) = entry.as_object_mut()
        {
            map.insert("type".to_owned(), Value::String("stdio".to_owned()));
        }
        entry
    }
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
            "{} is not a JSON file Ipse can read, so it was left as it is. Fix it in {}, or add Ipse to it by hand (docs/guide/connecting.md).",
            path.display(),
            app.label()
        ))
    };
    let mut config: Map<String, Value> = match &before {
        None => Map::new(),
        Some(bytes) if bytes.iter().all(u8::is_ascii_whitespace) => Map::new(),
        Some(bytes) => match serde_json::from_slice::<Value>(bytes) {
            Ok(Value::Object(map)) => map,
            _ => return Err(unreadable()),
        },
    };
    let servers = config
        .entry("mcpServers")
        .or_insert_with(|| Value::Object(Map::new()));
    let Value::Object(servers) = servers else {
        return Err(unreadable());
    };
    let replaced = servers
        .get(ENTRY)
        .and_then(|e| e.pointer("/env/IPSE_TOKEN"))
        .and_then(Value::as_str)
        .map(str::to_owned);
    servers.insert(ENTRY.to_owned(), app.entry(program, vault, token));

    let mut text = serde_json::to_string_pretty(&Value::Object(config)).map_err(|_| {
        Error::Refused("The configuration could not be written as JSON.".to_owned())
    })?;
    text.push('\n');
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
        assert!(message.contains("claude-desktop, claude-code or lm-studio"));
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
}
