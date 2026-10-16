mod args;
mod bar;
mod compositor;
mod hyprland;
mod input;
mod keymap;
mod lock;
mod overlay;
mod tachyon;

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
    let position = args::parse_position(std::env::args_os().skip(1))?;
    let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR").ok_or("XDG_RUNTIME_DIR is not set")?;
    let _lock = lock::acquire(Path::new(&runtime_dir))?;
    let compositor = compositor::Compositor::detect()?;
    let setup = compositor.query()?;
    let keymap = keymap::Keymap::new(&setup.layout, &setup.variant, &setup.options)?;
    let (sender, events) = async_channel::unbounded();
    input::start(Path::new(INPUT_DIR), sender)?;
    Ok(overlay::run(
        setup.focused_monitor,
        position,
        events,
        keymap,
        compositor,
    ))
}
