//! Short human-readable labels for bindings, sized for the 128x64 OLED
//! (about 7 characters per key cell). Shared with the desktop app.

use core::fmt::{self, Write};

use crate::config::{Action, KeyBinding, KeyCode, Modifiers};

/// Writes a short label for an effective (already resolved) binding.
/// `None` and `Inherit` produce no text.
pub fn write_binding(w: &mut impl Write, binding: KeyBinding) -> fmt::Result {
    match binding {
        KeyBinding::None | KeyBinding::Inherit => Ok(()),
        KeyBinding::Layer(id) => write!(w, ">L{}", id.0),
        KeyBinding::Action(action) => write_action(w, action),
    }
}

pub fn write_action(w: &mut impl Write, action: Action) -> fmt::Result {
    match action {
        Action::Key { code, mods } => {
            write_mods(w, mods)?;
            write_key(w, code)
        }
        Action::Consumer(usage) => match consumer_name(usage) {
            Some(name) => w.write_str(name),
            None => write!(w, "c{usage:X}"),
        },
    }
}

/// Modifiers as a compact prefix, e.g. "CS-" for Ctrl+Shift. Left and
/// right variants share a letter.
fn write_mods(w: &mut impl Write, mods: Modifiers) -> fmt::Result {
    let m = mods.0 | (mods.0 >> 4);
    if m & 0x0F == 0 {
        return Ok(());
    }
    for (bit, letter) in [(0x01, 'C'), (0x02, 'S'), (0x04, 'A'), (0x08, 'G')] {
        if m & bit != 0 {
            w.write_char(letter)?;
        }
    }
    w.write_char('-')
}

fn write_key(w: &mut impl Write, code: KeyCode) -> fmt::Result {
    let c = code.0;
    match c {
        0x04..=0x1D => w.write_char((b'A' + c - 0x04) as char),
        0x1E..=0x26 => w.write_char((b'1' + c - 0x1E) as char),
        0x27 => w.write_char('0'),
        0x3A..=0x45 => write!(w, "F{}", c - 0x3A + 1),
        0x68..=0x73 => write!(w, "F{}", c - 0x68 + 13),
        _ => match key_name(c) {
            Some(name) => w.write_str(name),
            None => write!(w, "x{c:02X}"),
        },
    }
}

fn key_name(code: u8) -> Option<&'static str> {
    Some(match code {
        0x28 => "Ent",
        0x29 => "Esc",
        0x2A => "Bksp",
        0x2B => "Tab",
        0x2C => "Spc",
        0x2D => "-",
        0x2E => "=",
        0x2F => "[",
        0x30 => "]",
        0x31 => "\\",
        0x33 => ";",
        0x34 => "'",
        0x35 => "`",
        0x36 => ",",
        0x37 => ".",
        0x38 => "/",
        0x39 => "Caps",
        0x46 => "PrtSc",
        0x47 => "ScrLk",
        0x48 => "Pause",
        0x49 => "Ins",
        0x4A => "Home",
        0x4B => "PgUp",
        0x4C => "Del",
        0x4D => "End",
        0x4E => "PgDn",
        0x4F => "Right",
        0x50 => "Left",
        0x51 => "Down",
        0x52 => "Up",
        0x65 => "Menu",
        _ => return None,
    })
}

fn consumer_name(usage: u16) -> Option<&'static str> {
    Some(match usage {
        0xB5 => "Next",
        0xB6 => "Prev",
        0xB7 => "Stop",
        0xCD => "Play",
        0xE2 => "Mute",
        0xE9 => "Vol+",
        0xEA => "Vol-",
        0x6F => "Bri+",
        0x70 => "Bri-",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::LayerId;

    fn label(binding: KeyBinding) -> String {
        let mut s = String::new();
        write_binding(&mut s, binding).unwrap();
        s
    }

    fn key(code: u8, mods: Modifiers) -> KeyBinding {
        KeyBinding::Action(Action::Key { code: KeyCode(code), mods })
    }

    #[test]
    fn labels() {
        assert_eq!(label(key(0x04, Modifiers::NONE)), "A");
        assert_eq!(label(key(0x27, Modifiers::NONE)), "0");
        assert_eq!(label(key(0x45, Modifiers::NONE)), "F12");
        assert_eq!(label(key(0x50, Modifiers::NONE)), "Left");
        assert_eq!(label(key(0x06, Modifiers(Modifiers::LCTRL.0 | Modifiers::RSHIFT.0))), "CS-C");
        assert_eq!(label(key(0xE0, Modifiers::NONE)), "xE0");
        assert_eq!(label(KeyBinding::Action(Action::Consumer(0xE9))), "Vol+");
        assert_eq!(label(KeyBinding::Action(Action::Consumer(0x123))), "c123");
        assert_eq!(label(KeyBinding::Layer(LayerId(3))), ">L3");
        assert_eq!(label(KeyBinding::None), "");
    }
}
