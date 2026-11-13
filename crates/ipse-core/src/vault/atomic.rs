//! Writing a file so that a crash leaves either the old or the new version, never a mix.

use std::io::Write as _;
use std::path::Path;

use crate::{Error, Result};

/// Writes `bytes` to `path` atomically: to a temporary file in the same folder, flushed
/// to disk, then renamed over `path`. Creates the folder if needed.
///
/// # Errors
///
/// [`Error::Io`] when the folder cannot be created or the file cannot be written or
/// renamed. The temporary file is removed on failure.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
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
    tmp.as_file().sync_all().map_err(|e| io("write", e))?;
    tmp.persist(path).map_err(|e| io("write", e.error))?;

    #[cfg(unix)]
    std::fs::File::open(dir)
        .and_then(|d| d.sync_all())
        .map_err(|e| io("write", e))?;
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
