mod args;
mod bar;
mod hyprland;
mod input;
mod keymap;
mod overlay;

use gtk4::glib;
use std::path::Path;

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
    let position = args::parse_position(std::env::args().skip(1))?;
    let hyprland = hyprland::query()?;
    let keymap = keymap::Keymap::new(&hyprland.layout, &hyprland.variant, &hyprland.options)?;
    let (sender, events) = async_channel::unbounded();
    input::start(Path::new(INPUT_DIR), sender)?;
    Ok(overlay::run(
        hyprland.focused_monitor,
        position,
        events,
        keymap,
    ))
}
