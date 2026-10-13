use std::fmt;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

/// How long a stuck compositor may hold up a key press.
const SOCKET_TIMEOUT: Duration = Duration::from_millis(100);

/// What tactile asks Hyprland once at launch. Empty strings mean "unset".
#[derive(Debug, PartialEq, Eq)]
pub struct Hyprland {
    pub layout: String,
    pub variant: String,
    pub options: String,
    pub focused_monitor: String,
}

/// Hyprland is missing, failed, or answered with something unreadable.
#[derive(Debug, PartialEq, Eq)]
pub struct HyprlandError(String);

impl fmt::Display for HyprlandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for HyprlandError {}

/// Runs `hyprctl getoption input:kb_layout|kb_variant|kb_options` and
/// `hyprctl monitors`.
pub fn query() -> Result<Hyprland, HyprlandError> {
    query_with(|args| run_command("hyprctl", args))
}

fn query_with(
    run: impl Fn(&[&str]) -> Result<String, HyprlandError>,
) -> Result<Hyprland, HyprlandError> {
    let option = |name: &str| parse_option(&run(&["getoption", name])?);
    Ok(Hyprland {
        layout: option("input:kb_layout")?,
        variant: option("input:kb_variant")?,
        options: option("input:kb_options")?,
        focused_monitor: parse_focused_monitor(&run(&["monitors"])?)?,
    })
}

/// True unless Hyprland answers that the session is unlocked. Any failure counts as
/// locked, so nothing is shown when the answer cannot be had.
pub fn locked() -> bool {
    socket_path().is_none_or(|path| locked_at(&path))
}

fn socket_path() -> Option<PathBuf> {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR")?;
    let instance = std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE")?;
    Some(
        PathBuf::from(runtime)
            .join("hypr")
            .join(instance)
            .join(".socket.sock"),
    )
}

/// Asks the request socket at `path` the plain request `locked`.
fn locked_at(path: &Path) -> bool {
    !ask(path, "locked").is_ok_and(|reply| is_unlocked(&reply))
}

fn ask(path: &Path, request: &str) -> std::io::Result<String> {
    let mut socket = UnixStream::connect(path)?;
    socket.set_read_timeout(Some(SOCKET_TIMEOUT))?;
    socket.set_write_timeout(Some(SOCKET_TIMEOUT))?;
    socket.write_all(request.as_bytes())?;
    let mut reply = String::new();
    socket.read_to_string(&mut reply)?;
    Ok(reply)
}

/// Only the exact reply `false` says the session is unlocked.
fn is_unlocked(reply: &str) -> bool {
    reply.trim() == "false"
}

fn run_command(program: &str, args: &[&str]) -> Result<String, HyprlandError> {
    let not_answering = "Hyprland is not running or not answering";
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|e| HyprlandError(format!("{not_answering}: cannot run {program}: {e}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let text = [stderr.trim(), stdout.trim()]
            .into_iter()
            .find(|text| !text.is_empty())
            .map_or(String::new(), |text| format!(": {text}"));
        return Err(HyprlandError(format!(
            "{not_answering}: {program} {} failed with {}{text}",
            args.join(" "),
            output.status
        )));
    }
    String::from_utf8(output.stdout)
        .map_err(|_| HyprlandError(format!("{program} printed text that is not UTF-8")))
}

/// Reads the `str: ` line of `hyprctl getoption`.
fn parse_option(text: &str) -> Result<String, HyprlandError> {
    text.lines()
        .find_map(|line| line.strip_prefix("str: "))
        .map(str::to_string)
        .ok_or_else(|| HyprlandError(format!("cannot read hyprctl getoption output: {text:?}")))
}

/// Finds the name of the monitor whose block says `focused: yes`.
fn parse_focused_monitor(text: &str) -> Result<String, HyprlandError> {
    let mut current = None;
    for line in text.lines() {
        if let Some(header) = line.strip_prefix("Monitor ") {
            current = header.split(" (").next();
        } else if line.trim() == "focused: yes"
            && let Some(name) = current
        {
            return Ok(name.to_string());
        }
    }
    Err(HyprlandError(
        "hyprctl monitors shows no focused monitor".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Captured from `hyprctl getoption ...` and `hyprctl monitors` on Hyprland
    // 0.56.2; the monitors text is cut to the lines that matter, plus a second,
    // unfocused monitor added by hand.
    const LAYOUT: &str = "str: us\nset: true\n";
    const EMPTY: &str = "str: \nset: true\n";
    const MONITORS: &str = "Monitor HDMI-A-1 (ID 0):\n\t1920x1080@60.00000 at 3840x0\n\tfocused: no\n\nMonitor DP-1 (ID 1):\n\t3840x2160@60.00000 at 0x0\n\tactive workspace: 1 (1)\n\tfocused: yes\n\tdpmsStatus: 1\n\n";

    // Captured from the live request socket: writing `locked` to
    // $XDG_RUNTIME_DIR/hypr/$HYPRLAND_INSTANCE_SIGNATURE/.socket.sock answered `false`.
    // The locked reply is derived from it, not captured: Hyprland prints the boolean.
    const UNLOCKED_REPLY: &str = "false";
    const LOCKED_REPLY: &str = "true";

    #[test]
    fn the_unlocked_reply_is_unlocked() {
        assert!(is_unlocked(UNLOCKED_REPLY));
    }

    #[test]
    fn the_locked_reply_is_locked() {
        assert!(!is_unlocked(LOCKED_REPLY));
    }

    #[test]
    fn any_other_reply_is_locked() {
        for reply in ["", "unknown request", "{\"locked\": false}", "False"] {
            assert!(!is_unlocked(reply), "{reply:?}");
        }
    }

    #[test]
    fn a_socket_that_cannot_be_reached_is_locked() {
        assert!(locked_at(Path::new("/nonexistent/.socket.sock")));
    }

    fn fake(args: &[&str]) -> Result<String, HyprlandError> {
        let text = match args {
            ["getoption", "input:kb_layout"] => LAYOUT,
            ["getoption", _] => EMPTY,
            ["monitors"] => MONITORS,
            _ => unreachable!(),
        };
        Ok(text.to_string())
    }

    #[test]
    fn reads_layout_variant_options_and_focused_monitor() {
        assert_eq!(
            query_with(fake),
            Ok(Hyprland {
                layout: "us".into(),
                variant: "".into(),
                options: "".into(),
                focused_monitor: "DP-1".into(),
            })
        );
    }

    #[test]
    fn output_it_cannot_read_is_an_error() {
        assert!(query_with(|_| Ok("nonsense".to_string())).is_err());
    }

    #[test]
    fn missing_command_is_an_error() {
        assert!(run_command("hyprctl-that-does-not-exist", &[]).is_err());
    }

    #[test]
    fn failing_command_is_an_error() {
        assert!(run_command("false", &[]).is_err());
    }

    #[test]
    fn missing_command_says_hyprland_is_not_running() {
        let error = run_command("hyprctl-that-does-not-exist", &[]).unwrap_err();
        assert!(
            error
                .to_string()
                .starts_with("Hyprland is not running or not answering")
        );
    }

    #[test]
    fn failing_command_with_text_on_stdout_shows_it() {
        let error = run_command("sh", &["-c", "echo no socket; exit 4"]).unwrap_err();
        assert!(error.to_string().ends_with(": no socket"));
    }

    #[test]
    fn failing_command_without_text_has_no_dangling_colon() {
        let error = run_command("sh", &["-c", "exit 4"]).unwrap_err();
        assert!(!error.to_string().ends_with(':'));
        assert!(!error.to_string().ends_with(": "));
    }

    #[test]
    fn failing_command_says_hyprland_is_not_running_and_shows_its_text() {
        let error = run_command("sh", &["-c", "echo no socket >&2; exit 4"]).unwrap_err();
        let message = error.to_string();
        assert!(message.contains("Hyprland is not running or not answering"));
        assert!(message.contains("no socket"));
    }
}
