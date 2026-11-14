//! The segments of a source: one per month, open (`<YYYY-MM>.jsonl`) or sealed
//! (`<YYYY-MM>.jsonl.zst` and its `.sha256`), as `docs/spec/journal.md` sections 1, 2
//! and 5 say.

use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Read as _, Seek as _, SeekFrom, Write as _};
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
        return false;
    };
    let Some(expected) = expected.split_whitespace().next() else {
        return false;
    };
    sha256_of(&segment.path).is_some_and(|actual| actual.eq_ignore_ascii_case(expected))
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
    let file = BufReader::with_capacity(64 * 1024, File::open(&segment.path)?);
    if !segment.sealed {
        return Ok(Box::new(file));
    }
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

/// Seals the open segment `open`: compresses it, with what an earlier seal of the same
/// month holds, writes the checksum, then deletes the open file. Returns `false`, and
/// changes nothing, when an earlier sealed file of that month does not match its
/// checksum: it is reported, never overwritten.
pub(crate) fn seal(open: &Segment, earlier: Option<&Segment>) -> Result<bool> {
    let io = |action, source| Error::Io {
        action,
        path: open.path.clone(),
        source,
    };
    let mut content = match earlier {
        None => Vec::new(),
        Some(sealed) => {
            if !verify(sealed) {
                return Ok(false);
            }
            match decompressed(sealed) {
                Some(content) => content,
                None => return Ok(false),
            }
        }
    };
    content.extend(std::fs::read(&open.path).map_err(|e| io("read", e))?);
    let compressed = ruzstd::encoding::compress_to_vec(
        content.as_slice(),
        ruzstd::encoding::CompressionLevel::Fastest,
    );
    let mut sealed_name = open.path.as_os_str().to_owned();
    sealed_name.push(".zst");
    let sealed = PathBuf::from(sealed_name);
    let mut hasher = Sha256::new();
    hasher.update(&compressed);
    let name = sealed
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    // The checksum first: a sealed file without its checksum is not trusted, and the
    // open file stays until both are on disk.
    write_atomic(
        &checksum_path(&sealed),
        format!("{}  {name}\n", hex(&hasher.finalize())).as_bytes(),
    )?;
    write_atomic(&sealed, &compressed)?;
    std::fs::remove_file(&open.path).map_err(|e| io("remove", e))?;
    Ok(true)
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
}
