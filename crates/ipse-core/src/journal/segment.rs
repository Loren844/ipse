//! The segments of a source: one per month, open (`<YYYY-MM>.jsonl`) or sealed
//! (`<YYYY-MM>.jsonl.zst` and its `.sha256`), as `docs/spec/journal.md` sections 1, 2
//! and 5 say.

use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Seek as _, SeekFrom, Write as _};
use std::path::{Path, PathBuf};

use sha2::{Digest as _, Sha256};

use crate::vault::write_atomic;
use crate::{Error, Result};

/// One segment file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Segment {
    pub(crate) path: PathBuf,
    /// `(year, month)`.
    pub(crate) month: (i16, i8),
    pub(crate) sealed: bool,
}

impl Segment {
    /// Reads `name` as a segment: `2026-11.jsonl` or `2026-11.jsonl.zst`.
    fn of(path: PathBuf) -> Option<Self> {
        let name = path.file_name()?.to_str()?;
        let (stem, sealed) = match name.strip_suffix(".jsonl.zst") {
            Some(stem) => (stem, true),
            None => (name.strip_suffix(".jsonl")?, false),
        };
        let (year, month) = stem.split_once('-')?;
        let month = (year.parse().ok()?, month.parse().ok()?);
        (stem.len() == 7 && (1..=12).contains(&month.1)).then_some(Self {
            path,
            month,
            sealed,
        })
    }

    /// The checksum file of a sealed segment.
    pub(crate) fn checksum_path(&self) -> PathBuf {
        checksum_path(&self.path)
    }
}

fn checksum_path(sealed: &Path) -> PathBuf {
    let mut name = sealed.as_os_str().to_owned();
    name.push(".sha256");
    PathBuf::from(name)
}

/// The segments of the source in `dir`, oldest month first and, within a month, the
/// sealed one before the open one: the order records were written in.
pub(crate) fn list(dir: &Path) -> Result<Vec<Segment>> {
    let years = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => {
            return Err(Error::Io {
                action: "read",
                path: dir.to_owned(),
                source,
            });
        }
    };
    let mut out = Vec::new();
    for year in years.flatten() {
        if !year.file_type().is_ok_and(|t| t.is_dir()) {
            continue;
        }
        let Ok(months) = std::fs::read_dir(year.path()) else {
            continue;
        };
        out.extend(months.flatten().filter_map(|m| Segment::of(m.path())));
    }
    out.sort_by_key(|s| (s.month, !s.sealed));
    Ok(out)
}

/// True when a sealed segment matches its checksum.
pub(crate) fn verify(segment: &Segment) -> bool {
    let Ok(expected) = std::fs::read_to_string(segment.checksum_path()) else {
        // A first seal stopped before its checksum was written.
        return sha256_of(&segment.path).is_some_and(|actual| {
            marker(&segment.path).is_some_and(|(checksum, _)| checksum == actual)
        });
    };
    let Some(expected) = expected.split_whitespace().next() else {
        return false;
    };
    let Some(actual) = sha256_of(&segment.path) else {
        return false;
    };
    // A seal stopped before its checksum was written: its marker vouches for it.
    actual.eq_ignore_ascii_case(expected)
        || marker(&segment.path).is_some_and(|(checksum, _)| checksum == actual)
}

fn sha256_of(path: &Path) -> Option<String> {
    let mut file = File::open(path).ok()?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        let n = file.read(&mut buffer).ok()?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    Some(hex(&hasher.finalize()))
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}

/// A reader of a segment's lines; a sealed one is decompressed as it is read.
pub(crate) fn open(segment: &Segment) -> std::io::Result<Box<dyn BufRead + Send>> {
    if !segment.sealed {
        let mut file = File::open(&segment.path)?;
        // What a stopped seal already put in the sealed file is read from there.
        let skip = consumed(&segment.path);
        if skip > 0 {
            file.seek(SeekFrom::Start(skip))?;
        }
        return Ok(Box::new(BufReader::with_capacity(64 * 1024, file)));
    }
    let file = BufReader::with_capacity(64 * 1024, File::open(&segment.path)?);
    let decoder = ruzstd::decoding::StreamingDecoder::new(file)
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "not zstd"))?;
    Ok(Box::new(BufReader::with_capacity(64 * 1024, decoder)))
}

/// The whole content of a sealed segment, decompressed.
fn decompressed(segment: &Segment) -> Option<Vec<u8>> {
    let mut reader = open(segment).ok()?;
    let mut out = Vec::new();
    reader.read_to_end(&mut out).ok()?;
    Some(out)
}

/// The marker of a seal in progress: `<sealed>.sealing`, holding the checksum of the
/// new sealed file and how many bytes of the open file it took. Written before the new
/// sealed file replaces the old one, removed once the open file is gone.
fn marker_path(sealed: &Path) -> PathBuf {
    let mut name = sealed.as_os_str().to_owned();
    name.push(".sealing");
    PathBuf::from(name)
}

/// The sealed file the open segment at `open` is sealed into.
fn sealed_path(open: &Path) -> PathBuf {
    let mut name = open.as_os_str().to_owned();
    name.push(".zst");
    PathBuf::from(name)
}

/// The checksum and the length a seal's marker holds, when it is there and readable.
fn marker(sealed: &Path) -> Option<(String, u64)> {
    let text = std::fs::read_to_string(marker_path(sealed)).ok()?;
    let mut words = text.split_whitespace();
    let checksum = words.next()?.to_owned();
    let length = words.next()?.parse().ok()?;
    Some((checksum, length))
}

/// How many bytes at the start of the open segment `open` are already in its sealed
/// file: a seal stopped after the new sealed file was in place, before the open file
/// was removed. 0 otherwise.
fn consumed(open: &Path) -> u64 {
    let sealed = sealed_path(open);
    match marker(&sealed) {
        Some((checksum, length)) if sha256_of(&sealed).is_some_and(|h| h == checksum) => length,
        _ => 0,
    }
}

/// Removes the marker of an earlier seal into the sealed file of `open`, before a new
/// open file of that month is created: the bytes it counts were another file's.
///
/// # Errors
///
/// [`Error::Io`] when it exists and cannot be removed.
pub(crate) fn forget_sealing(open: &Path) -> Result<()> {
    let path = marker_path(&sealed_path(open));
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(Error::Io {
            action: "remove",
            path,
            source,
        }),
    }
}

/// Seals the open segment `open`: compresses it, after what an earlier seal of the same
/// month holds, as a stream, then deletes the open file. Returns `false`, and changes
/// nothing, when an earlier sealed file of that month does not match its checksum: it
/// is reported, never overwritten.
///
/// A crash at any step leaves a month the next seal completes. The new sealed file is
/// written aside; a marker then records its checksum and how much of the open file it
/// holds; it replaces the old sealed file; then the checksum, then the open file and
/// the marker go. Until the checksum is written, the marker vouches for the sealed
/// file, and readers skip what of the open file it already holds.
pub(crate) fn seal(open: &Segment, earlier: Option<&Segment>) -> Result<bool> {
    let io = |action, path: &Path, source| Error::Io {
        action,
        path: path.to_owned(),
        source,
    };
    if let Some(sealed) = earlier
        && !verify(sealed)
    {
        return Ok(false);
    }
    let sealed = sealed_path(&open.path);
    let skip = consumed(&open.path);
    let length = std::fs::metadata(&open.path)
        .map_err(|e| io("read", &open.path, e))?
        .len();
    let checksum = if skip > 0 && skip >= length {
        // Stopped after the sealed file was in place: nothing new to add to it.
        marker(&sealed)
            .map(|(checksum, _)| checksum)
            .unwrap_or_default()
    } else {
        let previous: Box<dyn Read> = match earlier {
            Some(segment) => {
                Box::new(self::open(segment).map_err(|e| io("read", &segment.path, e))?)
            }
            None => Box::new(std::io::empty()),
        };
        let mut file = File::open(&open.path).map_err(|e| io("read", &open.path, e))?;
        file.seek(SeekFrom::Start(skip))
            .map_err(|e| io("read", &open.path, e))?;
        let source = Failing::new(previous.chain(file.take(length - skip)));
        let dir = sealed.parent().unwrap_or(Path::new("."));
        let tmp = tempfile::Builder::new()
            .prefix(".ipse-")
            .suffix(".tmp")
            .tempfile_in(dir)
            .map_err(|e| io("prepare", &sealed, e))?;
        let mut target = Failing::new(Hashing {
            file: tmp.as_file(),
            hasher: Sha256::new(),
        });
        let mut source = source;
        ruzstd::encoding::compress(
            &mut source,
            &mut target,
            ruzstd::encoding::CompressionLevel::Fastest,
        );
        if let Some(e) = source.error.take() {
            return Err(io("read", &open.path, e));
        }
        if let Some(e) = target.error.take() {
            return Err(io("write", &sealed, e));
        }
        tmp.as_file()
            .sync_all()
            .map_err(|e| io("write", &sealed, e))?;
        let checksum = hex(&target.inner.hasher.finalize());
        write_atomic(
            &marker_path(&sealed),
            format!("{checksum} {length}\n").as_bytes(),
        )?;
        tmp.persist(&sealed)
            .map_err(|e| io("write", &sealed, e.error))?;
        #[cfg(unix)]
        File::open(dir)
            .and_then(|d| d.sync_all())
            .map_err(|e| io("write", &sealed, e))?;
        checksum
    };
    let name = sealed
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    write_atomic(
        &checksum_path(&sealed),
        format!("{checksum}  {name}\n").as_bytes(),
    )?;
    std::fs::remove_file(&open.path).map_err(|e| io("remove", &open.path, e))?;
    forget_sealing(&open.path)?;
    Ok(true)
}

/// A reader or a writer that keeps its first error and then reads nothing more, or
/// takes every byte without writing it: the compressor panics on an error, so it never
/// sees one; the error is returned after.
struct Failing<T> {
    inner: T,
    error: Option<std::io::Error>,
}

impl<T> Failing<T> {
    fn new(inner: T) -> Self {
        Self { inner, error: None }
    }
}

impl<R: Read> Read for Failing<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.error.is_some() {
            return Ok(0);
        }
        match self.inner.read(buf) {
            Ok(n) => Ok(n),
            Err(e) => {
                self.error = Some(e);
                Ok(0)
            }
        }
    }
}

impl<W: std::io::Write> std::io::Write for Failing<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if self.error.is_none()
            && let Err(e) = self.inner.write_all(buf)
        {
            self.error = Some(e);
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        if self.error.is_none()
            && let Err(e) = self.inner.flush()
        {
            self.error = Some(e);
        }
        Ok(())
    }
}

/// Writes to a file and hashes what it writes.
struct Hashing<'f> {
    file: &'f File,
    hasher: Sha256,
}

impl std::io::Write for Hashing<'_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let n = self.file.write(buf)?;
        self.hasher.update(&buf[..n]);
        Ok(n)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.file.flush()
    }
}

/// Cuts an incomplete last line off the open segment at `path`, as left by a crash in
/// the middle of a write. Returns whether it did. Nothing of the line is kept or shown.
pub(crate) fn recover(path: &Path) -> Result<bool> {
    let io = |action, source| Error::Io {
        action,
        path: path.to_owned(),
        source,
    };
    let mut file = match OpenOptions::new().read(true).write(true).open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(io("open", e)),
    };
    let len = file.metadata().map_err(|e| io("read", e))?.len();
    if len == 0 {
        return Ok(false);
    }
    let mut last = [0_u8; 1];
    file.seek(SeekFrom::Start(len - 1))
        .and_then(|_| file.read_exact(&mut last))
        .map_err(|e| io("read", e))?;
    if last[0] == b'\n' {
        return Ok(false);
    }
    // Back to the end of the last whole line, by chunks.
    let mut end = len;
    let mut keep = 0;
    let mut chunk = vec![0_u8; 64 * 1024];
    while end > 0 {
        let start = end.saturating_sub(chunk.len() as u64);
        let n = usize::try_from(end - start).unwrap_or(chunk.len());
        file.seek(SeekFrom::Start(start))
            .and_then(|_| file.read_exact(&mut chunk[..n]))
            .map_err(|e| io("read", e))?;
        if let Some(at) = chunk[..n].iter().rposition(|&b| b == b'\n') {
            keep = start + at as u64 + 1;
            break;
        }
        end = start;
    }
    file.set_len(keep).map_err(|e| io("write", e))?;
    file.flush().map_err(|e| io("write", e))?;
    file.sync_data().map_err(|e| io("write", e))?;
    Ok(true)
}

/// The id of the last record of the source in `dir`: the last line of its newest
/// segment. `None` when there is none, or it cannot be read.
pub(crate) fn last_id(dir: &Path) -> Option<String> {
    #[derive(serde::Deserialize)]
    struct Id {
        id: String,
    }
    let newest = list(dir).ok()?.pop()?;
    let line = if newest.sealed {
        let content = decompressed(&newest)?;
        let text = String::from_utf8(content).ok()?;
        text.lines()
            .rev()
            .find(|l| !l.trim().is_empty())?
            .to_owned()
    } else {
        last_line(&newest.path)?
    };
    serde_json::from_str::<Id>(&line).ok().map(|r| r.id)
}

/// The last whole line of a file, read from its end.
fn last_line(path: &Path) -> Option<String> {
    let mut file = File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    let mut size = 4096_u64;
    loop {
        let start = len.saturating_sub(size);
        file.seek(SeekFrom::Start(start)).ok()?;
        let mut tail = Vec::new();
        file.read_to_end(&mut tail).ok()?;
        let text = String::from_utf8_lossy(&tail);
        let mut lines = text.lines().rev().filter(|l| !l.trim().is_empty());
        if let Some(line) = lines.next() {
            // A line cut by the start of the window is only trusted when the window
            // started at the beginning of the file, or another line follows it.
            if start == 0 || lines.next().is_some() {
                return Some(line.to_owned());
            }
        } else if start == 0 {
            return None;
        }
        size *= 4;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segment_names() {
        let s = Segment::of(PathBuf::from("j/2026/2026-11.jsonl")).unwrap();
        assert_eq!((s.month, s.sealed), ((2026, 11), false));
        let s = Segment::of(PathBuf::from("j/2026/2026-01.jsonl.zst")).unwrap();
        assert_eq!((s.month, s.sealed), ((2026, 1), true));
        for bad in [
            "2026-13.jsonl",
            "notes.jsonl",
            "2026-11.jsonl.zst.sha256",
            "2026-1.jsonl",
        ] {
            assert!(Segment::of(PathBuf::from(bad)).is_none(), "{bad}");
        }
    }

    #[test]
    fn a_torn_last_line_is_cut() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("2026-11.jsonl");
        std::fs::write(&path, "{\"a\":1}\n{\"b\":2}\n{\"c\":").unwrap();
        assert!(recover(&path).unwrap());
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "{\"a\":1}\n{\"b\":2}\n"
        );
        assert!(!recover(&path).unwrap(), "a whole file is left alone");
        std::fs::write(&path, "{\"c\":").unwrap();
        assert!(recover(&path).unwrap());
        assert_eq!(std::fs::read(&path).unwrap(), b"");
    }

    #[test]
    fn sealing_compresses_checks_and_keeps_what_does_not_match() {
        let dir = tempfile::tempdir().unwrap();
        let open = dir.path().join("2026-10.jsonl");
        std::fs::write(&open, "{\"id\":\"a\"}\n{\"id\":\"b\"}\n").unwrap();
        let segment = Segment::of(open.clone()).unwrap();
        assert!(seal(&segment, None).unwrap());
        assert!(!open.exists());
        let sealed = Segment::of(dir.path().join("2026-10.jsonl.zst")).unwrap();
        assert!(verify(&sealed));
        assert_eq!(
            decompressed(&sealed).unwrap(),
            b"{\"id\":\"a\"}\n{\"id\":\"b\"}\n"
        );
        let checksum = std::fs::read_to_string(sealed.checksum_path()).unwrap();
        assert!(
            checksum.ends_with("  2026-10.jsonl.zst\n"),
            "as sha256sum writes it"
        );
        assert_eq!(last_id(dir.path()), None, "not a source folder");

        // A late record of that month, sealed again with what was there.
        std::fs::write(&open, "{\"id\":\"c\"}\n").unwrap();
        assert!(seal(&segment, Some(&sealed)).unwrap());
        assert_eq!(
            decompressed(&sealed).unwrap(),
            b"{\"id\":\"a\"}\n{\"id\":\"b\"}\n{\"id\":\"c\"}\n"
        );

        // A damaged sealed file is never overwritten.
        let mut bytes = std::fs::read(&sealed.path).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 0xff;
        std::fs::write(&sealed.path, &bytes).unwrap();
        assert!(!verify(&sealed));
        std::fs::write(&open, "{\"id\":\"d\"}\n").unwrap();
        assert!(!seal(&segment, Some(&sealed)).unwrap());
        assert!(open.exists());
        assert_eq!(std::fs::read(&sealed.path).unwrap(), bytes);
    }

    #[test]
    fn the_last_line_is_read_from_the_end() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.jsonl");
        let long = "x".repeat(10_000);
        std::fs::write(&path, format!("{{\"id\":\"{long}\"}}\n{{\"id\":\"z\"}}\n")).unwrap();
        assert_eq!(last_line(&path).as_deref(), Some("{\"id\":\"z\"}"));
        std::fs::write(&path, format!("{{\"id\":\"{long}\"}}\n")).unwrap();
        assert_eq!(last_line(&path), Some(format!("{{\"id\":\"{long}\"}}")));
    }

    /// What a reader gets from the segments in `dir`: the sealed ones that match their
    /// checksum, then the open ones, in order.
    fn read_all(dir: &Path) -> String {
        let mut out = String::new();
        for segment in list(dir).unwrap() {
            assert!(
                !segment.sealed || verify(&segment),
                "{segment:?} is trusted"
            );
            open(&segment).unwrap().read_to_string(&mut out).unwrap();
        }
        out
    }

    /// Where a seal stops, for [`stopped`].
    #[derive(Clone, Copy, PartialEq, PartialOrd)]
    enum Step {
        /// The new sealed file is written aside.
        Aside,
        /// The marker is written.
        Marker,
        /// The new sealed file replaced the old one.
        Replaced,
        /// The checksum is written.
        Checksum,
        /// The open file is removed.
        Removed,
    }

    /// The files of month 2026-10 as a seal of `open` after `before` (already sealed,
    /// if not empty) leaves them when it stops right after `step`.
    fn stopped(dir: &Path, before: &str, open: &str, step: Step) -> Segment {
        let year = dir.join("2026");
        std::fs::create_dir_all(&year).unwrap();
        let open_path = year.join("2026-10.jsonl");
        let sealed = sealed_path(&open_path);
        let write_sealed = |content: &str, path: &Path| {
            let compressed = ruzstd::encoding::compress_to_vec(
                content.as_bytes(),
                ruzstd::encoding::CompressionLevel::Fastest,
            );
            std::fs::write(path, &compressed).unwrap();
            let mut hasher = Sha256::new();
            hasher.update(&compressed);
            hex(&hasher.finalize())
        };
        if !before.is_empty() {
            let checksum = write_sealed(before, &sealed);
            std::fs::write(
                checksum_path(&sealed),
                format!("{checksum}  2026-10.jsonl.zst\n"),
            )
            .unwrap();
        }
        std::fs::write(&open_path, open).unwrap();
        let aside = year.join(".ipse-x.tmp");
        let checksum = write_sealed(&format!("{before}{open}"), &aside);
        if step >= Step::Marker {
            std::fs::write(marker_path(&sealed), format!("{checksum} {}\n", open.len())).unwrap();
        }
        if step >= Step::Replaced {
            std::fs::rename(&aside, &sealed).unwrap();
        }
        if step >= Step::Checksum {
            std::fs::write(
                checksum_path(&sealed),
                format!("{checksum}  2026-10.jsonl.zst\n"),
            )
            .unwrap();
        }
        if step >= Step::Removed {
            std::fs::remove_file(&open_path).unwrap();
        }
        Segment::of(open_path).unwrap()
    }

    #[test]
    fn a_seal_stopped_at_any_step_is_completed_by_the_next() {
        let (a, c) = ("{\"id\":\"a\"}\n", "{\"id\":\"c\"}\n");
        let late = "{\"id\":\"d\"}\n";
        for before in ["", a] {
            for step in [Step::Aside, Step::Marker, Step::Replaced, Step::Checksum] {
                let dir = tempfile::tempdir().unwrap();
                let open_segment = stopped(dir.path(), before, c, step);
                let whole = format!("{before}{c}");
                assert_eq!(read_all(dir.path()), whole, "read once, stopped there");

                // A late record of that month, then the next seal.
                std::fs::OpenOptions::new()
                    .append(true)
                    .open(&open_segment.path)
                    .unwrap()
                    .write_all(late.as_bytes())
                    .unwrap();
                let sealed = Segment::of(sealed_path(&open_segment.path)).unwrap();
                let earlier = sealed.path.exists().then_some(&sealed);
                assert!(seal(&open_segment, earlier).unwrap(), "{before:?}");
                assert!(!open_segment.path.exists());
                assert!(!marker_path(&sealed.path).exists());
                assert!(verify(&sealed));
                assert_eq!(
                    String::from_utf8(decompressed(&sealed).unwrap()).unwrap(),
                    format!("{whole}{late}")
                );
            }
        }
    }

    #[test]
    fn a_seal_stopped_with_nothing_new_is_finished_as_it_was() {
        let dir = tempfile::tempdir().unwrap();
        let open_segment = stopped(dir.path(), "", "{\"id\":\"c\"}\n", Step::Replaced);
        let sealed = Segment::of(sealed_path(&open_segment.path)).unwrap();
        let bytes = std::fs::read(&sealed.path).unwrap();
        assert!(seal(&open_segment, Some(&sealed)).unwrap());
        assert_eq!(std::fs::read(&sealed.path).unwrap(), bytes);
        assert!(verify(&sealed));
        assert!(!open_segment.path.exists());
    }

    #[test]
    fn a_marker_left_after_the_open_file_went_does_not_count_a_new_one() {
        let dir = tempfile::tempdir().unwrap();
        let open_segment = stopped(dir.path(), "", "{\"id\":\"c\"}\n", Step::Removed);
        // A new late record of that month: a new open file.
        forget_sealing(&open_segment.path).unwrap();
        std::fs::write(&open_segment.path, "{\"id\":\"e\"}\n").unwrap();
        assert_eq!(read_all(dir.path()), "{\"id\":\"c\"}\n{\"id\":\"e\"}\n");
        let sealed = Segment::of(sealed_path(&open_segment.path)).unwrap();
        assert!(seal(&open_segment, Some(&sealed)).unwrap());
        assert_eq!(
            decompressed(&sealed).unwrap(),
            b"{\"id\":\"c\"}\n{\"id\":\"e\"}\n"
        );
    }
}
