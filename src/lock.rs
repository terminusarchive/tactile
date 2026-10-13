use std::fs::{File, TryLockError};
use std::io;
use std::path::Path;

/// Takes `tactile.lock` in `dir` and returns the file; the lock lasts as long as it
/// stays open. Fails with `WouldBlock` when another tactile already holds it.
pub fn acquire(dir: &Path) -> io::Result<File> {
    let file = File::options()
        .write(true)
        .create(true)
        .truncate(false)
        .open(dir.join("tactile.lock"))?;
    match file.try_lock() {
        Ok(()) => Ok(file),
        Err(TryLockError::WouldBlock) => Err(io::Error::new(
            io::ErrorKind::WouldBlock,
            "tactile is already running",
        )),
        Err(TryLockError::Error(error)) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("tactile-lock-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn second_acquire_reports_already_running() {
        let dir = temp_dir("held");
        let _first = acquire(&dir).unwrap();
        let error = acquire(&dir).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::WouldBlock);
        assert_eq!(error.to_string(), "tactile is already running");
    }

    #[test]
    fn leftover_lock_file_from_a_dead_process_does_not_block() {
        let dir = temp_dir("stale");
        std::fs::write(dir.join("tactile.lock"), "12345").unwrap();
        assert!(acquire(&dir).is_ok());
    }
}
