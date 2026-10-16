use crate::compositor::{CompositorError, Setup};
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

/// How long a stuck compositor may hold up a key press.
const SOCKET_TIMEOUT: Duration = Duration::from_millis(100);

/// Asks tachyon at `path` for the keyboard names and the focused monitor. A tachyon
/// that predates `core.keyboard` gets empty names, which libxkbcommon fills in.
pub fn query(path: &Path) -> Result<Setup, CompositorError> {
    let keyboard = match ask(path, "core.keyboard") {
        Err(Reply::Error(message)) if message.starts_with("unknown action") => Value::Null,
        other => other.map_err(Reply::describe)?,
    };
    let name = |key: &str| keyboard[key].as_str().unwrap_or_default().to_string();
    let windows = ask(path, "core.windows").map_err(Reply::describe)?;
    let outputs = ask(path, "core.outputs").map_err(Reply::describe)?;
    Ok(Setup {
        layout: name("layout"),
        variant: name("variant"),
        options: name("options"),
        focused_monitor: focused_monitor(&windows, &outputs)?,
    })
}

/// True unless tachyon answers that the session is unlocked. Any failure counts as
/// locked, so nothing is shown when the answer cannot be had.
pub fn locked(path: &Path) -> bool {
    !ask(path, "core.session").is_ok_and(|session| session["locked"] == false)
}

/// Why a request got no value.
enum Reply {
    /// tachyon answered `{"error": ...}`.
    Error(String),
    /// The socket failed, ran out of time, or the reply was not JSON of the shape.
    Broken(String),
}

impl Reply {
    fn describe(self) -> CompositorError {
        let (Reply::Error(text) | Reply::Broken(text)) = self;
        CompositorError(format!("tachyon is not running or not answering: {text}"))
    }
}

/// Sends one request and returns the `ok` value of the one-line reply.
fn ask(path: &Path, action: &str) -> Result<Value, Reply> {
    let broken = |e: std::io::Error| Reply::Broken(e.to_string());
    let mut socket = UnixStream::connect(path).map_err(broken)?;
    socket
        .set_read_timeout(Some(SOCKET_TIMEOUT))
        .map_err(broken)?;
    socket
        .set_write_timeout(Some(SOCKET_TIMEOUT))
        .map_err(broken)?;
    writeln!(socket, "{}", json!({ "action": action, "args": [] })).map_err(broken)?;
    let mut line = String::new();
    BufReader::new(socket)
        .read_line(&mut line)
        .map_err(broken)?;
    let mut reply: Value = serde_json::from_str(&line)
        .map_err(|e| Reply::Broken(format!("cannot read the reply to {action}: {e}")))?;
    if let Some(message) = reply["error"].as_str() {
        return Err(Reply::Error(message.to_string()));
    }
    match reply.get_mut("ok") {
        Some(value) => Ok(value.take()),
        None => Err(Reply::Broken(format!("the reply to {action} has no ok"))),
    }
}

/// Names the output of the focused window; with no focused window, the first output.
fn focused_monitor(windows: &Value, outputs: &Value) -> Result<String, CompositorError> {
    let outputs = outputs.as_array().map_or(&[][..], Vec::as_slice);
    let focused = windows
        .as_array()
        .and_then(|list| list.iter().find(|window| window["is_focused"] == true))
        .map(|window| &window["output"]);
    outputs
        .iter()
        .find(|output| focused.is_none_or(|id| &output["output"] == id))
        .and_then(|output| output["name"].as_str())
        .map(str::to_string)
        .ok_or_else(|| CompositorError("tachyon lists no output for the focused window".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixListener;
    use std::path::PathBuf;

    const KEYBOARD: &str = r#"{"ok":{"rules":"","model":"","layout":"de","variant":"nodeadkeys","options":"caps:escape"}}"#;
    const WINDOWS: &str =
        r#"{"ok":[{"id":7,"output":1,"is_focused":false},{"id":8,"output":2,"is_focused":true}]}"#;
    const OUTPUTS: &str = r#"{"ok":[{"output":1,"name":"DP-1"},{"output":2,"name":"eDP-1"}]}"#;
    const UNLOCKED: &str = r#"{"ok":{"locked":false}}"#;

    /// Serves `answer(action)` to each of `count` connections; None closes without
    /// replying, and a `slow` reply comes after the 100 ms limit.
    fn serve(
        name: &str,
        count: usize,
        slow: bool,
        answer: fn(&str) -> Option<&'static str>,
    ) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "tactile-tachyon-{name}-{}.sock",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let listener = UnixListener::bind(&path).unwrap();
        std::thread::spawn(move || {
            for stream in listener.incoming().take(count) {
                let mut stream = stream.unwrap();
                let mut line = String::new();
                BufReader::new(stream.try_clone().unwrap())
                    .read_line(&mut line)
                    .unwrap();
                let request: serde_json::Value = serde_json::from_str(&line).unwrap();
                if slow {
                    std::thread::sleep(Duration::from_millis(400));
                }
                if let Some(reply) = answer(request["action"].as_str().unwrap()) {
                    let _ = writeln!(stream, "{reply}");
                }
            }
        });
        path
    }

    fn healthy(action: &str) -> Option<&'static str> {
        Some(match action {
            "core.keyboard" => KEYBOARD,
            "core.windows" => WINDOWS,
            "core.outputs" => OUTPUTS,
            "core.session" => UNLOCKED,
            _ => unreachable!(),
        })
    }

    #[test]
    fn reads_the_layout_and_the_focused_windows_output_by_name() {
        let path = serve("query", 3, false, healthy);
        assert_eq!(
            query(&path),
            Ok(Setup {
                layout: "de".into(),
                variant: "nodeadkeys".into(),
                options: "caps:escape".into(),
                focused_monitor: "eDP-1".into(),
            })
        );
    }

    #[test]
    fn a_tachyon_without_core_keyboard_gets_the_default_names() {
        let path = serve("old", 3, false, |action| match action {
            "core.keyboard" => Some(r#"{"error":"unknown action \"core.keyboard\""}"#),
            other => healthy(other),
        });
        let setup = query(&path).unwrap();
        assert_eq!(
            (
                setup.layout.as_str(),
                setup.variant.as_str(),
                setup.options.as_str()
            ),
            ("", "", "")
        );
        assert_eq!(setup.focused_monitor, "eDP-1");
    }

    #[test]
    fn any_other_error_is_an_error() {
        let path = serve("error", 1, false, |_| Some(r#"{"error":"boom"}"#));
        assert!(query(&path).unwrap_err().to_string().contains("boom"));
    }

    #[test]
    fn unlocked_is_not_locked() {
        let path = serve("unlocked", 1, false, healthy);
        assert!(!locked(&path));
    }

    #[test]
    fn a_locked_reply_is_locked() {
        let path = serve("locked", 1, false, |_| Some(r#"{"ok":{"locked":true}}"#));
        assert!(locked(&path));
    }

    #[test]
    fn a_closed_socket_is_locked() {
        let path = serve("closed", 1, false, |_| None);
        assert!(locked(&path));
    }

    #[test]
    fn a_missing_socket_is_locked() {
        assert!(locked(Path::new("/nonexistent/tachyon.sock")));
    }

    #[test]
    fn an_answer_slower_than_100_ms_is_locked() {
        let path = serve("slow", 1, true, healthy);
        assert!(locked(&path));
    }

    #[test]
    fn a_reply_that_is_not_the_locked_object_is_locked() {
        let path = serve("odd", 1, false, |_| Some(r#"{"ok":null}"#));
        assert!(locked(&path));
    }
}
