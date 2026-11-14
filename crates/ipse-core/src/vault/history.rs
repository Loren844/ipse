//! The vault's invisible history (`docs/spec/vault.md`, section 9).
//!
//! The vault is a git repository the person never has to know about. Every change Ipse
//! makes is one commit, whatever the number of files, with a trailer saying where it
//! came from: `Ipse-Source: connector:contacts`, `person`… Files the person changed in
//! another editor are found by comparing the folder with the last commit, and recorded
//! as `Ipse-Source: person` before Ipse changes anything, so that their edit is never
//! lost and Ipse can tell it was theirs.
//!
//! Git is used through `gix`, in pure Rust: no `git` program is needed. The index is
//! rewritten after each commit, so that a person who opens the folder with git sees a
//! clean state.

use std::collections::{BTreeSet, HashMap};
use std::fmt;
use std::path::{Component, Path, PathBuf};

use gix::ObjectId;
use gix::bstr::ByteSlice;
use gix::object::tree::EntryKind;

use super::atomic::write_atomic;
use crate::{Error, Result};

/// The trailer naming where a change came from.
const TRAILER: &str = "Ipse-Source:";

/// What the vault's `.gitignore` must list: data that is not memory (the raw journal,
/// indexes, the connected AIs and what they read) and editors' scratch files.
const IGNORED: [&str; 11] = [
    "journal/",
    ".ipse/materialized.toml",
    ".ipse/added-relations.toml",
    ".ipse/sync.toml",
    ".ipse/index/",
    ".ipse/proposals/",
    ".ipse/access/",
    ".ipse/clients.toml",
    ".obsidian/workspace*",
    ".trash/",
    ".DS_Store",
];

/// Who or what made a change.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Source {
    /// The person, in Ipse or in another editor.
    Person,
    /// Ipse itself: creating the memory, upgrading its format.
    Ipse,
    /// A connector instance: `connector:calendar-2`.
    Connector(String),
    /// A proposal the person accepted: `proposal:<id>`.
    Proposal(String),
}

impl Source {
    /// Reads `person`, `ipse`, `connector:<instance>` or `proposal:<id>`.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        match text.split_once(':') {
            None if text == "person" => Some(Self::Person),
            None if text == "ipse" => Some(Self::Ipse),
            Some(("connector", name)) if !name.is_empty() => Some(Self::Connector(name.to_owned())),
            Some(("proposal", id)) if !id.is_empty() => Some(Self::Proposal(id.to_owned())),
            _ => None,
        }
    }
}

impl fmt::Display for Source {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Person => f.write_str("person"),
            Self::Ipse => f.write_str("ipse"),
            Self::Connector(name) => write!(f, "connector:{name}"),
            Self::Proposal(id) => write!(f, "proposal:{id}"),
        }
    }
}

/// One change of the history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Its identifier, to undo it later.
    pub id: String,
    /// Where it came from; `None` for a commit made outside Ipse without a trailer.
    pub source: Option<Source>,
    /// Its one-line summary.
    pub summary: String,
    /// The files it touched, relative to the vault, with `/` separators.
    pub files: Vec<String>,
}

/// The history of one vault.
pub struct History {
    repo: gix::Repository,
    root: PathBuf,
}

impl fmt::Debug for History {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // The repository handle has nothing useful to show.
        f.debug_struct("History")
            .field("root", &self.root)
            .finish_non_exhaustive()
    }
}

impl History {
    /// Opens the history of the vault in `root`, creating it when the folder has none,
    /// and makes sure its `.gitignore` lists what is not memory.
    ///
    /// # Errors
    ///
    /// [`Error::Io`] when `.gitignore` cannot be written; [`Error::History`] when the
    /// repository cannot be opened or created.
    pub fn open(root: &Path) -> Result<Self> {
        ensure_ignored(root)?;
        let repo = if root.join(".git").exists() {
            gix::open(root).map_err(|_| failed("open"))?
        } else {
            gix::init(root).map_err(|_| failed("create"))?
        };
        Ok(Self {
            repo,
            root: root.to_owned(),
        })
    }

    /// Starts a change made by `source`. Files written through it, or marked with
    /// [`Change::touched`], become one commit.
    #[must_use]
    pub fn change(&self, source: Source) -> Change<'_> {
        Change {
            history: self,
            source,
            paths: BTreeSet::new(),
        }
    }

    /// Records every file changed, added or removed since the last change, outside
    /// Ipse, as one change by the person. Returns `None` when there was nothing.
    ///
    /// # Errors
    ///
    /// [`Error::Io`] when a file cannot be read; [`Error::History`] when the history
    /// cannot be read or updated.
    pub fn record_outside_edits(&self) -> Result<Option<Entry>> {
        self.record(Source::Person, "Changes made outside Ipse", &[])
    }

    /// True when nothing was ever recorded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.repo.head_id().is_err()
    }

    /// Records every file that differs from the last change, except `skip` (paths
    /// relative to the vault), as one change by `source`.
    pub(crate) fn record(
        &self,
        source: Source,
        summary: &str,
        skip: &[&str],
    ) -> Result<Option<Entry>> {
        let recorded = self.flatten(self.head_tree()?)?;
        let mut excludes = self.excludes()?;
        let mut paths = BTreeSet::new();
        let mut seen = BTreeSet::new();
        let mut stack = vec![self.root.clone()];
        while let Some(dir) = stack.pop() {
            let entries = std::fs::read_dir(&dir).map_err(|source| Error::Io {
                action: "read",
                path: dir.clone(),
                source,
            })?;
            for entry in entries.flatten() {
                let path = entry.path();
                let Some(rel) = git_path(&self.root, &path) else {
                    continue;
                };
                let Ok(kind) = entry.file_type() else {
                    continue;
                };
                if rel == ".git" || skip.contains(&rel.as_str()) {
                    continue;
                }
                let mode = if kind.is_dir() {
                    gix::index::entry::Mode::DIR
                } else {
                    gix::index::entry::Mode::FILE
                };
                let ignored = excludes
                    .at_path(rel.as_str(), Some(mode))
                    .map_err(|_| failed("read"))?
                    .is_excluded();
                if ignored {
                    continue;
                }
                if kind.is_dir() {
                    stack.push(path);
                } else if kind.is_file() {
                    let bytes = std::fs::read(&path).map_err(|source| Error::Io {
                        action: "read",
                        path: path.clone(),
                        source,
                    })?;
                    let id = gix::objs::compute_hash(
                        gix::hash::Kind::Sha1,
                        gix::objs::Kind::Blob,
                        &bytes,
                    )
                    .map_err(|_| failed("read"))?;
                    if recorded.get(&rel) != Some(&id) {
                        paths.insert(rel.clone());
                    }
                    seen.insert(rel);
                }
            }
        }
        for rel in recorded.keys() {
            if !seen.contains(rel) && !skip.contains(&rel.as_str()) {
                paths.insert(rel.clone());
            }
        }
        let change = Change {
            history: self,
            source,
            paths,
        };
        change.commit(summary)
    }

    /// For each of `paths` (relative to the vault), the source of the last change that
    /// touched it. Paths never recorded are left out.
    ///
    /// # Errors
    ///
    /// [`Error::History`] when the history cannot be read.
    pub fn last_sources(&self, paths: &[PathBuf]) -> Result<HashMap<PathBuf, Source>> {
        let mut wanted: HashMap<String, PathBuf> = paths
            .iter()
            .filter_map(|p| relative(&self.root, p).map(|rel| (rel, p.clone())))
            .collect();
        let mut found = HashMap::with_capacity(wanted.len());
        for commit in self.commits() {
            if wanted.is_empty() {
                break;
            }
            let commit = commit?;
            let touched = self.diff(commit.parent_tree, Some(commit.tree))?;
            for rel in touched {
                if let Some(path) = wanted.remove(&rel)
                    && let Some(source) = commit.source.clone()
                {
                    found.insert(path, source);
                }
            }
        }
        Ok(found)
    }

    /// The last `limit` changes, most recent first.
    ///
    /// # Errors
    ///
    /// [`Error::History`] when the history cannot be read.
    pub fn entries(&self, limit: usize) -> Result<Vec<Entry>> {
        let mut entries = Vec::new();
        for commit in self.commits().take(limit) {
            let commit = commit?;
            let files = self.diff(commit.parent_tree, Some(commit.tree))?;
            entries.push(Entry {
                id: commit.id.to_string(),
                source: commit.source,
                summary: commit.summary,
                files,
            });
        }
        Ok(entries)
    }

    fn head_tree(&self) -> Result<Option<ObjectId>> {
        match self.repo.head_tree_id() {
            Ok(id) => Ok(Some(id.detach())),
            Err(_) if self.repo.head_id().is_err() => Ok(None),
            Err(_) => Err(failed("read")),
        }
    }

    fn excludes(&self) -> Result<gix::AttributeStack<'_>> {
        let index = self.repo.index_or_empty().map_err(|_| failed("read"))?;
        self.repo
            .excludes(
                &index,
                None,
                gix::worktree::stack::state::ignore::Source::WorktreeThenIdMappingIfNotSkipped,
            )
            .map_err(|_| failed("read"))
    }

    /// Every file of a tree, by path.
    fn flatten(&self, tree: Option<ObjectId>) -> Result<HashMap<String, ObjectId>> {
        let mut files = HashMap::new();
        let Some(tree) = tree else {
            return Ok(files);
        };
        let mut stack = vec![(String::new(), tree)];
        while let Some((prefix, id)) = stack.pop() {
            for (name, entry_id, is_tree) in self.tree_entries(id)? {
                let path = join(&prefix, &name);
                if is_tree {
                    stack.push((path, entry_id));
                } else {
                    files.insert(path, entry_id);
                }
            }
        }
        Ok(files)
    }

    /// The files that differ between two trees. Subtrees with the same id are equal
    /// and never opened, so the cost follows the size of the change.
    fn diff(&self, old: Option<ObjectId>, new: Option<ObjectId>) -> Result<Vec<String>> {
        let mut changed = Vec::new();
        let mut stack = vec![(String::new(), old, new)];
        while let Some((prefix, old, new)) = stack.pop() {
            if old == new {
                continue;
            }
            let old = match old {
                Some(id) => self.tree_entries(id)?,
                None => Vec::new(),
            };
            let new = match new {
                Some(id) => self.tree_entries(id)?,
                None => Vec::new(),
            };
            let mut names: BTreeSet<&str> = old.iter().map(|e| e.0.as_str()).collect();
            names.extend(new.iter().map(|e| e.0.as_str()));
            for name in names {
                let a = old.iter().find(|e| e.0 == name);
                let b = new.iter().find(|e| e.0 == name);
                let path = join(&prefix, name);
                let tree = |e: Option<&(String, ObjectId, bool)>| e.filter(|e| e.2).map(|e| e.1);
                let blob = |e: Option<&(String, ObjectId, bool)>| e.filter(|e| !e.2).map(|e| e.1);
                if tree(a).is_some() || tree(b).is_some() {
                    stack.push((path.clone(), tree(a), tree(b)));
                }
                if blob(a) != blob(b) {
                    changed.push(path);
                }
            }
        }
        changed.sort();
        Ok(changed)
    }

    /// The entries of a tree: name, id, and whether it is a tree.
    fn tree_entries(&self, id: ObjectId) -> Result<Vec<(String, ObjectId, bool)>> {
        let tree = self
            .repo
            .find_object(id)
            .map_err(|_| failed("read"))?
            .try_into_tree()
            .map_err(|_| failed("read"))?;
        let decoded = tree.decode().map_err(|_| failed("read"))?;
        Ok(decoded
            .entries
            .iter()
            .map(|e| {
                (
                    e.filename.to_str_lossy().into_owned(),
                    e.oid.to_owned(),
                    e.mode.is_tree(),
                )
            })
            .collect())
    }

    /// The commits from the newest, following first parents.
    fn commits(&self) -> impl Iterator<Item = Result<Commit>> + '_ {
        let mut next = self.repo.head_id().ok().map(gix::Id::detach);
        std::iter::from_fn(move || {
            let id = next.take()?;
            let commit = (|| {
                let commit = self.repo.find_commit(id).map_err(|_| failed("read"))?;
                let tree = commit.tree_id().map_err(|_| failed("read"))?.detach();
                let parent = commit.parent_ids().next().map(gix::Id::detach);
                let parent_tree = match parent {
                    Some(p) => Some(
                        self.repo
                            .find_commit(p)
                            .and_then(|c| c.tree_id())
                            .map_err(|_| failed("read"))?
                            .detach(),
                    ),
                    None => None,
                };
                let message = commit.message_raw_sloppy().to_str_lossy().into_owned();
                Ok((
                    Commit {
                        id,
                        tree,
                        parent_tree,
                        source: trailer(&message),
                        summary: message.lines().next().unwrap_or_default().to_owned(),
                    },
                    parent,
                ))
            })();
            Some(commit.map(|(commit, parent)| {
                next = parent;
                commit
            }))
        })
    }
}

/// A commit, as the history reads it.
struct Commit {
    id: ObjectId,
    tree: ObjectId,
    parent_tree: Option<ObjectId>,
    source: Option<Source>,
    summary: String,
}

/// A change being prepared: the files it touches, committed together.
#[derive(Debug)]
pub struct Change<'h> {
    history: &'h History,
    source: Source,
    /// Paths relative to the vault, with `/` separators.
    paths: BTreeSet<String>,
}

impl Change<'_> {
    /// Marks `path` (absolute, or relative to the vault) as part of this change: its
    /// content on disk when committing is recorded, or its removal if it is gone.
    /// Paths outside the vault are ignored.
    pub fn touched(&mut self, path: &Path) {
        if let Some(rel) = relative(&self.history.root, path) {
            self.paths.insert(rel);
        }
    }

    /// True when nothing was touched.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.paths.is_empty()
    }

    /// The history this change goes into.
    #[must_use]
    pub fn history(&self) -> &History {
        self.history
    }

    /// Records the touched files as one commit, with `summary` as its first line and
    /// the source as trailer. Ignored files (the journal…) are left out. Returns
    /// `None` when nothing differs from the last change.
    ///
    /// # Errors
    ///
    /// [`Error::Io`] when a file cannot be read; [`Error::History`] when the history
    /// cannot be updated.
    pub fn commit(self, summary: &str) -> Result<Option<Entry>> {
        let history = self.history;
        let repo = &history.repo;
        let base = history.head_tree()?;
        let mut excludes = history.excludes()?;
        let mut editor = repo
            .edit_tree(base.unwrap_or_else(|| ObjectId::empty_tree(gix::hash::Kind::Sha1)))
            .map_err(|_| failed("update"))?;
        let mut files = Vec::new();
        let mut blobs = Blobs::new(repo.git_dir().join("objects"));
        for rel in &self.paths {
            let ignored = excludes
                .at_path(rel.as_str(), Some(gix::index::entry::Mode::FILE))
                .map_err(|_| failed("read"))?
                .is_excluded();
            if ignored {
                continue;
            }
            let path = history.root.join(rel);
            match std::fs::read(&path) {
                Ok(bytes) => {
                    let blob = blobs.write(&bytes)?;
                    editor
                        .upsert(rel.as_str(), EntryKind::Blob, blob)
                        .map_err(|_| failed("update"))?;
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    editor.remove(rel.as_str()).map_err(|_| failed("update"))?;
                }
                Err(source) => {
                    return Err(Error::Io {
                        action: "read",
                        path,
                        source,
                    });
                }
            }
            files.push(rel.clone());
        }
        let tree = editor.write().map_err(|_| failed("update"))?.detach();
        let unchanged = match base {
            Some(base) => base == tree,
            None => tree == ObjectId::empty_tree(gix::hash::Kind::Sha1),
        };
        if unchanged {
            return Ok(None);
        }
        let files = history.diff(base, Some(tree))?;

        let signature = gix::actor::Signature {
            name: "Ipse".into(),
            email: "ipse@localhost".into(),
            time: gix::date::Time::now_local_or_utc(),
        };
        let mut time = gix::date::parse::TimeBuf::default();
        let signature = signature.to_ref(&mut time);
        let message = format!("{summary}\n\n{TRAILER} {}\n", self.source);
        let parents: Vec<ObjectId> = repo
            .head_id()
            .ok()
            .map(gix::Id::detach)
            .into_iter()
            .collect();
        let id = repo
            .commit_as(signature, signature, "HEAD", message, tree, parents)
            .map_err(|_| failed("update"))?
            .detach();

        // Keeps `git status` clean for a person who uses git on this folder.
        let mut index = repo.index_from_tree(&tree).map_err(|_| failed("update"))?;
        index
            .write(gix::index::write::Options::default())
            .map_err(|_| failed("update"))?;

        Ok(Some(Entry {
            id: id.to_string(),
            source: Some(self.source),
            summary: summary.to_owned(),
            files,
        }))
    }
}

/// Writes file contents as git loose objects, reusing one compressor.
///
/// `gix` makes a new compressor for every object, and setting one up costs more than
/// compressing a small Markdown file: with many files in one change, that was most of
/// the time. The loose object format is git's oldest and simplest: the zlib of
/// `blob <len>\0` followed by the content, in `objects/<2 hex>/<38 hex>`.
struct Blobs {
    objects: PathBuf,
    compress: gix::zlib::stream::deflate::Compress,
    input: Vec<u8>,
    output: Vec<u8>,
}

impl Blobs {
    fn new(objects: PathBuf) -> Self {
        Self {
            objects,
            compress: gix::zlib::stream::deflate::Compress::new(gix::zlib::Compression::default()),
            input: Vec::new(),
            output: Vec::new(),
        }
    }

    /// Writes `content` as a blob unless it is already there; returns its id.
    fn write(&mut self, content: &[u8]) -> Result<ObjectId> {
        let id = gix::objs::compute_hash(gix::hash::Kind::Sha1, gix::objs::Kind::Blob, content)
            .map_err(|_| failed("update"))?;
        let hex = id.to_hex().to_string();
        let dir = self.objects.join(&hex[..2]);
        let path = dir.join(&hex[2..]);
        if path.exists() {
            return Ok(id);
        }

        self.input.clear();
        self.input
            .extend_from_slice(format!("blob {}\0", content.len()).as_bytes());
        self.input.extend_from_slice(content);
        // Large enough for any zlib output of this input.
        self.output.clear();
        self.output
            .resize(self.input.len() + self.input.len() / 1000 + 128, 0);
        self.compress.reset();
        let status = self
            .compress
            .compress(
                &self.input,
                &mut self.output,
                gix::zlib::stream::deflate::FlushCompress::Finish,
            )
            .map_err(|_| failed("update"))?;
        if status != gix::zlib::Status::StreamEnd {
            return Err(failed("update"));
        }
        let written = usize::try_from(self.compress.total_out()).map_err(|_| failed("update"))?;
        self.output.truncate(written);

        std::fs::create_dir_all(&dir).map_err(|source| Error::Io {
            action: "create",
            path: dir.clone(),
            source,
        })?;
        let mut tmp = tempfile::NamedTempFile::new_in(&dir).map_err(|_| failed("update"))?;
        std::io::Write::write_all(&mut tmp, &self.output).map_err(|_| failed("update"))?;
        match tmp.persist_noclobber(&path) {
            Ok(_) => Ok(id),
            // Written meanwhile by another process: same content, same id.
            Err(e) if e.error.kind() == std::io::ErrorKind::AlreadyExists => Ok(id),
            Err(_) => Err(failed("update")),
        }
    }
}

/// Adds to the vault's `.gitignore` the lines it lacks, keeping the person's own.
fn ensure_ignored(root: &Path) -> Result<()> {
    let path = root.join(".gitignore");
    let current = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(source) => {
            return Err(Error::Io {
                action: "read",
                path,
                source,
            });
        }
    };
    let present: BTreeSet<&str> = current.lines().map(str::trim).collect();
    let missing: Vec<&str> = IGNORED
        .iter()
        .copied()
        .filter(|l| !present.contains(l))
        .collect();
    if missing.is_empty() {
        return Ok(());
    }
    let mut text = current.clone();
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    if current.is_empty() {
        text.push_str("# Not part of the memory's history (docs/spec/vault.md, section 9).\n");
    }
    for line in missing {
        text.push_str(line);
        text.push('\n');
    }
    write_atomic(&path, text.as_bytes())
}

/// The source in a commit message's `Ipse-Source:` trailer.
fn trailer(message: &str) -> Option<Source> {
    message
        .lines()
        .rev()
        .find_map(|l| l.trim().strip_prefix(TRAILER))
        .and_then(Source::parse)
}

/// `path` relative to `root`, with `/` separators, if it is inside.
fn relative(root: &Path, path: &Path) -> Option<String> {
    if path.is_absolute() {
        git_path(root, path)
    } else {
        git_path(root, &root.join(path))
    }
}

/// `path` (inside `root`) relative to it, with `/` separators. `None` outside `root`
/// or for a path going up with `..`.
fn git_path(root: &Path, path: &Path) -> Option<String> {
    let rel = path.strip_prefix(root).ok()?;
    let mut parts = Vec::new();
    for component in rel.components() {
        match component {
            Component::Normal(part) => parts.push(part.to_str()?.to_owned()),
            Component::CurDir => {}
            _ => return None,
        }
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

fn join(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.to_owned()
    } else {
        format!("{prefix}/{name}")
    }
}

fn failed(action: &'static str) -> Error {
    Error::History { action }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sources() {
        for source in [
            Source::Person,
            Source::Ipse,
            Source::Connector("calendar-2".into()),
            Source::Proposal("01jcx8".into()),
        ] {
            assert_eq!(Source::parse(&source.to_string()), Some(source));
        }
        for bad in ["", "robot", "connector:", "proposal"] {
            assert_eq!(Source::parse(bad), None, "{bad}");
        }
        assert_eq!(
            trailer("Import\n\nIpse-Source: connector:contacts\n"),
            Some(Source::Connector("contacts".into()))
        );
        assert_eq!(trailer("A commit made with git\n"), None);
    }

    #[test]
    fn paths_are_relative_with_slashes() {
        let root = Path::new("/vault");
        assert_eq!(
            git_path(root, Path::new("/vault/people/marie.md")).as_deref(),
            Some("people/marie.md")
        );
        assert_eq!(git_path(root, Path::new("/elsewhere/x.md")), None);
        assert_eq!(git_path(root, Path::new("/vault")), None);
        assert_eq!(
            relative(root, Path::new("events/2026/x.md")).as_deref(),
            Some("events/2026/x.md")
        );
        assert_eq!(relative(root, Path::new("../x.md")), None);
    }
}
