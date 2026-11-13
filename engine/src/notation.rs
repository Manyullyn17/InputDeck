//! Short text notation for bindings and colors, as used in config files.
//!
//! | Text | Meaning |
//! | --- | --- |
//! | `_` | inherit |
//! | `none` or empty | nothing |
//! | `layer:<id>` | hold for layer `<id>` |
//! | `a`, `7`, `f5`, `enter`, `left`, `-`, ... | a key |
//! | `ctrl+shift+t`, `rctrl+c`, `gui+space` | a key with modifiers |
//! | `0x2c`, `ctrl+0x2c` | a raw HID keyboard usage |
//! | `vol+`, `mute`, `play`, ... | a media key |
//! | `media:0xb5` | a raw consumer-control usage |
//! | `#rrggbb`, `off`, `_` | colors |
//!
//! Parsing is case-insensitive. Formatting produces the canonical names.

use alloc::string::String;
use core::fmt::{self, Write};

use crate::config::{Action, EncoderBinding, KeyBinding, KeyCode, LayerId, Modifiers};
use crate::led::{Color, Rgb};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseError {
    UnknownKey,
    UnknownModifier,
    UnknownLayer,
    BadNumber,
    BadColor,
    /// A layer key where only actions are allowed.
    LayerNotAllowed,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ParseError::UnknownKey => "unknown key name",
            ParseError::UnknownModifier => "unknown modifier",
            ParseError::UnknownLayer => "unknown layer",
            ParseError::BadNumber => "invalid number",
            ParseError::BadColor => "invalid color (expected #rrggbb, off or _)",
            ParseError::LayerNotAllowed => "layer keys are not allowed here",
        })
    }
}

/// Named keys; the first name for a code is canonical.
const KEYS: &[(&str, u8)] = &[
    ("enter", 0x28),
    ("return", 0x28),
    ("esc", 0x29),
    ("escape", 0x29),
    ("backspace", 0x2A),
    ("bksp", 0x2A),
    ("tab", 0x2B),
    ("space", 0x2C),
    ("spc", 0x2C),
    ("-", 0x2D),
    ("minus", 0x2D),
    ("=", 0x2E),
    ("equal", 0x2E),
    ("[", 0x2F),
    ("lbracket", 0x2F),
    ("]", 0x30),
    ("rbracket", 0x30),
    ("\\", 0x31),
    ("backslash", 0x31),
    (";", 0x33),
    ("semicolon", 0x33),
    ("'", 0x34),
    ("quote", 0x34),
    ("`", 0x35),
    ("grave", 0x35),
    (",", 0x36),
    ("comma", 0x36),
    (".", 0x37),
    ("period", 0x37),
    ("dot", 0x37),
    ("/", 0x38),
    ("slash", 0x38),
    ("capslock", 0x39),
    ("caps", 0x39),
    ("printscreen", 0x46),
    ("prtsc", 0x46),
    ("scrolllock", 0x47),
    ("pause", 0x48),
    ("insert", 0x49),
    ("ins", 0x49),
    ("home", 0x4A),
    ("pageup", 0x4B),
    ("pgup", 0x4B),
    ("delete", 0x4C),
    ("del", 0x4C),
    ("end", 0x4D),
    ("pagedown", 0x4E),
    ("pgdn", 0x4E),
    ("right", 0x4F),
    ("left", 0x50),
    ("down", 0x51),
    ("up", 0x52),
    ("numlock", 0x53),
    ("menu", 0x65),
    ("app", 0x65),
];

const MEDIA: &[(&str, u16)] = &[
    ("vol+", 0xE9),
    ("volup", 0xE9),
    ("vol-", 0xEA),
    ("voldown", 0xEA),
    ("mute", 0xE2),
    ("play", 0xCD),
    ("playpause", 0xCD),
    ("next", 0xB5),
    ("prev", 0xB6),
    ("stop", 0xB7),
    ("bri+", 0x6F),
    ("bri-", 0x70),
];

/// Modifier names; the first name for a bit is canonical.
const MODS: &[(&str, u8)] = &[
    ("ctrl", 0x01),
    ("control", 0x01),
    ("lctrl", 0x01),
    ("shift", 0x02),
    ("lshift", 0x02),
    ("alt", 0x04),
    ("option", 0x04),
    ("lalt", 0x04),
    ("gui", 0x08),
    ("super", 0x08),
    ("win", 0x08),
    ("cmd", 0x08),
    ("lgui", 0x08),
    ("rctrl", 0x10),
    ("rshift", 0x20),
    ("ralt", 0x40),
    ("altgr", 0x40),
    ("rgui", 0x80),
];

fn lower(s: &str) -> String {
    s.trim().to_ascii_lowercase()
}

fn parse_hex<T: TryFrom<u32>>(s: &str) -> Result<T, ParseError> {
    let digits = s.strip_prefix("0x").ok_or(ParseError::BadNumber)?;
    let n = u32::from_str_radix(digits, 16).map_err(|_| ParseError::BadNumber)?;
    T::try_from(n).map_err(|_| ParseError::BadNumber)
}

fn parse_key(s: &str) -> Result<KeyCode, ParseError> {
    let b = s.as_bytes();
    if b.len() == 1 && b[0].is_ascii_lowercase() {
        return Ok(KeyCode(0x04 + b[0] - b'a'));
    }
    if b.len() == 1 && b[0].is_ascii_digit() {
        return Ok(KeyCode(if b[0] == b'0' { 0x27 } else { 0x1E + b[0] - b'1' }));
    }
    if let Some(n) = s.strip_prefix('f').and_then(|n| n.parse::<u8>().ok()) {
        return match n {
            1..=12 => Ok(KeyCode(0x3A + n - 1)),
            13..=24 => Ok(KeyCode(0x68 + n - 13)),
            _ => Err(ParseError::UnknownKey),
        };
    }
    if s.starts_with("0x") {
        return parse_hex(s).map(KeyCode);
    }
    KEYS.iter().find(|(name, _)| *name == s).map(|&(_, c)| KeyCode(c)).ok_or(ParseError::UnknownKey)
}

/// Parses an action (key, key with modifiers, or media key).
pub fn parse_action(text: &str) -> Result<Action, ParseError> {
    let s = lower(text);
    if let Some(&(_, usage)) = MEDIA.iter().find(|(name, _)| *name == s) {
        return Ok(Action::Consumer(usage));
    }
    if let Some(raw) = s.strip_prefix("media:") {
        return parse_hex(raw).map(Action::Consumer);
    }
    // A trailing "+" can't be split off as a separator ("ctrl++" is not supported).
    let (mod_part, key_part) = match s.rfind('+') {
        Some(i) if i + 1 < s.len() => (&s[..i], &s[i + 1..]),
        _ => ("", s.as_str()),
    };
    let mut mods = 0u8;
    if !mod_part.is_empty() {
        for m in mod_part.split('+') {
            let &(_, bit) = MODS.iter().find(|(name, _)| *name == m).ok_or(ParseError::UnknownModifier)?;
            mods |= bit;
        }
    }
    Ok(Action::Key { code: parse_key(key_part)?, mods: Modifiers(mods) })
}

/// Parses a key binding. `layer` maps a layer id from `layer:<id>` to its
/// index.
pub fn parse_binding(text: &str, layer: impl Fn(&str) -> Option<LayerId>) -> Result<KeyBinding, ParseError> {
    let t = text.trim();
    match lower(t).as_str() {
        "_" => return Ok(KeyBinding::Inherit),
        "" | "none" => return Ok(KeyBinding::None),
        _ => {}
    }
    if let Some(id) = t.strip_prefix("layer:") {
        return layer(id.trim()).map(KeyBinding::Layer).ok_or(ParseError::UnknownLayer);
    }
    parse_action(t).map(KeyBinding::Action)
}

pub fn parse_encoder(text: &str) -> Result<EncoderBinding, ParseError> {
    match lower(text).as_str() {
        "_" => Ok(EncoderBinding::Inherit),
        "" | "none" => Ok(EncoderBinding::None),
        s if s.starts_with("layer:") => Err(ParseError::LayerNotAllowed),
        _ => parse_action(text).map(EncoderBinding::Action),
    }
}

pub fn parse_color(text: &str) -> Result<Color, ParseError> {
    let s = lower(text);
    match s.as_str() {
        "_" | "inherit" => return Ok(Color::Inherit),
        "off" => return Ok(Color::Off),
        _ => {}
    }
    let hex = s.strip_prefix('#').filter(|h| h.len() == 6).ok_or(ParseError::BadColor)?;
    let n = u32::from_str_radix(hex, 16).map_err(|_| ParseError::BadColor)?;
    Ok(Color::Rgb(Rgb::new((n >> 16) as u8, (n >> 8) as u8, n as u8)))
}

pub fn write_action(w: &mut impl Write, action: Action) -> fmt::Result {
    match action {
        Action::Consumer(usage) => match MEDIA.iter().find(|&&(_, u)| u == usage) {
            Some((name, _)) => w.write_str(name),
            None => write!(w, "media:0x{usage:02x}"),
        },
        Action::Key { code, mods } => {
            for bit in (0..8).map(|i| 1u8 << i) {
                if mods.0 & bit != 0 {
                    let (name, _) = MODS.iter().find(|&&(_, b)| b == bit).unwrap();
                    write!(w, "{name}+")?;
                }
            }
            let c = code.0;
            match c {
                0x04..=0x1D => w.write_char((b'a' + c - 0x04) as char),
                0x1E..=0x26 => w.write_char((b'1' + c - 0x1E) as char),
                0x27 => w.write_char('0'),
                0x3A..=0x45 => write!(w, "f{}", c - 0x3A + 1),
                0x68..=0x73 => write!(w, "f{}", c - 0x68 + 13),
                _ => match KEYS.iter().find(|&&(_, k)| k == c) {
                    Some((name, _)) => w.write_str(name),
                    None => write!(w, "0x{c:02x}"),
                },
            }
        }
    }
}

/// Formats a binding; `layer` gives the id written after `layer:`.
pub fn write_binding<W: Write>(
    w: &mut W,
    binding: KeyBinding,
    layer: impl Fn(&mut W, LayerId) -> fmt::Result,
) -> fmt::Result {
    match binding {
        KeyBinding::Inherit => w.write_char('_'),
        KeyBinding::None => w.write_str("none"),
        KeyBinding::Layer(id) => {
            w.write_str("layer:")?;
            layer(w, id)
        }
        KeyBinding::Action(a) => write_action(w, a),
    }
}

pub fn write_color(w: &mut impl Write, color: Color) -> fmt::Result {
    match color {
        Color::Inherit => w.write_char('_'),
        Color::Off => w.write_str("off"),
        Color::Rgb(Rgb { r, g, b }) => write!(w, "#{r:02x}{g:02x}{b:02x}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layers(id: &str) -> Option<LayerId> {
        ["base", "nav"].iter().position(|&n| n == id).map(|i| LayerId(i as u8))
    }

    fn key(code: u8, mods: u8) -> Action {
        Action::Key { code: KeyCode(code), mods: Modifiers(mods) }
    }

    #[test]
    fn parses_bindings() {
        assert_eq!(parse_binding("_", layers), Ok(KeyBinding::Inherit));
        assert_eq!(parse_binding("", layers), Ok(KeyBinding::None));
        assert_eq!(parse_binding("layer:nav", layers), Ok(KeyBinding::Layer(LayerId(1))));
        assert_eq!(parse_binding("layer:nope", layers), Err(ParseError::UnknownLayer));
        assert_eq!(parse_binding("A", layers), Ok(KeyBinding::Action(key(0x04, 0))));
    }

    #[test]
    fn parses_actions() {
        assert_eq!(parse_action("0"), Ok(key(0x27, 0)));
        assert_eq!(parse_action("f13"), Ok(key(0x68, 0)));
        assert_eq!(parse_action("Ctrl+Shift+T"), Ok(key(0x17, 0x03)));
        assert_eq!(parse_action("rctrl+-"), Ok(key(0x2D, 0x10)));
        assert_eq!(parse_action("gui+space"), Ok(key(0x2C, 0x08)));
        assert_eq!(parse_action("0x64"), Ok(key(0x64, 0)));
        assert_eq!(parse_action("vol+"), Ok(Action::Consumer(0xE9)));
        assert_eq!(parse_action("media:0x183"), Ok(Action::Consumer(0x183)));
        assert_eq!(parse_action("ctlr+c"), Err(ParseError::UnknownModifier));
        assert_eq!(parse_action("foo"), Err(ParseError::UnknownKey));
        assert_eq!(parse_action("f25"), Err(ParseError::UnknownKey));
    }

    #[test]
    fn parses_colors() {
        assert_eq!(parse_color("#FF0080"), Ok(Color::Rgb(Rgb::new(255, 0, 128))));
        assert_eq!(parse_color("off"), Ok(Color::Off));
        assert_eq!(parse_color("_"), Ok(Color::Inherit));
        assert_eq!(parse_color("red"), Err(ParseError::BadColor));
    }

    #[test]
    fn encoder_rejects_layers() {
        assert_eq!(parse_encoder("layer:nav"), Err(ParseError::LayerNotAllowed));
        assert_eq!(parse_encoder("vol-"), Ok(EncoderBinding::Action(Action::Consumer(0xEA))));
    }

    #[test]
    fn format_parse_roundtrip() {
        for text in ["a", "0", "f24", "ctrl+shift+t", "rgui+enter", "-", "0x64", "vol+", "media:0x183"] {
            let action = parse_action(text).unwrap();
            let mut out = String::new();
            write_action(&mut out, action).unwrap();
            assert_eq!(out, text);
            assert_eq!(parse_action(&out), Ok(action));
        }
        for code in 0..=255u8 {
            for mods in [0u8, 0x05, 0xF0] {
                let action = key(code, mods);
                let mut out = String::new();
                write_action(&mut out, action).unwrap();
                assert_eq!(parse_action(&out), Ok(action), "{out}");
            }
        }
    }
}
