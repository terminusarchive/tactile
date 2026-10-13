/// Where the bar sits on the monitor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Position {
    TopLeft,
    Top,
    TopRight,
    BottomLeft,
    #[default]
    Bottom,
    BottomRight,
}

const POSITIONS: [(&str, Position); 6] = [
    ("top-left", Position::TopLeft),
    ("top", Position::Top),
    ("top-right", Position::TopRight),
    ("bottom-left", Position::BottomLeft),
    ("bottom", Position::Bottom),
    ("bottom-right", Position::BottomRight),
];

/// The command line was not understood; the message says what is accepted.
#[derive(Debug, PartialEq, Eq)]
pub struct UsageError(String);

impl std::fmt::Display for UsageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for UsageError {}

/// Reads the arguments after the program name. Only `--position <value>` exists;
/// `--position=<value>`, a missing value and any other argument are usage errors.
pub fn parse_position(mut args: impl Iterator<Item = String>) -> Result<Position, UsageError> {
    let Some(flag) = args.next() else {
        return Ok(Position::default());
    };
    if flag != "--position" {
        return Err(usage(&format!("unknown argument `{flag}`")));
    }
    let Some(value) = args.next() else {
        return Err(usage("`--position` needs a value"));
    };
    if let Some(extra) = args.next() {
        return Err(usage(&format!("unexpected argument `{extra}`")));
    }
    POSITIONS
        .iter()
        .find(|(name, _)| *name == value)
        .map(|(_, position)| *position)
        .ok_or_else(|| usage(&format!("unknown position `{value}`")))
}

fn usage(problem: &str) -> UsageError {
    let names: Vec<_> = POSITIONS.iter().map(|(name, _)| *name).collect();
    UsageError(format!(
        "{problem}\nusage: tactile [--position <{}>]",
        names.join("|")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Position, UsageError> {
        parse_position(args.iter().map(|arg| arg.to_string()))
    }

    #[test]
    fn each_position_value_is_accepted() {
        let expected = [
            ("top-left", Position::TopLeft),
            ("top", Position::Top),
            ("top-right", Position::TopRight),
            ("bottom-left", Position::BottomLeft),
            ("bottom", Position::Bottom),
            ("bottom-right", Position::BottomRight),
        ];
        for (value, position) in expected {
            assert_eq!(parse(&["--position", value]), Ok(position));
        }
    }

    #[test]
    fn no_flag_means_bottom() {
        assert_eq!(parse(&[]), Ok(Position::Bottom));
    }

    #[test]
    fn bad_value_lists_the_accepted_values() {
        let message = parse(&["--position", "middle"]).unwrap_err().to_string();
        assert!(message.contains("middle"));
        for value in [
            "top-left",
            "top",
            "top-right",
            "bottom-left",
            "bottom",
            "bottom-right",
        ] {
            assert!(message.contains(value), "{message}");
        }
    }

    #[test]
    fn missing_value_equals_form_and_unknown_flag_are_usage_errors() {
        assert!(parse(&["--position"]).is_err());
        assert!(parse(&["--position=top"]).is_err());
        assert!(parse(&["--color", "red"]).is_err());
    }
}
