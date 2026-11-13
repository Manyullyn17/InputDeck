//! Turns engine [`Output`]s into USB HID report contents.
//!
//! Actions are reference-counted, so two buttons sending the same key keep
//! it pressed until both are released. The caller should send reports after
//! every applied output (not once per event), or an encoder tap
//! (press + release in one event) would never reach the host.

use crate::config::{Action, Modifiers};
use crate::engine::Output;

/// Standard 6-key-rollover boot keyboard report.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct KeyboardReport {
    pub modifiers: u8,
    pub keys: [u8; 6],
}

const ERROR_ROLLOVER: u8 = 0x01;
const CONSUMER_SLOTS: usize = 4;

#[derive(Clone, Debug)]
pub struct HidState {
    key_counts: [u8; 256],
    mod_counts: [u8; 8],
    /// Pressed consumer usages in press order; the newest is reported.
    consumer: [u16; CONSUMER_SLOTS],
    consumer_len: usize,
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
            Output::LayerChanged(_) => {}
        }
    }

    fn adjust_mods(&mut self, mods: Modifiers, press: bool) {
        for (bit, count) in self.mod_counts.iter_mut().enumerate() {
            if mods.0 & (1 << bit) != 0 {
                *count = if press { count.saturating_add(1) } else { count.saturating_sub(1) };
            }
        }
    }

    pub fn keyboard_report(&self) -> KeyboardReport {
        let mut report = KeyboardReport::default();
        for (bit, &count) in self.mod_counts.iter().enumerate() {
            if count > 0 {
                report.modifiers |= 1 << bit;
            }
        }
        let mut n = 0;
        for (code, &count) in self.key_counts.iter().enumerate() {
            if count == 0 {
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
