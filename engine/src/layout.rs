//! Character to key mapping for typing text, per host keyboard layout.
//!
//! A USB keyboard sends key positions; the computer turns them into
//! characters using its own layout. To type a character, the device has to
//! know which key (and modifiers) produce it on that layout. Dead keys
//! (e.g. `^` and `´` on German keyboards) are not supported.

use serde::{Deserialize, Serialize};

use crate::config::{KeyCode, Modifiers};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeyboardLayout {
    /// US QWERTY.
    Us,
    /// German QWERTZ (ISO).
    De,
}

const SHIFT: u8 = Modifiers::LSHIFT.0;
const ALTGR: u8 = Modifiers::RALT.0;

fn key(code: u8, mods: u8) -> Option<(KeyCode, Modifiers)> {
    Some((KeyCode(code), Modifiers(mods)))
}

/// The key and modifiers that type `c` on `layout`, if it can be typed.
pub fn char_to_key(layout: KeyboardLayout, c: char) -> Option<(KeyCode, Modifiers)> {
    // Shared by both layouts.
    match c {
        '\n' => return key(0x28, 0),
        '\t' => return key(0x2B, 0),
        ' ' => return key(0x2C, 0),
        '1'..='9' => return key(0x1E + (c as u8 - b'1'), 0),
        '0' => return key(0x27, 0),
        _ => {}
    }
    match layout {
        KeyboardLayout::Us => us(c),
        KeyboardLayout::De => de(c),
    }
}

fn letter(c: char, swap_yz: bool) -> Option<(KeyCode, Modifiers)> {
    let lower = c.to_ascii_lowercase();
    if !lower.is_ascii_lowercase() {
        return None;
    }
    let pos = match (swap_yz, lower) {
        (true, 'y') => 'z',
        (true, 'z') => 'y',
        (_, l) => l,
    };
    key(0x04 + (pos as u8 - b'a'), if c.is_ascii_uppercase() { SHIFT } else { 0 })
}

fn us(c: char) -> Option<(KeyCode, Modifiers)> {
    if c.is_ascii_alphabetic() {
        return letter(c, false);
    }
    let (code, mods) = match c {
        '!' => (0x1E, SHIFT),
        '@' => (0x1F, SHIFT),
        '#' => (0x20, SHIFT),
        '$' => (0x21, SHIFT),
        '%' => (0x22, SHIFT),
        '^' => (0x23, SHIFT),
        '&' => (0x24, SHIFT),
        '*' => (0x25, SHIFT),
        '(' => (0x26, SHIFT),
        ')' => (0x27, SHIFT),
        '-' => (0x2D, 0),
        '_' => (0x2D, SHIFT),
        '=' => (0x2E, 0),
        '+' => (0x2E, SHIFT),
        '[' => (0x2F, 0),
        '{' => (0x2F, SHIFT),
        ']' => (0x30, 0),
        '}' => (0x30, SHIFT),
        '\\' => (0x31, 0),
        '|' => (0x31, SHIFT),
        ';' => (0x33, 0),
        ':' => (0x33, SHIFT),
        '\'' => (0x34, 0),
        '"' => (0x34, SHIFT),
        '`' => (0x35, 0),
        '~' => (0x35, SHIFT),
        ',' => (0x36, 0),
        '<' => (0x36, SHIFT),
        '.' => (0x37, 0),
        '>' => (0x37, SHIFT),
        '/' => (0x38, 0),
        '?' => (0x38, SHIFT),
        _ => return None,
    };
    key(code, mods)
}

fn de(c: char) -> Option<(KeyCode, Modifiers)> {
    if c.is_ascii_alphabetic() {
        return letter(c, true);
    }
    let (code, mods) = match c {
        '!' => (0x1E, SHIFT),
        '"' => (0x1F, SHIFT),
        '²' => (0x1F, ALTGR),
        '§' => (0x20, SHIFT),
        '³' => (0x20, ALTGR),
        '$' => (0x21, SHIFT),
        '%' => (0x22, SHIFT),
        '&' => (0x23, SHIFT),
        '/' => (0x24, SHIFT),
        '{' => (0x24, ALTGR),
        '(' => (0x25, SHIFT),
        '[' => (0x25, ALTGR),
        ')' => (0x26, SHIFT),
        ']' => (0x26, ALTGR),
        '=' => (0x27, SHIFT),
        '}' => (0x27, ALTGR),
        'ß' => (0x2D, 0),
        '?' => (0x2D, SHIFT),
        '\\' => (0x2D, ALTGR),
        'ü' => (0x2F, 0),
        'Ü' => (0x2F, SHIFT),
        '+' => (0x30, 0),
        '*' => (0x30, SHIFT),
        '~' => (0x30, ALTGR),
        '#' => (0x32, 0),
        '\'' => (0x32, SHIFT),
        'ö' => (0x33, 0),
        'Ö' => (0x33, SHIFT),
        'ä' => (0x34, 0),
        'Ä' => (0x34, SHIFT),
        '°' => (0x35, SHIFT),
        ',' => (0x36, 0),
        ';' => (0x36, SHIFT),
        '.' => (0x37, 0),
        ':' => (0x37, SHIFT),
        '-' => (0x38, 0),
        '_' => (0x38, SHIFT),
        '<' => (0x64, 0),
        '>' => (0x64, SHIFT),
        '|' => (0x64, ALTGR),
        '@' => (0x14, ALTGR),
        '€' => (0x08, ALTGR),
        'µ' => (0x10, ALTGR),
        _ => return None,
    };
    key(code, mods)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn k(layout: KeyboardLayout, c: char) -> Option<(u8, u8)> {
        char_to_key(layout, c).map(|(code, mods)| (code.0, mods.0))
    }

    #[test]
    fn us_layout() {
        use KeyboardLayout::Us;
        assert_eq!(k(Us, 'a'), Some((0x04, 0)));
        assert_eq!(k(Us, 'Z'), Some((0x1D, SHIFT)));
        assert_eq!(k(Us, '0'), Some((0x27, 0)));
        assert_eq!(k(Us, '@'), Some((0x1F, SHIFT)));
        assert_eq!(k(Us, '\n'), Some((0x28, 0)));
        assert_eq!(k(Us, 'ä'), None);
    }

    #[test]
    fn german_layout() {
        use KeyboardLayout::De;
        assert_eq!(k(De, 'z'), Some((0x1C, 0)));
        assert_eq!(k(De, 'Y'), Some((0x1D, SHIFT)));
        assert_eq!(k(De, '@'), Some((0x14, ALTGR)));
        assert_eq!(k(De, 'ß'), Some((0x2D, 0)));
        assert_eq!(k(De, 'Ö'), Some((0x33, SHIFT)));
        assert_eq!(k(De, '#'), Some((0x32, 0)));
        assert_eq!(k(De, '^'), None); // dead key
    }

    #[test]
    fn every_printable_ascii_char_types_on_us() {
        for c in (0x20u8..0x7F).map(char::from) {
            assert!(k(KeyboardLayout::Us, c).is_some(), "{c:?}");
        }
    }

    #[test]
    fn german_covers_ascii_except_dead_keys() {
        for c in (0x20u8..0x7F).map(char::from) {
            let expected = !matches!(c, '^' | '`');
            assert_eq!(k(KeyboardLayout::De, c).is_some(), expected, "{c:?}");
        }
    }
}
