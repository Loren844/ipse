//! Connected AIs and their access codes, `.ipse/clients.toml` (`docs/spec/rights.md`,
//! section 2).
//!
//! An access code is shown once, when the AI is connected. Ipse keeps only its BLAKE3
//! hash, so the file never holds anything that grants access.

use std::fmt::{self, Write as _};
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::slug::to_file_stem;
use crate::vault::{DateTime, write_atomic};
use crate::{Error, Result};

/// Where clients are listed, relative to the vault's folder.
pub const CLIENTS_FILE: &str = ".ipse/clients.toml";

/// Prefix of every access code, so that one is recognizable when pasted.
const TOKEN_PREFIX: &str = "ipse_";

/// An access code, as given to the AI app. Shown once; never stored by Ipse.
pub struct Token(String);

impl Token {
    /// The code to give to the AI app.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[access code]")
    }
}

/// One connected AI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Client {
    /// Identifier, from its name: `claude-desktop`.
    pub id: String,
    /// Name shown to the person.
    pub name: String,
    /// When it was connected.
    pub created: String,
    token_hash: String,
    /// Categories it may read.
    pub read: Vec<String>,
    /// Categories it may submit proposals for; each must also be in `read`.
    pub propose: Vec<String>,
    /// The person said this AI runs entirely on this computer.
    pub local: bool,
    /// The kinds of change the person accepts in advance from this AI (decision
    /// 0017): empty unless they turned one on.
    pub auto_accept: Vec<Standing>,
    /// The kinds the person said no to, or turned off: Ipse no longer offers them.
    pub declined: Vec<Standing>,
}

/// A kind of change the person may accept in advance from one AI (decision 0017).
/// Only low-risk changes the AI made for a task Ipse gave it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Standing {
    /// Summaries of a day, week, month or year, written for a `summary` task.
    Summaries,
    /// Event kinds, set for a `classify` task.
    EventKinds,
    /// New files of the kinds that say nothing about anyone (things, topics, projects,
    /// organizations, works) in a category that is not sensitive, and facts or relations
    /// added later to the files that AI created (decision 0018).
    Creations,
}

impl Standing {
    /// Every kind, in the order the interface lists them.
    pub const ALL: [Self; 3] = [Self::Summaries, Self::EventKinds, Self::Creations];

    /// `summaries`, `event_kinds` or `creations`.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Summaries => "summaries",
            Self::EventKinds => "event_kinds",
            Self::Creations => "creations",
        }
    }

    /// The kind named `text`; `None` for any other name.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.as_str() == text)
    }
}

impl Client {
    /// The vault's owner, as the interface reads on their behalf: every category of
    /// `categories`, and `private` for unknown ones. Never in `.ipse/clients.toml`, and
    /// no access code is its.
    #[must_use]
    pub fn owner(categories: &super::Categories) -> Self {
        Self {
            id: "owner".to_owned(),
            name: "owner".to_owned(),
            created: String::new(),
            token_hash: String::new(),
            read: categories.all().into_iter().map(str::to_owned).collect(),
            propose: Vec::new(),
            local: true,
            auto_accept: Vec::new(),
            declined: Vec::new(),
        }
    }

    /// True when the client may read `category`, among the vault's `categories`.
    /// Unknown categories count as `private`.
    #[must_use]
    pub fn reads(&self, category: &str, categories: &super::Categories) -> bool {
        let category = categories.effective(category);
        self.read.iter().any(|c| c == category)
    }
}

#[derive(Deserialize)]
struct File {
    #[serde(default)]
    client: Vec<RawClient>,
}

#[derive(Deserialize)]
struct RawClient {
    id: String,
    name: String,
    created: toml::value::Datetime,
    token_hash: String,
    #[serde(default)]
    read: Vec<String>,
    #[serde(default)]
    propose: Vec<String>,
    #[serde(default)]
    local: bool,
    /// Unknown names, from a later version, are ignored.
    #[serde(default)]
    auto_accept: Vec<String>,
    #[serde(default)]
    declined: Vec<String>,
}

/// The connected AIs of one vault.
#[derive(Debug, Clone)]
pub struct Clients {
    path: PathBuf,
    clients: Vec<Client>,
}

impl Clients {
    /// Reads the clients of the vault in `root`. No file means no client.
    ///
    /// # Errors
    ///
    /// [`Error::Io`] when the file exists but cannot be read; [`Error::Invalid`] when
    /// it is not valid.
    pub fn load(root: &Path) -> Result<Self> {
        let path = root.join(CLIENTS_FILE);
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self {
                    path,
                    clients: Vec::new(),
                });
            }
            Err(source) => {
                return Err(Error::Io {
                    action: "read",
                    path,
                    source,
                });
            }
        };
        let file: File = toml::from_str(&text).map_err(|_| Error::Invalid {
            path: path.clone(),
            line: None,
            problem: "the list of connected AIs is not valid; remove the AI and connect it again"
                .to_owned(),
        })?;
        let clients = file
            .client
            .into_iter()
            .map(|c| Client {
                id: c.id,
                name: c.name,
                created: c.created.to_string(),
                token_hash: c.token_hash,
                read: c.read,
                propose: c.propose,
                local: c.local,
                auto_accept: c
                    .auto_accept
                    .iter()
                    .filter_map(|s| Standing::parse(s))
                    .collect(),
                declined: c
                    .declined
                    .iter()
                    .filter_map(|s| Standing::parse(s))
                    .collect(),
            })
            .collect();
        Ok(Self { path, clients })
    }

    /// The connected AIs.
    #[must_use]
    pub fn list(&self) -> &[Client] {
        &self.clients
    }

    /// Connects a new AI named `name`, reading every category of `categories` that is
    /// not sensitive, and saves. `local` records that the person said it runs entirely
    /// on this computer. Returns it with its access code, which is shown only now.
    ///
    /// # Errors
    ///
    /// [`Error::Refused`] for an empty name or when no random code can be made;
    /// [`Error::Io`] when the list cannot be saved.
    pub fn add(
        &mut self,
        name: &str,
        created: DateTime,
        categories: &super::Categories,
        local: bool,
    ) -> Result<(Client, Token)> {
        let name = name.trim();
        if name.is_empty() {
            return Err(Error::Refused(
                "Give the AI a name, like \"Claude\".".to_owned(),
            ));
        }
        let mut bytes = [0_u8; 32];
        getrandom::fill(&mut bytes).map_err(|_| {
            Error::Refused("This computer could not make a random access code.".to_owned())
        })?;
        let token = Token(format!("{TOKEN_PREFIX}{}", hex(&bytes)));

        let base = to_file_stem(name).unwrap_or_else(|| "ai".to_owned());
        let mut id = base.clone();
        let mut n = 2;
        while self.clients.iter().any(|c| c.id == id) {
            id = format!("{base}-{n}");
            n += 1;
        }
        let client = Client {
            id,
            name: name.to_owned(),
            created: created.to_string(),
            token_hash: hash(token.expose()),
            read: categories.default_read(),
            propose: Vec::new(),
            local,
            auto_accept: Vec::new(),
            declined: Vec::new(),
        };
        self.clients.push(client.clone());
        self.save()?;
        Ok((client, token))
    }

    /// Disconnects the AI with this id and saves. Its code stops working at once.
    /// Returns `false` when there is no such AI.
    ///
    /// # Errors
    ///
    /// [`Error::Io`] when the list cannot be saved.
    pub fn remove(&mut self, id: &str) -> Result<bool> {
        let before = self.clients.len();
        self.clients.retain(|c| c.id != id);
        if self.clients.len() == before {
            return Ok(false);
        }
        self.save()?;
        Ok(true)
    }

    /// Sets what the AI `id` may read and propose on, each when given, and saves. A
    /// category it proposes on must be one it reads.
    ///
    /// # Errors
    ///
    /// [`Error::Refused`] when there is no such AI, a category is unknown, or one
    /// proposed on is not read; [`Error::Io`] when the list cannot be saved.
    pub fn set_rights(
        &mut self,
        id: &str,
        read: Option<Vec<String>>,
        propose: Option<Vec<String>>,
        categories: &super::Categories,
    ) -> Result<Client> {
        let Some(at) = self.clients.iter().position(|c| c.id == id) else {
            return Err(Error::Refused(format!(
                "There is no AI {id}. `ipse client list` shows their ids."
            )));
        };
        let clean = |list: Vec<String>| -> Result<Vec<String>> {
            let mut out: Vec<String> = Vec::new();
            for name in list {
                let name = name.trim().to_owned();
                if name.is_empty() || out.contains(&name) {
                    continue;
                }
                if !categories.known(&name) {
                    return Err(Error::Refused(format!(
                        "`{name}` is not a category of this memory. Add it to .ipse/vault.toml first."
                    )));
                }
                out.push(name);
            }
            Ok(out)
        };
        let read = match read {
            Some(list) => clean(list)?,
            None => self.clients[at].read.clone(),
        };
        let propose = match propose {
            Some(list) => clean(list)?,
            None => self.clients[at].propose.clone(),
        };
        if let Some(name) = propose.iter().find(|c| !read.contains(c)) {
            return Err(Error::Refused(format!(
                "An AI can only propose changes in a category it reads: give `{name}` to read too."
            )));
        }
        let client = &mut self.clients[at];
        client.read = read;
        client.propose = propose;
        let client = client.clone();
        self.save()?;
        Ok(client)
    }

    /// Turns on or off the standing consent `standing` for the AI `id` (decision
    /// 0017), and saves. Turned off, it is no longer offered.
    ///
    /// # Errors
    ///
    /// [`Error::Refused`] when there is no such AI; [`Error::Io`] when the list cannot
    /// be saved.
    pub fn set_auto_accept(&mut self, id: &str, standing: Standing, on: bool) -> Result<Client> {
        let Some(client) = self.clients.iter_mut().find(|c| c.id == id) else {
            return Err(Error::Refused(format!(
                "There is no AI {id}. `ipse client list` shows their ids."
            )));
        };
        client.auto_accept.retain(|s| *s != standing);
        if on {
            client.auto_accept.push(standing);
            client.auto_accept.sort_unstable();
        } else if !client.declined.contains(&standing) {
            client.declined.push(standing);
            client.declined.sort_unstable();
        }
        let client = client.clone();
        self.save()?;
        Ok(client)
    }

    /// Records that the person does not want `standing` offered again for the AI `id`,
    /// and saves.
    ///
    /// # Errors
    ///
    /// As for [`Clients::set_auto_accept`].
    pub fn decline(&mut self, id: &str, standing: Standing) -> Result<Client> {
        let Some(client) = self.clients.iter_mut().find(|c| c.id == id) else {
            return Err(Error::Refused(format!(
                "There is no AI {id}. `ipse client list` shows their ids."
            )));
        };
        if !client.declined.contains(&standing) {
            client.declined.push(standing);
            client.declined.sort_unstable();
        }
        let client = client.clone();
        self.save()?;
        Ok(client)
    }

    /// The kinds of standing consent Ipse does not offer the AI: those it has, and
    /// those the person declined. Give it to [`crate::proposals::Proposals::offers`].
    #[must_use]
    pub fn not_offered(client: &Client) -> Vec<Standing> {
        let mut skip = client.auto_accept.clone();
        skip.extend(&client.declined);
        skip
    }

    /// The client whose access code is `token`. Every stored hash is compared in
    /// constant time.
    #[must_use]
    pub fn authenticate(&self, token: &str) -> Option<&Client> {
        let presented = blake3::hash(token.as_bytes());
        let mut found = None;
        for client in &self.clients {
            let stored = client
                .token_hash
                .strip_prefix("b3:")
                .and_then(|h| blake3::Hash::from_hex(h).ok());
            // `blake3::Hash` compares in constant time.
            if stored.is_some_and(|stored| stored == presented) {
                found = Some(client);
            }
        }
        found
    }

    fn save(&self) -> Result<()> {
        let mut out = String::from(
            "# AIs connected to this memory. Access codes are not stored here, only their hashes.\n",
        );
        for c in &self.clients {
            let _ = write!(
                out,
                "\n[[client]]\nid = {}\nname = {}\ncreated = {}\ntoken_hash = {}\nread = {}\npropose = {}\nlocal = {}\n",
                quote(&c.id),
                quote(&c.name),
                c.created,
                quote(&c.token_hash),
                list(&c.read),
                list(&c.propose),
                c.local
            );
            if !c.auto_accept.is_empty() {
                let names: Vec<String> = c
                    .auto_accept
                    .iter()
                    .map(|s| s.as_str().to_owned())
                    .collect();
                let _ = writeln!(out, "auto_accept = {}", list(&names));
            }
            if !c.declined.is_empty() {
                let names: Vec<String> = c.declined.iter().map(|s| s.as_str().to_owned()).collect();
                let _ = writeln!(out, "declined = {}", list(&names));
            }
        }
        write_atomic(&self.path, out.as_bytes())
    }
}

/// `b3:` and the BLAKE3 hash of `token`, in hex.
fn hash(token: &str) -> String {
    format!("b3:{}", blake3::hash(token.as_bytes()).to_hex())
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(out, "{b:02x}");
    }
    out
}

/// A TOML basic string. JSON's string escapes are all valid in TOML.
fn quote(text: &str) -> String {
    serde_json::to_string(text).unwrap_or_else(|_| "\"\"".to_owned())
}

fn list(items: &[String]) -> String {
    let quoted: Vec<String> = items.iter().map(|i| quote(i)).collect();
    format!("[{}]", quoted.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::TimeZone;

    fn none() -> crate::rights::Categories {
        crate::rights::Categories::default()
    }

    fn now() -> DateTime {
        DateTime::now(&TimeZone::UTC)
    }

    #[test]
    fn add_authenticate_remove() {
        let dir = tempfile::tempdir().unwrap();
        let mut clients = Clients::load(dir.path()).unwrap();
        let (client, token) = clients
            .add("Claude (ordinateur)", now(), &none(), false)
            .unwrap();
        assert_eq!(client.id, "claude-ordinateur");
        assert!(token.expose().starts_with("ipse_"));
        assert_eq!(token.expose().len(), 5 + 64);
        assert_eq!(format!("{token:?}"), "[access code]");

        let text = std::fs::read_to_string(dir.path().join(CLIENTS_FILE)).unwrap();
        assert!(
            !text.contains(token.expose()),
            "the code itself is never stored"
        );
        assert!(text.contains(&hash(token.expose())));

        let reloaded = Clients::load(dir.path()).unwrap();
        assert_eq!(reloaded.list(), clients.list());
        assert_eq!(reloaded.authenticate(token.expose()).unwrap().id, client.id);
        assert!(reloaded.authenticate("ipse_wrong").is_none());
        assert!(reloaded.authenticate("").is_none());

        let mut reloaded = reloaded;
        assert!(reloaded.remove(&client.id).unwrap());
        assert!(!reloaded.remove(&client.id).unwrap());
        assert!(
            Clients::load(dir.path())
                .unwrap()
                .authenticate(token.expose())
                .is_none()
        );
    }

    #[test]
    fn ids_are_unique_and_codes_differ() {
        let dir = tempfile::tempdir().unwrap();
        let mut clients = Clients::load(dir.path()).unwrap();
        let (a, ta) = clients.add("Claude", now(), &none(), false).unwrap();
        let (b, tb) = clients.add("Claude", now(), &none(), false).unwrap();
        assert_eq!((a.id.as_str(), b.id.as_str()), ("claude", "claude-2"));
        assert_ne!(ta.expose(), tb.expose());
        assert_eq!(clients.authenticate(tb.expose()).unwrap().id, "claude-2");
    }

    #[test]
    fn new_clients_cannot_read_sensitive_categories() {
        let dir = tempfile::tempdir().unwrap();
        let (client, _) = Clients::load(dir.path())
            .unwrap()
            .add("Claude", now(), &none(), false)
            .unwrap();
        assert!(client.reads("people", &none()) && client.reads("identity", &none()));
        for closed in ["location", "home", "health", "private", "studies"] {
            assert!(!client.reads(closed, &none()), "{closed}");
        }
    }

    #[test]
    fn local_is_recorded() {
        let dir = tempfile::tempdir().unwrap();
        let mut clients = Clients::load(dir.path()).unwrap();
        let (client, _) = clients.add("Ollama", now(), &none(), true).unwrap();
        assert!(client.local);
        assert!(Clients::load(dir.path()).unwrap().list()[0].local);
    }

    #[test]
    fn names_with_quotes_are_saved_safely() {
        let dir = tempfile::tempdir().unwrap();
        let mut clients = Clients::load(dir.path()).unwrap();
        clients
            .add("Mon \"IA\"\nlocale", now(), &none(), false)
            .unwrap();
        let reloaded = Clients::load(dir.path()).unwrap();
        assert_eq!(reloaded.list()[0].name, "Mon \"IA\"\nlocale");
    }

    #[test]
    fn empty_name_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        assert!(
            Clients::load(dir.path())
                .unwrap()
                .add(" ", now(), &none(), false)
                .is_err()
        );
    }
}
