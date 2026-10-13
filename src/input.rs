use std::fmt;
use std::fs;
use std::path::Path;
use std::sync::mpsc::Sender;
use std::thread;

use evdev::{AttributeSetRef, Device, EventType, InputEvent as RawEvent, KeyCode};

use crate::bar::{KeyId, KeyboardId};

const KEY_RELEASED: i32 = 0;
const KEY_PRESSED: i32 = 1;

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

/// Opens every keyboard `event*` device in `dir` without grabbing it and starts one
/// reader thread per device, sending its key events to `sender`.
pub fn start(dir: &Path, sender: Sender<InputEvent>) -> Result<(), InputError> {
    let keyboards = open_keyboards(dir)?;
    if keyboards.is_empty() {
        return Err(InputError(format!(
            "tactile cannot read any keyboard in {}; is your user in the `input` group?",
            dir.display()
        )));
    }
    for (index, device) in keyboards.into_iter().enumerate() {
        let keyboard = KeyboardId(index as u32);
        let sender = sender.clone();
        thread::spawn(move || read_keys(device, keyboard, sender));
    }
    Ok(())
}

fn open_keyboards(dir: &Path) -> Result<Vec<Device>, InputError> {
    let entries = fs::read_dir(dir).map_err(|e| {
        InputError(format!(
            "cannot list {}: {e}; is your user in the `input` group?",
            dir.display()
        ))
    })?;
    Ok(entries
        .flatten()
        .filter(|entry| entry.file_name().to_string_lossy().starts_with("event"))
        .filter_map(|entry| Device::open(entry.path()).ok())
        .filter(|device| device.supported_keys().is_some_and(is_keyboard))
        .collect())
}

fn is_keyboard(keys: &AttributeSetRef<KeyCode>) -> bool {
    keys.contains(KeyCode::KEY_A) && keys.contains(KeyCode::KEY_ENTER)
}

/// Runs until the device cannot be read (unplugged), then reports the keyboard gone.
fn read_keys(mut device: Device, keyboard: KeyboardId, sender: Sender<InputEvent>) {
    while let Ok(events) = device.fetch_events() {
        for event in events {
            let Some(event) = key_event(keyboard, &event) else {
                continue;
            };
            if sender.send(event).is_err() {
                return;
            }
        }
    }
    let _ = sender.send(InputEvent::Gone(keyboard));
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
    use std::sync::mpsc::channel;

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
        let (sender, _receiver) = channel();
        let error = start(&temp_dir("empty"), sender).unwrap_err();
        assert!(error.to_string().contains("`input` group"));
    }

    #[test]
    fn missing_directory_is_an_error_naming_the_input_group() {
        let (sender, _receiver) = channel();
        let error = start(Path::new("/nonexistent/tactile-input"), sender).unwrap_err();
        assert!(error.to_string().contains("`input` group"));
    }

    #[test]
    fn event_file_that_is_not_a_device_is_not_a_keyboard() {
        let dir = temp_dir("not-a-device");
        fs::write(dir.join("event0"), "").unwrap();
        let (sender, _receiver) = channel();
        assert!(start(&dir, sender).is_err());
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
