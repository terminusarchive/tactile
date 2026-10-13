use std::time::{Duration, Instant};

use crate::keymap::{KeyKind, Modifier, Modifiers, Press};

pub const IDLE_LIMIT: Duration = Duration::from_secs(2);

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct KeyboardId(pub u32);

/// A physical key: its evdev code and the keyboard it is on.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct KeyId {
    pub keyboard: KeyboardId,
    pub code: u32,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
struct CapId(u64);

pub struct Cap {
    id: CapId,
    pub label: String,
    pub count: u32,
    pub lit: bool,
    pressed_with: Modifiers,
}

impl Cap {
    fn repeats(&self, label: &str, held: Modifiers) -> bool {
        self.label == label && self.pressed_with == held
    }
}

struct HeldKey {
    key: KeyId,
    cap: Option<CapId>,
    is_shift: bool,
    /// Held but not shown: pressed while the session was locked, or on the bar when it
    /// was cleared. It is known as held so a release or a keyboard going away reaches
    /// the key map, but it draws nothing and does not keep the bar from clearing.
    hidden: bool,
}

#[derive(Default)]
pub struct Bar {
    caps: Vec<Cap>,
    held_keys: Vec<HeldKey>,
    shift_cap: Option<CapId>,
    last_activity: Option<Instant>,
    next_cap_id: u64,
}

impl Bar {
    pub fn press(&mut self, key: KeyId, press: Press, now: Instant) {
        self.last_activity = Some(now);
        let held = press.held;
        let (cap, is_shift) = match press.kind {
            KeyKind::Modifier(Modifier::Shift) => (None, true),
            KeyKind::Modifier(modifier) => (
                Some(self.repeat_or_push(modifier_label(modifier), held)),
                false,
            ),
            KeyKind::Typed(label) => (Some(self.push_cap(label, held)), false),
            KeyKind::Shortcut(label) => (Some(self.press_shortcut(label, held)), false),
        };
        self.held_keys.push(HeldKey {
            key,
            cap,
            is_shift,
            hidden: false,
        });
    }

    /// Records a key going down without a cap, for a press while the session is locked.
    pub fn press_hidden(&mut self, key: KeyId, press: Press) {
        self.held_keys.push(HeldKey {
            key,
            cap: None,
            is_shift: press.kind == KeyKind::Modifier(Modifier::Shift),
            hidden: true,
        });
    }

    pub fn release(&mut self, key: KeyId, now: Instant) {
        self.last_activity = Some(now);
        self.release_where(|held_key| held_key.key == key);
    }

    /// Lets go of every key held on `keyboard` and returns their codes, so the caller
    /// can release them in the key map too.
    pub fn keyboard_gone(&mut self, keyboard: KeyboardId, now: Instant) -> Vec<u32> {
        self.last_activity = Some(now);
        self.release_where(|held_key| held_key.key.keyboard == keyboard)
    }

    /// Empties the bar once the idle limit has passed since the last press or release
    /// and no key is held.
    pub fn clear_if_idle(&mut self, now: Instant) {
        let is_idle = self
            .last_activity
            .is_some_and(|pressed| now.duration_since(pressed) >= IDLE_LIMIT);
        if is_idle && self.held_keys.iter().all(|held_key| held_key.hidden) {
            self.caps.clear();
            self.last_activity = None;
        }
    }

    /// Removes every cap. Keys still held stay known as held, with no cap.
    pub fn clear(&mut self) {
        self.caps.clear();
        for held_key in &mut self.held_keys {
            held_key.cap = None;
            held_key.hidden = true;
        }
        self.shift_cap = None;
        self.last_activity = None;
    }

    pub fn drop_oldest(&mut self) {
        if !self.caps.is_empty() {
            let dropped = self.caps.remove(0);
            if self.shift_cap == Some(dropped.id) {
                self.shift_cap = None;
            }
        }
    }

    pub fn caps(&self) -> &[Cap] {
        &self.caps
    }

    fn press_shortcut(&mut self, label: String, held: Modifiers) -> CapId {
        let repeats_last = self
            .caps
            .last()
            .is_some_and(|last| last.repeats(&label, held));
        if held.shift && self.shift_cap.is_none() && !repeats_last {
            let shift_label = modifier_label(Modifier::Shift);
            self.shift_cap = Some(self.push_cap(shift_label, held));
        }
        self.repeat_or_push(label, held)
    }

    fn repeat_or_push(&mut self, label: String, held: Modifiers) -> CapId {
        match self.caps.last_mut() {
            Some(last) if last.repeats(&label, held) => {
                last.count += 1;
                last.lit = true;
                last.id
            }
            _ => self.push_cap(label, held),
        }
    }

    fn push_cap(&mut self, label: String, held: Modifiers) -> CapId {
        let id = CapId(self.next_cap_id);
        self.next_cap_id += 1;
        self.caps.push(Cap {
            id,
            label,
            count: 1,
            lit: true,
            pressed_with: held,
        });
        id
    }

    fn release_where(&mut self, is_released: impl Fn(&HeldKey) -> bool) -> Vec<u32> {
        let (released, kept): (Vec<_>, Vec<_>) = std::mem::take(&mut self.held_keys)
            .into_iter()
            .partition(is_released);
        self.held_keys = kept;
        for cap in released.iter().filter_map(|held_key| held_key.cap) {
            if !self.is_held(cap) {
                self.unlight(cap);
            }
        }
        let shift_let_go = released.iter().any(|held_key| held_key.is_shift)
            && !self.held_keys.iter().any(|held_key| held_key.is_shift);
        if let Some(cap) = self.shift_cap.filter(|_| shift_let_go) {
            self.unlight(cap);
            self.shift_cap = None;
        }
        released.iter().map(|held_key| held_key.key.code).collect()
    }

    fn is_held(&self, cap: CapId) -> bool {
        self.held_keys
            .iter()
            .any(|held_key| held_key.cap == Some(cap))
    }

    fn unlight(&mut self, id: CapId) {
        if let Some(cap) = self.caps.iter_mut().find(|cap| cap.id == id) {
            cap.lit = false;
        }
    }
}

fn modifier_label(modifier: Modifier) -> String {
    match modifier {
        Modifier::Ctrl => "Ctrl",
        Modifier::Alt => "Alt",
        Modifier::Super => "Super",
        Modifier::Shift => "Shift",
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::keymap::{KeyKind, Modifier, Modifiers};

    const KB: KeyboardId = KeyboardId(1);
    const OTHER_KB: KeyboardId = KeyboardId(2);

    fn key(code: u32) -> KeyId {
        KeyId { keyboard: KB, code }
    }

    fn modifier(modifier: Modifier, held: Modifiers) -> Press {
        Press {
            kind: KeyKind::Modifier(modifier),
            held,
        }
    }

    fn typed(label: &str, held: Modifiers) -> Press {
        Press {
            kind: KeyKind::Typed(label.to_string()),
            held,
        }
    }

    fn shortcut(label: &str, held: Modifiers) -> Press {
        Press {
            kind: KeyKind::Shortcut(label.to_string()),
            held,
        }
    }

    fn none() -> Modifiers {
        Modifiers::default()
    }

    fn shift() -> Modifiers {
        Modifiers {
            shift: true,
            ..none()
        }
    }

    fn ctrl() -> Modifiers {
        Modifiers {
            ctrl: true,
            ..none()
        }
    }

    fn labels(bar: &Bar) -> Vec<&str> {
        bar.caps().iter().map(|cap| cap.label.as_str()).collect()
    }

    #[test]
    fn modifier_adds_its_cap_at_once() {
        let mut bar = Bar::default();
        let now = Instant::now();
        bar.press(key(1), modifier(Modifier::Ctrl, none()), now);
        bar.press(key(2), modifier(Modifier::Alt, ctrl()), now);
        bar.press(key(3), modifier(Modifier::Super, none()), now);
        assert_eq!(labels(&bar), ["Ctrl", "Alt", "Super"]);
    }

    #[test]
    fn shift_alone_adds_nothing() {
        let mut bar = Bar::default();
        let now = Instant::now();
        bar.press(key(1), modifier(Modifier::Shift, none()), now);
        bar.release(key(1), now);
        assert!(bar.caps().is_empty());
    }

    #[test]
    fn shift_while_typing_adds_nothing_but_the_character() {
        let mut bar = Bar::default();
        let now = Instant::now();
        bar.press(key(1), modifier(Modifier::Shift, none()), now);
        bar.press(key(2), typed("H", shift()), now);
        assert_eq!(labels(&bar), ["H"]);
    }

    #[test]
    fn shift_cap_comes_once_per_hold_just_before_first_shortcut() {
        let mut bar = Bar::default();
        let now = Instant::now();
        bar.press(key(1), modifier(Modifier::Shift, none()), now);
        bar.press(key(2), shortcut("Enter", shift()), now);
        bar.release(key(2), now);
        bar.press(key(3), shortcut("Tab", shift()), now);
        assert_eq!(labels(&bar), ["Shift", "Enter", "Tab"]);
    }

    #[test]
    fn shift_cap_is_lit_until_shift_is_released() {
        let mut bar = Bar::default();
        let now = Instant::now();
        bar.press(key(1), modifier(Modifier::Shift, none()), now);
        bar.press(key(2), shortcut("Enter", shift()), now);
        bar.release(key(2), now);
        assert!(bar.caps()[0].lit);
        bar.release(key(1), now);
        assert!(!bar.caps()[0].lit);
    }

    #[test]
    fn a_new_hold_of_shift_adds_a_new_shift_cap() {
        let mut bar = Bar::default();
        let now = Instant::now();
        bar.press(key(1), modifier(Modifier::Shift, none()), now);
        bar.press(key(2), shortcut("Enter", shift()), now);
        bar.release(key(1), now);
        bar.release(key(2), now);
        bar.press(key(1), modifier(Modifier::Shift, none()), now);
        bar.press(key(3), shortcut("Tab", shift()), now);
        assert_eq!(labels(&bar), ["Shift", "Enter", "Shift", "Tab"]);
    }

    #[test]
    fn cap_is_lit_from_press_to_release() {
        let mut bar = Bar::default();
        let now = Instant::now();
        bar.press(key(2), typed("h", none()), now);
        assert!(bar.caps()[0].lit);
        bar.release(key(2), now);
        assert!(!bar.caps()[0].lit);
    }

    #[test]
    fn repeated_shortcut_with_same_modifiers_counts() {
        let mut bar = Bar::default();
        let now = Instant::now();
        for _ in 0..3 {
            bar.press(key(2), shortcut("Enter", none()), now);
            bar.release(key(2), now);
        }
        assert_eq!(bar.caps().len(), 1);
        assert_eq!(bar.caps()[0].count, 3);
        bar.press(key(2), shortcut("Enter", none()), now);
        assert!(bar.caps()[0].lit);
    }

    #[test]
    fn repeated_modifier_counts() {
        let mut bar = Bar::default();
        let now = Instant::now();
        for _ in 0..2 {
            bar.press(key(1), modifier(Modifier::Ctrl, none()), now);
            bar.release(key(1), now);
        }
        assert_eq!(bar.caps().len(), 1);
        assert_eq!(bar.caps()[0].count, 2);
    }

    #[test]
    fn same_label_with_other_modifiers_adds_a_cap() {
        let mut bar = Bar::default();
        let now = Instant::now();
        bar.press(key(2), shortcut("T", ctrl()), now);
        bar.press(key(2), shortcut("T", none()), now);
        assert_eq!(labels(&bar), ["T", "T"]);
    }

    #[test]
    fn typed_characters_never_count() {
        let mut bar = Bar::default();
        let now = Instant::now();
        for (code, label) in [(35, "h"), (18, "e"), (38, "l"), (38, "l"), (24, "o")] {
            bar.press(key(code), typed(label, none()), now);
            bar.release(key(code), now);
        }
        assert_eq!(labels(&bar), ["h", "e", "l", "l", "o"]);
    }

    #[test]
    fn bar_empties_two_seconds_after_last_activity() {
        let mut bar = Bar::default();
        let start = Instant::now();
        bar.press(key(2), typed("h", none()), start);
        bar.release(key(2), start);
        bar.clear_if_idle(start + Duration::from_millis(1999));
        assert_eq!(bar.caps().len(), 1);
        bar.clear_if_idle(start + Duration::from_secs(2));
        assert!(bar.caps().is_empty());
    }

    #[test]
    fn bar_stays_while_a_key_is_held() {
        let mut bar = Bar::default();
        let start = Instant::now();
        bar.press(key(2), typed("h", none()), start);
        bar.clear_if_idle(start + Duration::from_secs(10));
        assert_eq!(bar.caps().len(), 1);
    }

    #[test]
    fn a_keyboard_going_away_unlights_only_its_caps() {
        let mut bar = Bar::default();
        let now = Instant::now();
        let on_other = KeyId {
            keyboard: OTHER_KB,
            code: 3,
        };
        bar.press(key(2), typed("a", none()), now);
        bar.press(on_other, typed("b", none()), now);
        bar.keyboard_gone(KB, now);
        assert!(!bar.caps()[0].lit);
        assert!(bar.caps()[1].lit);
    }

    #[test]
    fn a_keyboard_going_away_with_shift_held_unlights_the_shift_cap() {
        let mut bar = Bar::default();
        let now = Instant::now();
        bar.press(key(1), modifier(Modifier::Shift, none()), now);
        bar.press(key(2), shortcut("Enter", shift()), now);
        bar.keyboard_gone(KB, now);
        assert!(bar.caps().iter().all(|cap| !cap.lit));
    }

    #[test]
    fn a_key_held_without_a_cap_draws_nothing_and_does_not_block_the_idle_clear() {
        let mut bar = Bar::default();
        let start = Instant::now();
        bar.press_hidden(key(1), modifier(Modifier::Shift, none()));
        assert!(bar.caps().is_empty());
        bar.press(key(2), typed("a", none()), start);
        bar.release(key(2), start);
        bar.clear_if_idle(start + Duration::from_secs(2));
        assert!(bar.caps().is_empty());
    }

    #[test]
    fn clearing_keeps_what_is_held_and_its_release_returns_the_code() {
        let mut bar = Bar::default();
        let now = Instant::now();
        bar.press(key(1), modifier(Modifier::Ctrl, none()), now);
        bar.clear();
        assert!(bar.caps().is_empty());
        assert_eq!(bar.keyboard_gone(KB, now), [1]);
    }

    #[test]
    fn shift_held_through_a_clear_keeps_the_shift_cap_lit_until_it_is_let_go() {
        let mut bar = Bar::default();
        let now = Instant::now();
        bar.press(key(1), modifier(Modifier::Shift, none()), now);
        bar.clear();
        bar.press(key(2), modifier(Modifier::Shift, none()), now);
        bar.press(key(3), shortcut("Enter", shift()), now);
        bar.release(key(2), now);
        assert!(bar.caps()[0].lit);
        bar.release(key(1), now);
        assert!(!bar.caps()[0].lit);
    }

    #[test]
    fn drop_oldest_removes_the_first_cap() {
        let mut bar = Bar::default();
        let now = Instant::now();
        bar.press(key(2), typed("a", none()), now);
        bar.press(key(3), typed("b", none()), now);
        bar.drop_oldest();
        assert_eq!(labels(&bar), ["b"]);
    }

    #[test]
    fn idle_time_counts_from_the_last_release() {
        let mut bar = Bar::default();
        let start = Instant::now();
        let released = start + Duration::from_secs(5);
        bar.press(key(2), typed("a", none()), start);
        bar.release(key(2), released);
        bar.clear_if_idle(released);
        assert_eq!(bar.caps().len(), 1);
        bar.clear_if_idle(released + Duration::from_secs(2));
        assert!(bar.caps().is_empty());
    }

    #[test]
    fn idle_time_counts_from_the_keyboard_going_away() {
        let mut bar = Bar::default();
        let start = Instant::now();
        let gone = start + Duration::from_secs(5);
        bar.press(key(2), typed("a", none()), start);
        bar.keyboard_gone(KB, gone);
        bar.clear_if_idle(gone);
        assert_eq!(bar.caps().len(), 1);
        bar.clear_if_idle(gone + Duration::from_secs(2));
        assert!(bar.caps().is_empty());
    }

    #[test]
    fn counted_cap_stays_lit_while_any_key_merged_into_it_is_held() {
        let mut bar = Bar::default();
        let now = Instant::now();
        let on_other = KeyId {
            keyboard: OTHER_KB,
            code: 28,
        };
        bar.press(key(28), shortcut("Enter", none()), now);
        bar.press(on_other, shortcut("Enter", none()), now);
        bar.release(key(28), now);
        assert!(bar.caps()[0].lit);
        bar.release(on_other, now);
        assert!(!bar.caps()[0].lit);
    }

    #[test]
    fn counted_cap_stays_lit_when_one_keyboard_holding_it_goes_away() {
        let mut bar = Bar::default();
        let now = Instant::now();
        let on_other = KeyId {
            keyboard: OTHER_KB,
            code: 28,
        };
        bar.press(key(28), shortcut("Enter", none()), now);
        bar.press(on_other, shortcut("Enter", none()), now);
        bar.keyboard_gone(KB, now);
        assert!(bar.caps()[0].lit);
    }

    #[test]
    fn shift_cap_returns_after_it_was_dropped_while_shift_is_held() {
        let mut bar = Bar::default();
        let now = Instant::now();
        bar.press(key(1), modifier(Modifier::Shift, none()), now);
        bar.press(key(2), shortcut("Enter", shift()), now);
        bar.drop_oldest();
        bar.drop_oldest();
        bar.press(key(3), shortcut("Tab", shift()), now);
        assert_eq!(labels(&bar), ["Shift", "Tab"]);
    }
}
