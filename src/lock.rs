use std::fs::{File, TryLockError};
use std::io::{self, Read, Write};
use std::path::Path;

/// What taking the lock came to.
#[derive(Debug)]
pub enum Outcome {
    /// This process owns the lock for as long as the file is kept open.
    Holding(File),
    /// Another tactile held it; it was sent SIGTERM and has let go.
    StoppedOther,
}

/// Takes `tactile.lock` in `dir`, or stops the tactile that holds it. After the
/// signal this waits until the lock is free, so the other process is gone on return.
pub fn acquire(dir: &Path) -> io::Result<Outcome> {
    let mut file = File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(dir.join("tactile.lock"))?;
    match file.try_lock() {
        Ok(()) => {
            file.set_len(0)?;
            write!(file, "{}", std::process::id())?;
            Ok(Outcome::Holding(file))
        }
        Err(TryLockError::WouldBlock) => {
            let pid = held_pid(&mut file)?;
            if unsafe { libc::kill(pid, libc::SIGTERM) } != 0 {
                return Err(io::Error::last_os_error());
            }
            file.lock()?;
            Ok(Outcome::StoppedOther)
        }
        Err(TryLockError::Error(error)) => Err(error),
    }
}

fn held_pid(file: &mut File) -> io::Result<libc::pid_t> {
    let mut text = String::new();
    file.read_to_string(&mut text)?;
    text.trim()
        .parse()
        .map_err(|_| io::Error::other("tactile.lock holds no process id"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Child, Command};
    use std::time::Duration;

    fn lock_path(dir: &Path) -> std::path::PathBuf {
        dir.join("tactile.lock")
    }

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("tactile-lock-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn recorded_pid(dir: &Path) -> u32 {
        std::fs::read_to_string(lock_path(dir))
            .unwrap()
            .trim()
            .parse()
            .unwrap()
    }

    /// A real process that holds the lock on the file with its own pid in it.
    fn spawn_holder(dir: &Path) -> Child {
        let path = lock_path(dir);
        let child = Command::new("sh")
            .arg("-c")
            .arg(r#"exec 9>>"$1"; flock -x 9; echo $$ > "$1"; exec sleep 60"#)
            .arg("sh")
            .arg(&path)
            .spawn()
            .unwrap();
        wait_until_held(&path);
        child
    }

    fn wait_until_held(path: &Path) {
        for _ in 0..200 {
            let probe = File::options()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(path)
                .unwrap();
            if probe.try_lock().is_err() && std::fs::metadata(path).unwrap().len() > 0 {
                return;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        panic!("holder never took the lock");
    }

    #[test]
    fn first_acquire_holds_the_lock_and_records_its_pid() {
        let dir = temp_dir("first");
        let outcome = acquire(&dir).unwrap();
        assert!(matches!(outcome, Outcome::Holding(_)));
        assert_eq!(recorded_pid(&dir), std::process::id());
    }

    #[test]
    fn acquire_stops_the_live_process_holding_the_lock() {
        let dir = temp_dir("stop");
        let mut holder = spawn_holder(&dir);
        let outcome = acquire(&dir).unwrap();
        assert!(matches!(outcome, Outcome::StoppedOther));
        let status = holder.wait().unwrap();
        assert_eq!(
            std::os::unix::process::ExitStatusExt::signal(&status),
            Some(15)
        );
    }

    #[test]
    fn leftover_lock_file_from_a_dead_process_does_not_block_or_signal() {
        let dir = temp_dir("stale");
        let mut bystander = Command::new("sleep").arg("60").spawn().unwrap();
        std::fs::write(lock_path(&dir), bystander.id().to_string()).unwrap();
        let outcome = acquire(&dir).unwrap();
        assert!(matches!(outcome, Outcome::Holding(_)));
        assert_eq!(bystander.try_wait().unwrap(), None);
        bystander.kill().unwrap();
        bystander.wait().unwrap();
    }
}
