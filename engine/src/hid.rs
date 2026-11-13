//! Turns engine [`Output`]s into USB HID report contents.
//!
//! Actions are reference-counted, so two buttons sending the same key keep
//! it pressed until both are released. The caller should send reports after
//! every applied output (not once per event), or an encoder tap
//! (press + release in one event) would never reach the host.
//!
//! Keys typed by text macros (`TypePress`) are reported with exactly their
//! own modifiers: while one is down, other held modifiers are suppressed,
//! so typed text comes out right even while e.g. Shift is held.

use crate::config::{Action, Modifiers};
use crate::engine::Output;

/// Standard 6-key-rollover boot keyboard report.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct KeyboardReport {
    pub modifiers: u8,
    pub keys: [u8; 6],
}

const ERROR_ROLLOVER: u8 = 0x01;

/// Key codes covered by the NKRO bitmap: 0x00..NKRO_KEYS.
pub const NKRO_KEYS: usize = 224;
/// NKRO report: modifier byte, then one bit per key code.
pub const NKRO_REPORT_LEN: usize = 1 + NKRO_KEYS / 8;
const CONSUMER_SLOTS: usize = 4;

#[derive(Clone, Debug)]
pub struct HidState {
    key_counts: [u8; 256],
    mod_counts: [u8; 8],
    typed_counts: [u8; 256],
    typed_mod_counts: [u8; 8],
    typed_down: u16,
    /// Pressed consumer usages in press order; the newest is reported.
    consumer: [u16; CONSUMER_SLOTS],
    consumer_len: usize,
}

fn adjust(counts: &mut [u8; 8], mods: Modifiers, press: bool) {
    for (bit, count) in counts.iter_mut().enumerate() {
        if mods.0 & (1 << bit) != 0 {
            *count = if press { count.saturating_add(1) } else { count.saturating_sub(1) };
        }
    }
}

impl Default for HidState {
    fn default() -> Self {
        Self::new()
    }
}

impl HidState {
    pub const fn new() -> Self {
        HidState {
            key_counts: [0; 256],
            mod_counts: [0; 8],
            typed_counts: [0; 256],
            typed_mod_counts: [0; 8],
            typed_down: 0,
            consumer: [0; CONSUMER_SLOTS],
            consumer_len: 0,
        }
    }

    pub fn apply(&mut self, output: Output) {
        match output {
            Output::Press(Action::Key { code, mods }) => {
                let c = &mut self.key_counts[code.0 as usize];
                *c = c.saturating_add(1);
                self.adjust_mods(mods, true);
            }
            Output::Release(Action::Key { code, mods }) => {
                let c = &mut self.key_counts[code.0 as usize];
                *c = c.saturating_sub(1);
                self.adjust_mods(mods, false);
            }
            Output::Press(Action::Consumer(usage)) => {
                if self.consumer_len < CONSUMER_SLOTS {
                    self.consumer[self.consumer_len] = usage;
                    self.consumer_len += 1;
                }
            }
            Output::Release(Action::Consumer(usage)) => {
                let pressed = &self.consumer[..self.consumer_len];
                if let Some(pos) = pressed.iter().rposition(|&u| u == usage) {
                    self.consumer.copy_within(pos + 1..self.consumer_len, pos);
                    self.consumer_len -= 1;
                }
            }
            Output::TypePress { code, mods } => {
                let c = &mut self.typed_counts[code.0 as usize];
                *c = c.saturating_add(1);
                adjust(&mut self.typed_mod_counts, mods, true);
                self.typed_down = self.typed_down.saturating_add(1);
            }
            Output::TypeRelease { code, mods } => {
                let c = &mut self.typed_counts[code.0 as usize];
                *c = c.saturating_sub(1);
                adjust(&mut self.typed_mod_counts, mods, false);
                self.typed_down = self.typed_down.saturating_sub(1);
            }
            Output::LayerChanged(_) | Output::System(_) => {}
        }
    }

    fn adjust_mods(&mut self, mods: Modifiers, press: bool) {
        adjust(&mut self.mod_counts, mods, press);
    }

    pub fn keyboard_report(&self) -> KeyboardReport {
        let mut report = KeyboardReport::default();
        let mods = if self.typed_down > 0 { &self.typed_mod_counts } else { &self.mod_counts };
        for (bit, &count) in mods.iter().enumerate() {
            if count > 0 {
                report.modifiers |= 1 << bit;
            }
        }
        let mut n = 0;
        // Code 0 means "no key" (modifier-only actions).
        for (code, (&count, &typed)) in self.key_counts.iter().zip(&self.typed_counts).enumerate().skip(1) {
            if count == 0 && typed == 0 {
                continue;
            }
            if n == report.keys.len() {
                report.keys = [ERROR_ROLLOVER; 6];
                break;
            }
            report.keys[n] = code as u8;
            n += 1;
        }
        report
    }

    /// N-key-rollover report: `[modifiers, bitmap...]`, bit `c % 8` of byte
    /// `1 + c / 8` set while key code `c` is down. No key limit.
    pub fn nkro_report(&self) -> [u8; NKRO_REPORT_LEN] {
        let mut report = [0u8; NKRO_REPORT_LEN];
        let mods = if self.typed_down > 0 { &self.typed_mod_counts } else { &self.mod_counts };
        for (bit, &count) in mods.iter().enumerate() {
            if count > 0 {
                report[0] |= 1 << bit;
            }
        }
        // Code 0 means "no key" (modifier-only actions).
        for code in 1..NKRO_KEYS {
            if self.key_counts[code] > 0 || self.typed_counts[code] > 0 {
                report[1 + code / 8] |= 1 << (code % 8);
            }
        }
        report
    }

    /// The consumer usage to report, or 0 for none.
    pub fn consumer_report(&self) -> u16 {
        match self.consumer_len {
            0 => 0,
            n => self.consumer[n - 1],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::KeyCode;
    use crate::testutil::act;

    #[test]
    fn shared_key_stays_down_until_all_released() {
        let mut h = HidState::new();
        h.apply(Output::Press(act(0x04)));
        h.apply(Output::Press(act(0x04)));
        h.apply(Output::Release(act(0x04)));
        assert_eq!(h.keyboard_report().keys[0], 0x04);
        h.apply(Output::Release(act(0x04)));
        assert_eq!(h.keyboard_report(), KeyboardReport::default());
    }

    #[test]
    fn modifiers_are_refcounted() {
        let mut h = HidState::new();
        let shift_a = Action::Key { code: KeyCode(0x04), mods: Modifiers::LSHIFT };
        let shift_b = Action::Key { code: KeyCode(0x05), mods: Modifiers::LSHIFT };
        h.apply(Output::Press(shift_a));
        h.apply(Output::Press(shift_b));
        h.apply(Output::Release(shift_a));
        let r = h.keyboard_report();
        assert_eq!(r.modifiers, Modifiers::LSHIFT.0);
        assert_eq!(r.keys, [0x05, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn more_than_six_keys_reports_rollover_error() {
        let mut h = HidState::new();
        for code in 0x04..0x0B {
            h.apply(Output::Press(act(code)));
        }
        assert_eq!(h.keyboard_report().keys, [ERROR_ROLLOVER; 6]);
    }

    #[test]
    fn typed_keys_use_exactly_their_modifiers() {
        let mut h = HidState::new();
        let shift_held = Action::Key { code: KeyCode(0xE1), mods: Modifiers::LSHIFT };
        h.apply(Output::Press(shift_held));
        h.apply(Output::TypePress { code: KeyCode(0x04), mods: Modifiers::NONE });
        let r = h.keyboard_report();
        assert_eq!(r.modifiers, 0); // shift suppressed while typing 'a'
        assert!(r.keys.contains(&0x04) && r.keys.contains(&0xE1));
        h.apply(Output::TypeRelease { code: KeyCode(0x04), mods: Modifiers::NONE });
        assert_eq!(h.keyboard_report().modifiers, Modifiers::LSHIFT.0);
    }

    #[test]
    fn nkro_reports_any_number_of_keys() {
        let mut h = HidState::new();
        for code in 0x04..0x10 {
            h.apply(Output::Press(act(code)));
        }
        h.apply(Output::Press(Action::Key { code: KeyCode(0), mods: Modifiers::LCTRL }));
        let r = h.nkro_report();
        assert_eq!(r[0], Modifiers::LCTRL.0);
        for code in 0..NKRO_KEYS {
            let down = r[1 + code / 8] & (1 << (code % 8)) != 0;
            assert_eq!(down, (0x04..0x10).contains(&code), "code {code:#x}");
        }
        // The boot report can't hold 12 keys.
        assert_eq!(h.keyboard_report().keys, [ERROR_ROLLOVER; 6]);
    }

    #[test]
    fn consumer_reports_newest_pressed() {
        let mut h = HidState::new();
        h.apply(Output::Press(Action::Consumer(0xE9)));
        h.apply(Output::Press(Action::Consumer(0xE2)));
        assert_eq!(h.consumer_report(), 0xE2);
        h.apply(Output::Release(Action::Consumer(0xE2)));
        assert_eq!(h.consumer_report(), 0xE9);
        h.apply(Output::Release(Action::Consumer(0xE9)));
        assert_eq!(h.consumer_report(), 0);
    }
}
