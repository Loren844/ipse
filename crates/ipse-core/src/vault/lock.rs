//! One writer at a time on a vault, across processes.
//!
//! The app (with its background sync), `ipse mcp` (started by an AI app) and the
//! command line can all change the same vault. Each change of the files and its commit
//! happen under an exclusive lock on `.ipse/write.lock`, so that one never builds on a
//! history another is still writing.
//!
//! The lock is taken again freely by the thread that holds it: a change may record the
//! person's edits first, or accepting a proposal may make its change, without waiting
//! on itself. Another thread, of this process or another one, waits.

use std::cell::RefCell;
use std::collections::HashMap;
use std::fs::{File, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::{Error, Result};

/// The lock file, relative to the vault.
pub(crate) const LOCK_FILE: &str = ".ipse/write.lock";

/// How long a writer waits for another one before giving up. A change of the vault
/// takes milliseconds; a writer still holding the lock after this is stuck.
const WAIT: Duration = Duration::from_secs(30);

/// How long to sleep between two tries.
const RETRY: Duration = Duration::from_millis(10);

thread_local! {
    /// The vaults this thread holds the lock of, with how many guards are alive and
    /// the locked file.
    static HELD: RefCell<HashMap<PathBuf, (usize, Option<File>)>> = RefCell::new(HashMap::new());
}

/// The right to write the vault, until dropped.
#[derive(Debug)]
#[must_use = "the lock is released when the guard is dropped"]
pub struct WriteLock {
    key: PathBuf,
}

impl WriteLock {
    /// Takes the write lock of the vault in `root`, waiting for another writer to
    /// finish. Taken again by the thread that holds it, it returns at once.
    ///
    /// On a file system that cannot lock files (some network shares), the change goes
    /// on without the lock, as before it existed.
    ///
    /// # Errors
    ///
    /// [`Error::Io`] when `.ipse/` cannot be created or the lock file cannot be opened;
    /// [`Error::Refused`] when another writer keeps the lock longer than 30 seconds.
    pub fn acquire(root: &Path) -> Result<Self> {
        let key = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_owned());
        let again = HELD.with_borrow_mut(|held| match held.get_mut(&key) {
            Some((count, _)) => {
                *count += 1;
                true
            }
            None => false,
        });
        if again {
            return Ok(Self { key });
        }
        let file = lock(root)?;
        HELD.with_borrow_mut(|held| held.insert(key.clone(), (1, file)));
        Ok(Self { key })
    }
}

impl Drop for WriteLock {
    fn drop(&mut self) {
        HELD.with_borrow_mut(|held| {
            if let Some((count, _)) = held.get_mut(&self.key) {
                *count -= 1;
                if *count == 0 {
                    // Closing the file releases the lock.
                    held.remove(&self.key);
                }
            }
        });
    }
}

/// Opens and locks `.ipse/write.lock`. `None` when the file system cannot lock.
fn lock(root: &Path) -> Result<Option<File>> {
    let path = root.join(LOCK_FILE);
    let io = |action, source| Error::Io {
        action,
        path: path.clone(),
        source,
    };
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| io("create", e))?;
    }
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&path)
        .map_err(|e| io("open", e))?;
    let start = Instant::now();
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(Some(file)),
            Err(TryLockError::WouldBlock) if start.elapsed() < WAIT => std::thread::sleep(RETRY),
            Err(TryLockError::WouldBlock) => {
                return Err(Error::Refused(
                    "Another Ipse is still changing this memory. Try again in a moment; if it lasts, quit the other Ipse (the app, ipse mcp or a command)."
                        .to_owned(),
                ));
            }
            Err(TryLockError::Error(_)) => return Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn the_thread_holding_the_lock_takes_it_again() {
        let dir = tempfile::tempdir().unwrap();
        let first = WriteLock::acquire(dir.path()).unwrap();
        let second = WriteLock::acquire(dir.path()).unwrap();
        drop(first);
        drop(second);
        assert!(HELD.with_borrow(HashMap::is_empty));
    }

    #[test]
    fn another_thread_waits_for_the_lock() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_owned();
        let guard = WriteLock::acquire(&root).unwrap();
        let (sent, got) = std::sync::mpsc::channel();
        let other = std::thread::spawn(move || {
            let _guard = WriteLock::acquire(&root).unwrap();
            sent.send(()).unwrap();
        });
        assert!(got.recv_timeout(Duration::from_millis(200)).is_err());
        drop(guard);
        got.recv_timeout(Duration::from_secs(5)).unwrap();
        other.join().unwrap();
    }
}
