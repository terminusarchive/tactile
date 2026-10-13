use std::fmt;

use xkbcommon::xkb;

/// Offset between evdev key codes and xkb key codes.
const EVDEV_TO_XKB_OFFSET: u32 = 8;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Modifier {
    Ctrl,
    Alt,
    Super,
    Shift,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Default)]
pub struct Modifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub super_key: bool,
    pub shift: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub enum KeyKind {
    Modifier(Modifier),
    Typed(String),
    Shortcut(String),
}

/// A key going down: what it is, and the modifiers held just before it.
#[derive(Debug, PartialEq, Eq)]
pub struct Press {
    pub kind: KeyKind,
    pub held: Modifiers,
}

#[derive(Debug)]
pub struct KeymapError(String);

impl fmt::Display for KeymapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for KeymapError {}

pub struct Keymap {
    state: xkb::State,
}

impl Keymap {
    pub fn new(layout: &str, variant: &str, options: &str) -> Result<Keymap, KeymapError> {
        let context = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
        let keymap = xkb::Keymap::new_from_names(
            &context,
            "",
            "",
            layout,
            variant,
            Some(options.to_string()),
            xkb::COMPILE_NO_FLAGS,
        )
        .ok_or_else(|| {
            KeymapError(format!(
                "no keymap for layout {layout:?}, variant {variant:?}, options {options:?}"
            ))
        })?;
        Ok(Keymap {
            state: xkb::State::new(&keymap),
        })
    }

    pub fn press(&mut self, code: u32) -> Press {
        let key = xkb_code(code);
        let held = self.held_modifiers();
        let sym = self.state.key_get_one_sym(key);
        self.state.update_key(key, xkb::KeyDirection::Down);
        Press {
            kind: classify(sym, held),
            held,
        }
    }

    pub fn release(&mut self, code: u32) {
        self.state.update_key(xkb_code(code), xkb::KeyDirection::Up);
    }

    fn held_modifiers(&self) -> Modifiers {
        let active = |name: &str| {
            self.state
                .mod_name_is_active(name, xkb::STATE_MODS_EFFECTIVE)
        };
        Modifiers {
            ctrl: active(xkb::MOD_NAME_CTRL),
            alt: active(xkb::MOD_NAME_ALT),
            super_key: active(xkb::MOD_NAME_LOGO),
            shift: active(xkb::MOD_NAME_SHIFT),
        }
    }
}

fn xkb_code(evdev_code: u32) -> xkb::Keycode {
    xkb::Keycode::new(evdev_code + EVDEV_TO_XKB_OFFSET)
}

fn classify(sym: xkb::Keysym, held: Modifiers) -> KeyKind {
    if let Some(modifier) = modifier_of(sym) {
        return KeyKind::Modifier(modifier);
    }
    let text = xkb::keysym_to_utf8(sym);
    let is_chorded = held.ctrl || held.alt || held.super_key;
    if is_printable(&text) && !is_chorded {
        return KeyKind::Typed(typed_label(&text));
    }
    KeyKind::Shortcut(shortcut_label(sym, &text))
}

#[allow(non_upper_case_globals)] // keysym constants keep their xkb names
fn modifier_of(sym: xkb::Keysym) -> Option<Modifier> {
    use xkbcommon::xkb::keysyms::*;
    match sym.raw() {
        KEY_Control_L | KEY_Control_R => Some(Modifier::Ctrl),
        KEY_Alt_L | KEY_Alt_R => Some(Modifier::Alt),
        KEY_Super_L | KEY_Super_R => Some(Modifier::Super),
        KEY_Shift_L | KEY_Shift_R => Some(Modifier::Shift),
        _ => None,
    }
}

fn is_printable(text: &str) -> bool {
    text.chars().next().is_some_and(|c| !c.is_control())
}

fn typed_label(text: &str) -> String {
    if text == " " {
        "␣".to_string()
    } else {
        text.to_string()
    }
}

#[allow(non_upper_case_globals)] // keysym constants keep their xkb names
fn shortcut_label(sym: xkb::Keysym, text: &str) -> String {
    use xkbcommon::xkb::keysyms::*;
    let word = match sym.raw() {
        KEY_Return | KEY_KP_Enter => "Enter",
        KEY_Escape => "Esc",
        KEY_Tab | KEY_ISO_Left_Tab => "Tab",
        KEY_BackSpace => "Backspace",
        KEY_space => "Space",
        KEY_Prior => "Page Up",
        KEY_Next => "Page Down",
        KEY_Left => "←",
        KEY_Up => "↑",
        KEY_Right => "→",
        KEY_Down => "↓",
        _ if is_printable(text) => {
            return text.to_uppercase();
        }
        _ => return xkb::keysym_get_name(sym).replace('_', " "),
    };
    word.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY_ESC: u32 = 1;
    const KEY_1: u32 = 2;
    const KEY_BACKSPACE: u32 = 14;
    const KEY_TAB: u32 = 15;
    const KEY_T: u32 = 20;
    const KEY_ENTER: u32 = 28;
    const KEY_LEFTCTRL: u32 = 29;
    const KEY_H: u32 = 35;
    const KEY_LEFTSHIFT: u32 = 42;
    const KEY_LEFTALT: u32 = 56;
    const KEY_SPACE: u32 = 57;
    const KEY_CAPSLOCK: u32 = 58;
    const KEY_F5: u32 = 63;
    const KEY_HOME: u32 = 102;
    const KEY_UP: u32 = 103;
    const KEY_LEFT: u32 = 105;
    const KEY_RIGHT: u32 = 106;
    const KEY_DOWN: u32 = 108;
    const KEY_DELETE: u32 = 111;
    const KEY_LEFTMETA: u32 = 125;

    fn us() -> Keymap {
        Keymap::new("us", "", "").unwrap()
    }

    fn typed(label: &str) -> KeyKind {
        KeyKind::Typed(label.to_string())
    }

    fn shortcut(label: &str) -> KeyKind {
        KeyKind::Shortcut(label.to_string())
    }

    #[test]
    fn letter_is_typed_with_its_character() {
        assert_eq!(us().press(KEY_H).kind, typed("h"));
    }

    #[test]
    fn shifted_letter_is_typed_with_capital() {
        let mut keymap = us();
        keymap.press(KEY_LEFTSHIFT);
        assert_eq!(keymap.press(KEY_H).kind, typed("H"));
    }

    #[test]
    fn shifted_digit_is_typed_with_its_symbol() {
        let mut keymap = us();
        keymap.press(KEY_LEFTSHIFT);
        assert_eq!(keymap.press(KEY_1).kind, typed("!"));
    }

    #[test]
    fn space_is_typed_as_open_box() {
        assert_eq!(us().press(KEY_SPACE).kind, typed("␣"));
    }

    #[test]
    fn modifier_keys_are_reported_as_modifiers() {
        let mut keymap = us();
        assert_eq!(
            keymap.press(KEY_LEFTCTRL).kind,
            KeyKind::Modifier(Modifier::Ctrl)
        );
        assert_eq!(
            keymap.press(KEY_LEFTALT).kind,
            KeyKind::Modifier(Modifier::Alt)
        );
        assert_eq!(
            keymap.press(KEY_LEFTMETA).kind,
            KeyKind::Modifier(Modifier::Super)
        );
        assert_eq!(
            keymap.press(KEY_LEFTSHIFT).kind,
            KeyKind::Modifier(Modifier::Shift)
        );
    }

    #[test]
    fn non_character_keys_are_shortcuts_labelled_in_words() {
        let mut keymap = us();
        let labelled = [
            (KEY_ENTER, "Enter"),
            (KEY_ESC, "Esc"),
            (KEY_TAB, "Tab"),
            (KEY_BACKSPACE, "Backspace"),
            (KEY_DELETE, "Delete"),
            (KEY_HOME, "Home"),
            (KEY_F5, "F5"),
        ];
        for (code, label) in labelled {
            assert_eq!(keymap.press(code).kind, shortcut(label));
        }
    }

    #[test]
    fn arrow_keys_are_labelled_with_arrows() {
        let mut keymap = us();
        let labelled = [
            (KEY_LEFT, "←"),
            (KEY_UP, "↑"),
            (KEY_RIGHT, "→"),
            (KEY_DOWN, "↓"),
        ];
        for (code, label) in labelled {
            assert_eq!(keymap.press(code).kind, shortcut(label));
        }
    }

    #[test]
    fn letter_with_ctrl_is_shortcut_labelled_with_capital() {
        let mut keymap = us();
        keymap.press(KEY_LEFTCTRL);
        let press = keymap.press(KEY_T);
        assert_eq!(press.kind, shortcut("T"));
        assert_eq!(
            press.held,
            Modifiers {
                ctrl: true,
                ..Modifiers::default()
            }
        );
    }

    #[test]
    fn released_modifier_is_no_longer_held() {
        let mut keymap = us();
        keymap.press(KEY_LEFTCTRL);
        keymap.release(KEY_LEFTCTRL);
        assert_eq!(keymap.press(KEY_T).kind, typed("t"));
    }

    #[test]
    fn caps_escape_option_makes_caps_lock_an_escape_key() {
        let mut keymap = Keymap::new("us", "", "caps:escape").unwrap();
        assert_eq!(keymap.press(KEY_CAPSLOCK).kind, shortcut("Esc"));
    }

    #[test]
    fn unknown_layout_is_an_error() {
        assert!(Keymap::new("no-such-layout", "", "").is_err());
    }
}
