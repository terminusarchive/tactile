use std::fmt;
use std::process::Command;

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

fn run_command(program: &str, args: &[&str]) -> Result<String, HyprlandError> {
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|e| HyprlandError(format!("cannot run {program}: {e}")))?;
    if !output.status.success() {
        return Err(HyprlandError(format!(
            "{program} {} failed: {}",
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
}
