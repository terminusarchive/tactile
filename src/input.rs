use std::collections::HashSet;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use async_channel::{SendError, Sender};
use evdev::{AttributeSetRef, Device, EventType, InputEvent as RawEvent, KeyCode};

use crate::bar::{KeyId, KeyboardId};

const KEY_RELEASED: i32 = 0;
const KEY_PRESSED: i32 = 1;
const RESCAN_INTERVAL: Duration = Duration::from_secs(1);

/// What a reader thread tells the main loop.
#[derive(Debug, PartialEq, Eq)]
pub enum InputEvent {
    Down(KeyId),
    Up(KeyId),
    Gone(KeyboardId),
}

/// No keyboard under the input directory could be opened.
#[derive(Debug, PartialEq, Eq)]
pub struct InputError(String);

impl fmt::Display for InputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for InputError {}

/// Device nodes already dealt with: open keyboards, and devices that are not keyboards.
/// Each is looked at once while its node exists; a failed open is not recorded, so a
/// node that is not readable yet is tried again.
#[derive(Default)]
struct Examined(HashSet<PathBuf>);

impl Examined {
    /// The listed paths not yet dealt with. Paths no longer listed are forgotten.
    fn take_unexamined(&mut self, listed: &[PathBuf]) -> Vec<PathBuf> {
        self.0.retain(|path| listed.contains(path));
        listed
            .iter()
            .filter(|path| !self.0.contains(*path))
            .cloned()
            .collect()
    }

    fn settle(&mut self, path: PathBuf) {
        self.0.insert(path);
    }

    fn forget(&mut self, path: &Path) {
        self.0.remove(path);
    }
}

/// Opens keyboards under `dir` that are not open yet and gives each a reader thread.
struct Scanner {
    dir: PathBuf,
    examined: Arc<Mutex<Examined>>,
    next_keyboard: u32,
    sender: Sender<InputEvent>,
}

impl Scanner {
    /// Returns how many keyboards were opened.
    fn scan(&mut self) -> Result<usize, InputError> {
        let listed = list_event_nodes(&self.dir)?;
        let take_unexamined = self.examined.lock().unwrap().take_unexamined(&listed);
        let mut opened = 0;
        for path in take_unexamined {
            let Ok(device) = Device::open(&path) else {
                continue;
            };
            self.examined.lock().unwrap().settle(path.clone());
            if device.supported_keys().is_some_and(is_keyboard) {
                self.spawn_reader(device, path);
                opened += 1;
            }
        }
        Ok(opened)
    }

    fn spawn_reader(&mut self, device: Device, path: PathBuf) {
        let keyboard = KeyboardId(self.next_keyboard);
        self.next_keyboard += 1;
        let sender = self.sender.clone();
        let examined = Arc::clone(&self.examined);
        thread::spawn(move || {
            read_keys(device, keyboard, &sender, || {
                examined.lock().unwrap().forget(&path)
            })
        });
    }
}

/// Opens every keyboard `event*` device in `dir` without grabbing it and starts one
/// reader thread per device, sending its key events to `sender`. Keyboards connected
/// later are picked up by a rescan once a second.
pub fn start(dir: &Path, sender: Sender<InputEvent>) -> Result<(), InputError> {
    let mut scanner = Scanner {
        dir: dir.to_path_buf(),
        examined: Arc::default(),
        next_keyboard: 0,
        sender,
    };
    if scanner.scan()? == 0 {
        return Err(InputError(format!(
            "tactile cannot read any keyboard in {}; is your user in the `input` group?",
            dir.display()
        )));
    }
    thread::spawn(move || {
        while !scanner.sender.is_closed() {
            thread::sleep(RESCAN_INTERVAL);
            let _ = scanner.scan();
        }
    });
    Ok(())
}

fn list_event_nodes(dir: &Path) -> Result<Vec<PathBuf>, InputError> {
    let entries = fs::read_dir(dir).map_err(|e| {
        InputError(format!(
            "cannot list {}: {e}; is your user in the `input` group?",
            dir.display()
        ))
    })?;
    Ok(entries
        .flatten()
        .filter(|entry| entry.file_name().to_string_lossy().starts_with("event"))
        .map(|entry| entry.path())
        .collect())
}

fn is_keyboard(keys: &AttributeSetRef<KeyCode>) -> bool {
    keys.contains(KeyCode::KEY_A) && keys.contains(KeyCode::KEY_ENTER)
}

/// Runs until the device cannot be read (unplugged), then calls `on_gone` and reports
/// the keyboard gone.
fn read_keys(
    mut device: Device,
    keyboard: KeyboardId,
    sender: &Sender<InputEvent>,
    on_gone: impl FnOnce(),
) {
    while let Ok(events) = device.fetch_events() {
        if forward_keys(events, keyboard, sender).is_err() {
            return;
        }
    }
    on_gone();
    let _ = sender.send_blocking(InputEvent::Gone(keyboard));
}

/// Sends the key events among `events`; fails once the main loop has gone.
fn forward_keys(
    events: impl Iterator<Item = RawEvent>,
    keyboard: KeyboardId,
    sender: &Sender<InputEvent>,
) -> Result<(), SendError<InputEvent>> {
    events
        .filter_map(|event| key_event(keyboard, &event))
        .try_for_each(|event| sender.send_blocking(event))
}

/// Key down and key up only: other event types and kernel auto-repeat are dropped.
fn key_event(keyboard: KeyboardId, event: &RawEvent) -> Option<InputEvent> {
    if event.event_type() != EventType::KEY {
        return None;
    }
    let key = KeyId {
        keyboard,
        code: u32::from(event.code()),
    };
    match event.value() {
        KEY_PRESSED => Some(InputEvent::Down(key)),
        KEY_RELEASED => Some(InputEvent::Up(key)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use evdev::AttributeSet;

    use super::*;

    const KEYBOARD: KeyboardId = KeyboardId(3);

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("tactile-input-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn key_event_of(value: i32) -> Option<InputEvent> {
        let raw = RawEvent::new(EventType::KEY.0, KeyCode::KEY_A.code(), value);
        key_event(KEYBOARD, &raw)
    }

    #[test]
    fn no_device_in_directory_is_an_error_naming_the_input_group() {
        let (sender, _receiver) = async_channel::unbounded();
        let error = start(&temp_dir("empty"), sender).unwrap_err();
        assert!(error.to_string().contains("`input` group"));
    }

    #[test]
    fn missing_directory_is_an_error_naming_the_input_group() {
        let (sender, _receiver) = async_channel::unbounded();
        let error = start(Path::new("/nonexistent/tactile-input"), sender).unwrap_err();
        assert!(error.to_string().contains("`input` group"));
    }

    #[test]
    fn event_file_that_is_not_a_device_is_not_a_keyboard() {
        let dir = temp_dir("not-a-device");
        fs::write(dir.join("event0"), "").unwrap();
        let (sender, _receiver) = async_channel::unbounded();
        assert!(start(&dir, sender).is_err());
    }

    fn paths(names: &[&str]) -> Vec<PathBuf> {
        names.iter().map(PathBuf::from).collect()
    }

    #[test]
    fn a_device_already_dealt_with_is_not_opened_again() {
        let mut examined = Examined::default();
        let listed = paths(&["event0", "event1"]);
        assert_eq!(examined.take_unexamined(&listed), listed);
        for path in &listed {
            examined.settle(path.clone());
        }
        assert!(examined.take_unexamined(&listed).is_empty());
        assert_eq!(
            examined.take_unexamined(&paths(&["event0", "event1", "event2"])),
            paths(&["event2"])
        );
    }

    #[test]
    fn a_failed_open_is_tried_again_on_the_next_rescan() {
        let mut examined = Examined::default();
        let listed = paths(&["event5"]);
        assert_eq!(examined.take_unexamined(&listed), listed);
        assert_eq!(examined.take_unexamined(&listed), listed);
    }

    #[test]
    fn a_keyboard_that_went_is_opened_again_when_its_path_is_still_listed() {
        let mut examined = Examined::default();
        let listed = paths(&["event3"]);
        examined.settle(PathBuf::from("event3"));
        examined.forget(Path::new("event3"));
        assert_eq!(examined.take_unexamined(&listed), listed);
    }

    #[test]
    fn a_node_that_vanished_and_came_back_is_opened_again() {
        let mut examined = Examined::default();
        let listed = paths(&["event3"]);
        examined.settle(PathBuf::from("event3"));
        assert!(examined.take_unexamined(&[]).is_empty());
        assert_eq!(examined.take_unexamined(&listed), listed);
    }

    #[test]
    fn device_with_letters_and_enter_is_a_keyboard() {
        let keys = AttributeSet::from_iter([KeyCode::KEY_A, KeyCode::KEY_ENTER, KeyCode::KEY_Z]);
        assert!(is_keyboard(&keys));
    }

    #[test]
    fn mouse_and_power_button_are_not_keyboards() {
        let mouse = AttributeSet::from_iter([KeyCode::BTN_LEFT, KeyCode::BTN_RIGHT]);
        let power = AttributeSet::from_iter([KeyCode::KEY_POWER]);
        assert!(!is_keyboard(&mouse));
        assert!(!is_keyboard(&power));
    }

    #[test]
    fn press_and_release_are_reported_with_their_keyboard() {
        let key = KeyId {
            keyboard: KEYBOARD,
            code: u32::from(KeyCode::KEY_A.code()),
        };
        assert_eq!(key_event_of(1), Some(InputEvent::Down(key)));
        assert_eq!(key_event_of(0), Some(InputEvent::Up(key)));
    }

    #[test]
    fn kernel_auto_repeat_is_dropped() {
        assert_eq!(key_event_of(2), None);
    }

    #[test]
    fn events_that_are_not_keys_are_dropped() {
        let sync = RawEvent::new(EventType::SYNCHRONIZATION.0, 0, 1);
        assert_eq!(key_event(KEYBOARD, &sync), None);
    }
}
