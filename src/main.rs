mod bar;
mod hyprland;
mod input;
mod keymap;
mod overlay;

use std::path::Path;
use std::sync::mpsc::channel;

use gtk4::glib;

const INPUT_DIR: &str = "/dev/input";

fn main() -> glib::ExitCode {
    match start() {
        Ok(exit) => exit,
        Err(message) => {
            eprintln!("tactile: {message}");
            glib::ExitCode::FAILURE
        }
    }
}

fn start() -> Result<glib::ExitCode, Box<dyn std::error::Error>> {
    let hyprland = hyprland::query()?;
    let keymap = keymap::Keymap::new(&hyprland.layout, &hyprland.variant, &hyprland.options)?;
    let (sender, events) = channel();
    input::start(Path::new(INPUT_DIR), sender)?;
    Ok(overlay::run(hyprland.focused_monitor, events, keymap))
}
