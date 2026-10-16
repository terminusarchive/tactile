use crate::{hyprland, tachyon};
use std::ffi::OsString;
use std::fmt;
use std::path::{Path, PathBuf};

/// What tactile asks the compositor once at launch. Empty strings mean "unset".
#[derive(Debug, PartialEq, Eq)]
pub struct Setup {
    pub layout: String,
    pub variant: String,
    pub options: String,
    pub focused_monitor: String,
}

/// The compositor is missing, failed, or answered with something unreadable.
#[derive(Debug, PartialEq, Eq)]
pub struct CompositorError(pub String);

impl fmt::Display for CompositorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for CompositorError {}

/// The compositor tactile talks to, chosen by which one's socket is present.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Compositor {
    Hyprland,
    Tachyon(PathBuf),
}

impl Compositor {
    pub fn detect() -> Result<Compositor, CompositorError> {
        detect_with(|name| std::env::var_os(name), Path::exists)
    }

    pub fn query(&self) -> Result<Setup, CompositorError> {
        match self {
            Compositor::Hyprland => hyprland::query(),
            Compositor::Tachyon(socket) => tachyon::query(socket),
        }
    }

    /// True unless the compositor answers that the session is unlocked.
    pub fn locked(&self) -> bool {
        match self {
            Compositor::Hyprland => hyprland::locked(),
            Compositor::Tachyon(socket) => tachyon::locked(socket),
        }
    }
}

fn detect_with(
    var: impl Fn(&str) -> Option<OsString>,
    exists: impl Fn(&Path) -> bool,
) -> Result<Compositor, CompositorError> {
    let socket = var("TACHYON_SOCKET").map(PathBuf::from).or_else(|| {
        let runtime = var("XDG_RUNTIME_DIR")?;
        let display = var("WAYLAND_DISPLAY")?;
        let mut file = display;
        file.push(".sock");
        Some(PathBuf::from(runtime).join("tachyon").join(file))
    });
    if let Some(socket) = socket.filter(|socket| exists(socket)) {
        Ok(Compositor::Tachyon(socket))
    } else if var("HYPRLAND_INSTANCE_SIGNATURE").is_some() {
        Ok(Compositor::Hyprland)
    } else {
        Err(CompositorError(
            "no supported compositor found: tactile supports tachyon and Hyprland".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detect(env: &[(&str, &str)], sockets: &[&str]) -> Result<Compositor, CompositorError> {
        detect_with(
            |name| {
                env.iter()
                    .find(|(key, _)| *key == name)
                    .map(|(_, v)| (*v).into())
            },
            |path| sockets.iter().any(|socket| Path::new(socket) == path),
        )
    }

    #[test]
    fn the_tachyon_socket_from_the_environment_picks_tachyon() {
        assert_eq!(
            detect(&[("TACHYON_SOCKET", "/t.sock")], &["/t.sock"]),
            Ok(Compositor::Tachyon("/t.sock".into()))
        );
    }

    #[test]
    fn the_socket_under_the_runtime_dir_picks_tachyon() {
        let env = [
            ("XDG_RUNTIME_DIR", "/run/u"),
            ("WAYLAND_DISPLAY", "wayland-1"),
        ];
        assert_eq!(
            detect(&env, &["/run/u/tachyon/wayland-1.sock"]),
            Ok(Compositor::Tachyon("/run/u/tachyon/wayland-1.sock".into()))
        );
    }

    #[test]
    fn the_hyprland_signature_picks_hyprland_when_no_tachyon_socket_exists() {
        let env = [
            ("HYPRLAND_INSTANCE_SIGNATURE", "abc"),
            ("TACHYON_SOCKET", "/gone.sock"),
        ];
        assert_eq!(detect(&env, &[]), Ok(Compositor::Hyprland));
    }

    #[test]
    fn neither_names_the_supported_compositors() {
        let message = detect(&[], &[]).unwrap_err().to_string();
        assert!(
            message.contains("tachyon") && message.contains("Hyprland"),
            "{message}"
        );
    }
}
