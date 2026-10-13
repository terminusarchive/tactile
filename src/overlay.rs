use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

use async_channel::Receiver;

use gtk4::prelude::*;
use gtk4::{gdk, glib};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

use crate::args::Position;
use crate::bar::{Bar, Cap, IDLE_LIMIT};
use crate::input::InputEvent;
use crate::keymap::Keymap;

const EDGE_MARGIN: i32 = 48;
const CAP_SPACING: i32 = 10;

const THEME: &str = r#"
window.tactile {
    background: transparent;
}
.bar {
    background-color: rgba(16, 17, 24, 0.84);
    border: 1px solid rgba(255, 255, 255, 0.07);
    border-radius: 26px;
    padding: 14px;
}
.cap {
    font-family: "Inter", "Noto Sans", "DejaVu Sans", sans-serif;
    font-size: 24px;
    font-weight: 600;
    color: #e9ebf3;
    min-width: 30px;
    padding: 9px 18px 8px 18px;
    background-image: linear-gradient(to bottom, #3b3d4d, #2a2b38);
    border: 1px solid rgba(255, 255, 255, 0.10);
    border-bottom: 4px solid rgba(0, 0, 0, 0.45);
    border-radius: 14px;
}
.cap.lit {
    color: #ffffff;
    background-image: linear-gradient(to bottom, #8f7dff, #6f5cf0);
    border-color: rgba(255, 255, 255, 0.28);
    border-bottom: 4px solid #4a38c4;
}
.count {
    font-size: 17px;
    font-weight: 500;
    opacity: 0.72;
    margin-left: 10px;
}
"#;

/// Applies one input event to the key map and the bar. Every key-up reaches the key
/// map, including keys the bar ignores, or its modifier state goes stale.
fn apply_event(keymap: &mut Keymap, bar: &mut Bar, event: InputEvent, now: Instant) {
    match event {
        InputEvent::Down(key) => {
            let press = keymap.press(key.code);
            bar.press(key, press, now);
        }
        InputEvent::Up(key) => {
            keymap.release(key.code);
            bar.release(key, now);
        }
        InputEvent::Gone(keyboard) => {
            for code in bar.keyboard_gone(keyboard, now) {
                keymap.release(code);
            }
        }
    }
}

/// Shows the bar in a layer-shell window on the named monitor until the process is
/// stopped.
pub fn run(
    monitor_name: String,
    position: Position,
    events: Receiver<InputEvent>,
    keymap: Keymap,
) -> glib::ExitCode {
    let app = gtk4::Application::builder()
        .application_id("dev.tactile.overlay")
        .flags(gtk4::gio::ApplicationFlags::NON_UNIQUE)
        .build();
    let state = RefCell::new(Some((events, keymap)));
    app.connect_activate(move |app| {
        if let Some((events, keymap)) = state.borrow_mut().take() {
            show(app, &monitor_name, position, events, keymap);
        }
    });
    app.run_with_args::<&str>(&[])
}

fn show(
    app: &gtk4::Application,
    monitor_name: &str,
    position: Position,
    events: Receiver<InputEvent>,
    keymap: Keymap,
) {
    let display = gdk::Display::default().expect("GTK opened a display");
    gtk4::style_context_add_provider_for_display(
        &display,
        &css_provider(),
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
    let monitor = find_monitor(&display, monitor_name);
    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, CAP_SPACING);
    row.add_css_class("bar");
    let window = layer_window(app, &row, monitor.as_ref(), position);
    let max_width = max_bar_width(
        monitor.as_ref().map(monitor_width),
        first_monitor(&display).as_ref().map(monitor_width),
    );

    let overlay = Rc::new(RefCell::new(Overlay {
        window,
        row,
        bar: Bar::default(),
        keymap,
        max_width,
        idle_timer: None,
    }));
    glib::spawn_future_local(async move {
        while let Ok(event) = events.recv().await {
            handle_event(&overlay, event);
        }
    });
}

/// Everything the main loop touches when a key arrives or the bar goes idle.
struct Overlay {
    window: gtk4::Window,
    row: gtk4::Box,
    bar: Bar,
    keymap: Keymap,
    max_width: i32,
    idle_timer: Option<glib::SourceId>,
}

/// Applies the event, redraws, and restarts the idle-clear timer. The timer exists
/// only after an event, so an idle tactile wakes for nothing.
fn handle_event(overlay: &Rc<RefCell<Overlay>>, event: InputEvent) {
    let mut guard = overlay.borrow_mut();
    let this = &mut *guard;
    apply_event(&mut this.keymap, &mut this.bar, event, Instant::now());
    redraw(&this.window, &this.row, &mut this.bar, this.max_width);
    if let Some(timer) = this.idle_timer.take() {
        timer.remove();
    }
    this.idle_timer = Some(clear_when_idle(Rc::clone(overlay)));
}

fn clear_when_idle(overlay: Rc<RefCell<Overlay>>) -> glib::SourceId {
    glib::timeout_add_local_once(IDLE_LIMIT, move || {
        let mut guard = overlay.borrow_mut();
        let this = &mut *guard;
        this.idle_timer = None;
        this.bar.clear_if_idle(Instant::now());
        redraw(&this.window, &this.row, &mut this.bar, this.max_width);
    })
}

fn css_provider() -> gtk4::CssProvider {
    let provider = gtk4::CssProvider::new();
    provider.load_from_string(THEME);
    provider
}

fn find_monitor(display: &gdk::Display, connector: &str) -> Option<gdk::Monitor> {
    display
        .monitors()
        .iter::<gdk::Monitor>()
        .flatten()
        .find(|monitor| monitor.connector().as_deref() == Some(connector))
}

fn first_monitor(display: &gdk::Display) -> Option<gdk::Monitor> {
    display.monitors().iter::<gdk::Monitor>().flatten().next()
}

fn monitor_width(monitor: &gdk::Monitor) -> i32 {
    monitor.geometry().width()
}

/// The widest the bar may be: the focused monitor's width less both margins, or the
/// first monitor's when the focused one was not found, so the trim always has a bound.
fn max_bar_width(focused: Option<i32>, first: Option<i32>) -> i32 {
    focused
        .or(first)
        .map_or(i32::MAX, |width| width - 2 * EDGE_MARGIN)
}

/// The edges the bar is pinned to; a bar pinned to one edge is centred along it.
fn anchored_edges(position: Position) -> &'static [Edge] {
    match position {
        Position::TopLeft => &[Edge::Top, Edge::Left],
        Position::Top => &[Edge::Top],
        Position::TopRight => &[Edge::Top, Edge::Right],
        Position::BottomLeft => &[Edge::Bottom, Edge::Left],
        Position::Bottom => &[Edge::Bottom],
        Position::BottomRight => &[Edge::Bottom, Edge::Right],
    }
}

/// A window on the overlay layer that takes no keyboard focus and lets clicks through.
fn layer_window(
    app: &gtk4::Application,
    row: &gtk4::Box,
    monitor: Option<&gdk::Monitor>,
    position: Position,
) -> gtk4::Window {
    let window = gtk4::Window::builder()
        .application(app)
        .child(row)
        .decorated(false)
        .resizable(false)
        .css_classes(["tactile"])
        .build();
    window.init_layer_shell();
    window.set_namespace(Some("tactile"));
    window.set_layer(Layer::Overlay);
    window.set_keyboard_mode(KeyboardMode::None);
    window.set_monitor(monitor);
    for &edge in anchored_edges(position) {
        window.set_anchor(edge, true);
        window.set_margin(edge, EDGE_MARGIN);
    }
    window.connect_realize(|window| {
        if let Some(surface) = window.surface() {
            surface.set_input_region(Some(&gtk4::cairo::Region::create()));
        }
    });
    window
}

/// Draws the bar's caps, dropping the oldest while they are wider than the monitor.
fn redraw(window: &gtk4::Window, row: &gtk4::Box, bar: &mut Bar, max_width: i32) {
    show_caps(row, bar.caps());
    while bar.caps().len() > 1 && natural_width(row) > max_width {
        bar.drop_oldest();
        show_caps(row, bar.caps());
    }
    window.set_visible(!bar.caps().is_empty());
}

fn natural_width(row: &gtk4::Box) -> i32 {
    row.measure(gtk4::Orientation::Horizontal, -1).1
}

fn show_caps(row: &gtk4::Box, caps: &[Cap]) {
    while let Some(child) = row.first_child() {
        row.remove(&child);
    }
    for cap in caps {
        row.append(&cap_widget(cap));
    }
}

fn cap_widget(cap: &Cap) -> gtk4::Box {
    let widget = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    widget.add_css_class("cap");
    widget.set_halign(gtk4::Align::Center);
    if cap.lit {
        widget.add_css_class("lit");
    }
    widget.append(&gtk4::Label::new(Some(&cap.label)));
    if cap.count > 1 {
        let count = gtk4::Label::new(Some(&format!("×{}", cap.count)));
        count.add_css_class("count");
        widget.append(&count);
    }
    widget
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bar::{KeyId, KeyboardId};

    const KEY_LEFTCTRL: u32 = 29;
    const KEY_A: u32 = 30;

    #[test]
    fn unfound_monitor_is_bounded_by_the_first_monitor() {
        assert_eq!(max_bar_width(None, Some(1920)), 1920 - 2 * EDGE_MARGIN);
        assert_eq!(
            max_bar_width(Some(2560), Some(1920)),
            2560 - 2 * EDGE_MARGIN
        );
    }

    fn key(code: u32) -> KeyId {
        KeyId {
            keyboard: KeyboardId(0),
            code,
        }
    }

    #[test]
    fn released_ctrl_no_longer_turns_a_letter_into_a_shortcut() {
        let mut keymap = Keymap::new("us", "", "").unwrap();
        let mut bar = Bar::default();
        let now = Instant::now();
        apply_event(
            &mut keymap,
            &mut bar,
            InputEvent::Down(key(KEY_LEFTCTRL)),
            now,
        );
        apply_event(
            &mut keymap,
            &mut bar,
            InputEvent::Up(key(KEY_LEFTCTRL)),
            now,
        );
        apply_event(&mut keymap, &mut bar, InputEvent::Down(key(KEY_A)), now);
        let labels: Vec<_> = bar.caps().iter().map(|cap| cap.label.as_str()).collect();
        assert_eq!(labels, ["Ctrl", "a"]);
    }

    #[test]
    fn ctrl_held_on_an_unplugged_keyboard_no_longer_affects_the_next_key() {
        let mut keymap = Keymap::new("us", "", "").unwrap();
        let mut bar = Bar::default();
        let now = Instant::now();
        let on_second = KeyId {
            keyboard: KeyboardId(1),
            code: KEY_A,
        };
        apply_event(
            &mut keymap,
            &mut bar,
            InputEvent::Down(key(KEY_LEFTCTRL)),
            now,
        );
        apply_event(&mut keymap, &mut bar, InputEvent::Gone(KeyboardId(0)), now);
        apply_event(&mut keymap, &mut bar, InputEvent::Down(on_second), now);
        let labels: Vec<_> = bar.caps().iter().map(|cap| cap.label.as_str()).collect();
        assert_eq!(labels, ["Ctrl", "a"]);
    }
}
