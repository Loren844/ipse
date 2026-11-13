//! Writing a file so that a crash leaves either the old or the new version, never a mix.

use std::io::Write as _;
use std::path::{Path, PathBuf};

use crate::{Error, Result};

/// Writes `bytes` to `path` atomically: to a temporary file in the same folder, flushed
/// to disk, then renamed over `path`. Creates the folder if needed.
///
/// # Errors
///
/// [`Error::Io`] when the folder cannot be created or the file cannot be written or
/// renamed. The temporary file is removed on failure.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    write(path, bytes, true)
}

/// Writes `bytes` to `path` atomically, like [`write_atomic`], but without waiting for
/// the disk: for many files written together, made durable at once with
/// [`sync_files`].
///
/// # Errors
///
/// As [`write_atomic`].
pub fn write_atomic_unsynced(path: &Path, bytes: &[u8]) -> Result<()> {
    write(path, bytes, false)
}

fn write(path: &Path, bytes: &[u8], sync: bool) -> Result<()> {
    let io = |action: &'static str, source: std::io::Error| Error::Io {
        action,
        path: path.to_owned(),
        source,
    };
    let dir = match path.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir,
        _ => Path::new("."),
    };
    std::fs::create_dir_all(dir).map_err(|e| io("create the folder of", e))?;

    let mut tmp = tempfile::Builder::new()
        .prefix(".ipse-")
        .suffix(".tmp")
        .tempfile_in(dir)
        .map_err(|e| io("prepare", e))?;
    tmp.write_all(bytes).map_err(|e| io("write", e))?;
    if sync {
        tmp.as_file().sync_all().map_err(|e| io("write", e))?;
    }
    tmp.persist(path).map_err(|e| io("write", e.error))?;

    #[cfg(unix)]
    if sync {
        std::fs::File::open(dir)
            .and_then(|d| d.sync_all())
            .map_err(|e| io("write", e))?;
    }
    Ok(())
}

/// Waits until `paths`, written with [`write_atomic_unsynced`], and their folders are
/// on disk. The files are synced in parallel, which lets the file system group the
/// work. Files that no longer exist are skipped.
///
/// # Errors
///
/// [`Error::Io`] naming a file that could not be synced.
pub fn sync_files(paths: &[PathBuf]) -> Result<()> {
    // Windows syncs a file only through a handle that may write; nothing is written.
    let open = |path: &Path| std::fs::OpenOptions::new().write(true).open(path);
    let sync = |path: &Path| -> Result<()> {
        match open(path) {
            Ok(file) => file.sync_all().map_err(|source| Error::Io {
                action: "write",
                path: path.to_owned(),
                source,
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(source) => Err(Error::Io {
                action: "write",
                path: path.to_owned(),
                source,
            }),
        }
    };
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get().clamp(2, 16));
    let chunk = paths.len().div_ceil(threads).max(1);
    std::thread::scope(|scope| {
        let workers: Vec<_> = paths
            .chunks(chunk)
            .map(|part| scope.spawn(move || part.iter().try_for_each(|p| sync(p))))
            .collect();
        workers
            .into_iter()
            .try_for_each(|w| w.join().unwrap_or(Ok(())))
    })?;
    #[cfg(unix)]
    {
        let dirs: std::collections::BTreeSet<&Path> =
            paths.iter().filter_map(|p| p.parent()).collect();
        for dir in dirs {
            std::fs::File::open(dir)
                .and_then(|d| d.sync_all())
                .map_err(|source| Error::Io {
                    action: "write",
                    path: dir.to_owned(),
                    source,
                })?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_replaces_and_leaves_no_temporary_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("people").join("a.md");
        write_atomic(&path, b"first").unwrap();
        write_atomic(&path, b"second").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"second");
        let names: Vec<_> = std::fs::read_dir(path.parent().unwrap())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, ["a.md"]);
    }
}
