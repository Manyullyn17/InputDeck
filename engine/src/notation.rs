//! Short text notation for bindings and colors, as used in config files.
//!
//! | Text | Meaning |
//! | --- | --- |
//! | `_` | inherit |
//! | `none` or empty | nothing |
//! | `layer:<id>` | hold for layer `<id>` |
//! | `toggle:<id>` | toggle layer `<id>` on/off |
//! | `latch:<action>` | hold `<action>` down until pressed again |
//! | `repeat:<action>` / `repeat:<action>@<ms>` | tap `<action>` repeatedly (default every 100 ms) until pressed again |
//! | `macro:<id>` | run macro `<id>` |
//! | `settings`, `profile:next`, `profile:prev`, `profile:<n>` (1-based), `bootloader` | device functions |
//! | `a`, `7`, `f5`, `enter`, `left`, `-`, ... | a key |
//! | `ctrl+shift+t`, `rctrl+c`, `gui+space` | a key with modifiers |
//! | `0x2c`, `ctrl+0x2c` | a raw HID keyboard usage |
//! | `kp7`, `kpenter`, `nonusbackslash`, `f13`, `undo`, ... | more keys (see `KEYS`) |
//! | `vol+`, `mute`, `playpause`, `calc`, `browser_back`, ... | a media / launch key (see `MEDIA`) |
//! | `media:0xb5` | a raw consumer-control usage |
//! | `mouse:left`, `mouse:right`, `mouse:middle`, `mouse:back`, `mouse:forward`, `mouse:6` ... `mouse:16` | a mouse button |
//! | `scroll:up`, `scroll:down:3`, `scroll:left`, `scroll:right:2` | scroll by that many notches (default 1) |
//! | `#rrggbb`, `off`, `_` | colors |
//!
//! Macro steps: `down:<action>`, `up:<action>`, `tap:<action>` (or just
//! `<action>`), `delay:<ms>`, `text:<text>` (taken literally).
//!
//! Parsing is case-insensitive. Formatting produces the canonical names.

use alloc::string::String;
use core::fmt::{self, Write};

use crate::config::{
    Action, EncoderBinding, KeyBinding, KeyCode, LayerId, MacroId, Modifiers, MouseButton, ScrollDir, Step,
    SystemAction, Toggle,
};

const SYSTEM: &[(&str, SystemAction)] = &[
    ("settings", SystemAction::Settings),
    ("profile:next", SystemAction::NextProfile),
    ("profile:prev", SystemAction::PrevProfile),
    ("bootloader", SystemAction::Bootloader),
];
use crate::led::{Color, Rgb};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseError {
    UnknownKey,
    UnknownModifier,
    UnknownLayer,
    UnknownMacro,
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
            ParseError::UnknownMacro => "unknown macro",
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
    ("kpdiv", 0x54),
    ("kpmul", 0x55),
    ("kpminus", 0x56),
    ("kpplus", 0x57),
    ("kpenter", 0x58),
    ("kp1", 0x59),
    ("kp2", 0x5A),
    ("kp3", 0x5B),
    ("kp4", 0x5C),
    ("kp5", 0x5D),
    ("kp6", 0x5E),
    ("kp7", 0x5F),
    ("kp8", 0x60),
    ("kp9", 0x61),
    ("kp0", 0x62),
    ("kpdot", 0x63),
    ("kpequal", 0x67),
    ("kpcomma", 0x85),
    ("nonushash", 0x32),
    ("nonusbackslash", 0x64),
    ("102nd", 0x64),
    ("menu", 0x65),
    ("app", 0x65),
    ("execute", 0x74),
    ("help", 0x75),
    ("select", 0x77),
    ("again", 0x79),
    ("undo", 0x7A),
    ("cut", 0x7B),
    ("copy", 0x7C),
    ("paste", 0x7D),
    ("find", 0x7E),
    ("ro", 0x87),
    ("katakanahiragana", 0x88),
    ("yen", 0x89),
    ("henkan", 0x8A),
    ("muhenkan", 0x8B),
    ("hangul", 0x90),
    ("lang1", 0x90),
    ("hanja", 0x91),
    ("lang2", 0x91),
];

const MEDIA: &[(&str, u16)] = &[
    ("vol+", 0xE9),
    ("volup", 0xE9),
    ("vol-", 0xEA),
    ("voldown", 0xEA),
    ("mute", 0xE2),
    ("playpause", 0xCD),
    ("play", 0xCD),
    ("next", 0xB5),
    ("prev", 0xB6),
    ("stop", 0xB7),
    ("bri+", 0x6F),
    ("bri-", 0x70),
    ("rewind", 0xB4),
    ("fastforward", 0xB3),
    ("record", 0xB2),
    ("eject", 0xB8),
    ("calc", 0x192),
    ("browser", 0x196),
    ("email", 0x18A),
    ("files", 0x194),
    ("search", 0x221),
    ("browser_home", 0x223),
    ("browser_back", 0x224),
    ("browser_forward", 0x225),
    ("browser_refresh", 0x227),
];

/// Named mouse buttons (by index); higher buttons are written by number.
const MOUSE: &[&str] = &["left", "right", "middle", "back", "forward"];

const SCROLL: &[(&str, ScrollDir)] =
    &[("up", ScrollDir::Up), ("down", ScrollDir::Down), ("left", ScrollDir::Left), ("right", ScrollDir::Right)];

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
    if let Some(b) = s.strip_prefix("mouse:") {
        let b = b.trim();
        if let Some(i) = MOUSE.iter().position(|&name| name == b) {
            return Ok(Action::Mouse(MouseButton(i as u8)));
        }
        return match b.parse::<u8>() {
            Ok(n @ 1..=MouseButton::COUNT) => Ok(Action::Mouse(MouseButton(n - 1))),
            Ok(_) => Err(ParseError::BadNumber),
            Err(_) => Err(ParseError::UnknownKey),
        };
    }
    if let Some(rest) = s.strip_prefix("scroll:") {
        let (dir, steps) = match rest.split_once(':') {
            Some((d, n)) => (d.trim(), n.trim().parse::<u8>().ok().filter(|&n| n > 0).ok_or(ParseError::BadNumber)?),
            None => (rest.trim(), 1),
        };
        let &(_, dir) = SCROLL.iter().find(|(name, _)| *name == dir).ok_or(ParseError::UnknownKey)?;
        return Ok(Action::Scroll { dir, steps });
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
    // A modifier on its own ("shift", "ctrl+alt") is modifier bits without a key.
    if let Some(&(_, bit)) = MODS.iter().find(|(name, _)| *name == key_part) {
        return Ok(Action::Key { code: KeyCode(0), mods: Modifiers(mods | bit) });
    }
    Ok(Action::Key { code: parse_key(key_part)?, mods: Modifiers(mods) })
}

pub const DEFAULT_REPEAT_MS: u16 = 100;

/// Parses a key binding. `layer` and `macro_` map ids to indices.
pub fn parse_binding(
    text: &str,
    layer: impl Fn(&str) -> Option<LayerId>,
    macro_: impl Fn(&str) -> Option<MacroId>,
) -> Result<KeyBinding, ParseError> {
    let t = text.trim();
    let l = lower(t);
    match l.as_str() {
        "_" => return Ok(KeyBinding::Inherit),
        "" | "none" => return Ok(KeyBinding::None),
        _ => {}
    }
    if let Some(&(_, action)) = SYSTEM.iter().find(|(name, _)| *name == l) {
        return Ok(KeyBinding::System(action));
    }
    if let Some(n) = l.strip_prefix("profile:") {
        return match n.trim().parse::<u16>() {
            Ok(n @ 1..=255) => Ok(KeyBinding::System(SystemAction::Profile((n - 1) as u8))),
            _ => Err(ParseError::BadNumber),
        };
    }
    if let Some(id) = t.strip_prefix("layer:") {
        return layer(id.trim()).map(KeyBinding::Layer).ok_or(ParseError::UnknownLayer);
    }
    if let Some(id) = t.strip_prefix("toggle:") {
        return layer(id.trim()).map(|l| KeyBinding::Toggle(Toggle::Layer(l))).ok_or(ParseError::UnknownLayer);
    }
    if let Some(id) = t.strip_prefix("macro:") {
        return macro_(id.trim()).map(KeyBinding::Macro).ok_or(ParseError::UnknownMacro);
    }
    if let Some(a) = t.strip_prefix("latch:") {
        return parse_action(a).map(|a| KeyBinding::Toggle(Toggle::Latch(a)));
    }
    if let Some(rest) = t.strip_prefix("repeat:") {
        let (a, interval_ms) = match rest.rsplit_once('@') {
            Some((a, ms)) => (a, parse_ms(ms)?),
            None => (rest, DEFAULT_REPEAT_MS),
        };
        if interval_ms < 2 {
            return Err(ParseError::BadNumber);
        }
        return parse_action(a).map(|action| KeyBinding::Toggle(Toggle::Repeat { action, interval_ms }));
    }
    parse_action(t).map(KeyBinding::Action)
}

/// Milliseconds, with an optional `ms` suffix.
fn parse_ms(text: &str) -> Result<u16, ParseError> {
    let t = lower(text);
    t.strip_suffix("ms").unwrap_or(&t).trim().parse().map_err(|_| ParseError::BadNumber)
}

pub fn parse_step(text: &str) -> Result<Step, ParseError> {
    if let Some(literal) = text.strip_prefix("text:") {
        return Ok(Step::Text(literal.into()));
    }
    let t = text.trim();
    if let Some(a) = t.strip_prefix("down:") {
        return parse_action(a).map(Step::Down);
    }
    if let Some(a) = t.strip_prefix("up:") {
        return parse_action(a).map(Step::Up);
    }
    if let Some(a) = t.strip_prefix("tap:") {
        return parse_action(a).map(Step::Tap);
    }
    if let Some(ms) = t.strip_prefix("delay:") {
        return parse_ms(ms).map(|ms| Step::Delay { ms });
    }
    parse_action(t).map(Step::Tap)
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
        Action::Mouse(MouseButton(b)) => match MOUSE.get(b as usize) {
            Some(name) => write!(w, "mouse:{name}"),
            None => write!(w, "mouse:{}", b as u16 + 1),
        },
        Action::Scroll { dir, steps } => {
            let (name, _) = SCROLL.iter().find(|&&(_, d)| d == dir).unwrap();
            match steps {
                1 => write!(w, "scroll:{name}"),
                n => write!(w, "scroll:{name}:{n}"),
            }
        }
        Action::Key { code, mods } => {
            let mut first = true;
            for bit in (0..8).map(|i| 1u8 << i) {
                if mods.0 & bit != 0 {
                    let (name, _) = MODS.iter().find(|&&(_, b)| b == bit).unwrap();
                    if code.0 == 0 {
                        // Modifier-only: "ctrl+shift" without a trailing "+".
                        if !first {
                            w.write_char('+')?;
                        }
                        w.write_str(name)?;
                    } else {
                        write!(w, "{name}+")?;
                    }
                    first = false;
                }
            }
            let c = code.0;
            match c {
                0 if mods.0 != 0 => Ok(()),
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

/// Formats a binding; `layer` and `macro_` write ids for references.
pub fn write_binding<W: Write>(
    w: &mut W,
    binding: KeyBinding,
    layer: impl Fn(&mut W, LayerId) -> fmt::Result,
    macro_: impl Fn(&mut W, MacroId) -> fmt::Result,
) -> fmt::Result {
    match binding {
        KeyBinding::Inherit => w.write_char('_'),
        KeyBinding::None => w.write_str("none"),
        KeyBinding::Layer(id) => {
            w.write_str("layer:")?;
            layer(w, id)
        }
        KeyBinding::Toggle(Toggle::Layer(id)) => {
            w.write_str("toggle:")?;
            layer(w, id)
        }
        KeyBinding::Toggle(Toggle::Latch(a)) => {
            w.write_str("latch:")?;
            write_action(w, a)
        }
        KeyBinding::Toggle(Toggle::Repeat { action, interval_ms }) => {
            w.write_str("repeat:")?;
            write_action(w, action)?;
            write!(w, "@{interval_ms}")
        }
        KeyBinding::Macro(id) => {
            w.write_str("macro:")?;
            macro_(w, id)
        }
        KeyBinding::System(SystemAction::Profile(i)) => write!(w, "profile:{}", i as u16 + 1),
        KeyBinding::System(action) => {
            let (name, _) = SYSTEM.iter().find(|(_, a)| *a == action).unwrap();
            w.write_str(name)
        }
        KeyBinding::Action(a) => write_action(w, a),
    }
}

pub fn write_step(w: &mut impl Write, step: &Step) -> fmt::Result {
    match step {
        Step::Down(a) => {
            w.write_str("down:")?;
            write_action(w, *a)
        }
        Step::Up(a) => {
            w.write_str("up:")?;
            write_action(w, *a)
        }
        // A bare action is a tap, as it's usually written by hand.
        Step::Tap(a) => write_action(w, *a),
        Step::Delay { ms } => write!(w, "delay:{ms}"),
        Step::Text(t) => write!(w, "text:{t}"),
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

    fn macros(id: &str) -> Option<MacroId> {
        (id == "hello").then_some(MacroId(0))
    }

    fn binding(text: &str) -> Result<KeyBinding, ParseError> {
        parse_binding(text, layers, macros)
    }

    fn key(code: u8, mods: u8) -> Action {
        Action::Key { code: KeyCode(code), mods: Modifiers(mods) }
    }

    #[test]
    fn parses_bindings() {
        assert_eq!(binding("_"), Ok(KeyBinding::Inherit));
        assert_eq!(binding(""), Ok(KeyBinding::None));
        assert_eq!(binding("layer:nav"), Ok(KeyBinding::Layer(LayerId(1))));
        assert_eq!(binding("layer:nope"), Err(ParseError::UnknownLayer));
        assert_eq!(binding("A"), Ok(KeyBinding::Action(key(0x04, 0))));
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
    fn parses_mouse() {
        assert_eq!(parse_action("Mouse:Middle"), Ok(Action::Mouse(MouseButton::MIDDLE)));
        assert_eq!(parse_action("mouse:2"), Ok(Action::Mouse(MouseButton::RIGHT)));
        assert_eq!(parse_action("mouse:17"), Err(ParseError::BadNumber));
        assert_eq!(parse_action("mouse:0"), Err(ParseError::BadNumber));
        assert_eq!(parse_action("mouse:wheel"), Err(ParseError::UnknownKey));
        assert_eq!(parse_action("scroll:down:5"), Ok(Action::Scroll { dir: ScrollDir::Down, steps: 5 }));
        assert_eq!(parse_action("scroll:down:0"), Err(ParseError::BadNumber));
        assert_eq!(parse_action("scroll:sideways"), Err(ParseError::UnknownKey));
        assert_eq!(parse_step("down:mouse:left"), Ok(Step::Down(Action::Mouse(MouseButton::LEFT))));
        assert_eq!(
            parse_encoder("scroll:up:3"),
            Ok(EncoderBinding::Action(Action::Scroll { dir: ScrollDir::Up, steps: 3 }))
        );
        let mut out = String::new();
        write_action(&mut out, Action::Mouse(MouseButton(1))).unwrap();
        assert_eq!(out, "mouse:right");
    }

    #[test]
    fn parses_toggles_and_macros() {
        let a = key(0x04, 0);
        assert_eq!(binding("toggle:nav"), Ok(KeyBinding::Toggle(Toggle::Layer(LayerId(1)))));
        assert_eq!(binding("latch:a"), Ok(KeyBinding::Toggle(Toggle::Latch(a))));
        assert_eq!(binding("repeat:a"), Ok(KeyBinding::Toggle(Toggle::Repeat { action: a, interval_ms: 100 })));
        assert_eq!(binding("repeat:a@25ms"), Ok(KeyBinding::Toggle(Toggle::Repeat { action: a, interval_ms: 25 })));
        assert_eq!(binding("repeat:a@1"), Err(ParseError::BadNumber));
        assert_eq!(binding("macro:hello"), Ok(KeyBinding::Macro(MacroId(0))));
        assert_eq!(binding("macro:nope"), Err(ParseError::UnknownMacro));
        assert_eq!(binding("Settings"), Ok(KeyBinding::System(SystemAction::Settings)));
        assert_eq!(binding("profile:next"), Ok(KeyBinding::System(SystemAction::NextProfile)));
        assert_eq!(binding("profile:1"), Ok(KeyBinding::System(SystemAction::Profile(0))));
        assert_eq!(binding("Profile:12"), Ok(KeyBinding::System(SystemAction::Profile(11))));
        assert_eq!(binding("profile:0"), Err(ParseError::BadNumber));
        assert_eq!(binding("profile:256"), Err(ParseError::BadNumber));
        let mut out = String::new();
        write_binding(&mut out, KeyBinding::System(SystemAction::Profile(2)), |_, _| Ok(()), |_, _| Ok(())).unwrap();
        assert_eq!(out, "profile:3");
    }

    #[test]
    fn parses_steps() {
        assert_eq!(parse_step("down:shift"), Ok(Step::Down(key(0x00, 0x02))));
        assert_eq!(parse_action("ctrl+shift"), Ok(key(0x00, 0x03)));
        assert_eq!(parse_step("delay:250"), Ok(Step::Delay { ms: 250 }));
        assert_eq!(parse_step("enter"), Ok(Step::Tap(key(0x28, 0))));
        assert_eq!(parse_step("text: Hi there "), Ok(Step::Text(" Hi there ".into())));
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
        for text in ["a", "0", "f24", "ctrl+shift+t", "rgui+enter", "-", "0x66", "vol+", "playpause", "media:0x183", "kpplus", "ctrl+kpminus", "nonusbackslash", "browser_back", "shift", "ctrl+alt", "mouse:left", "mouse:forward", "mouse:6", "mouse:16", "scroll:up", "scroll:right:3"] {
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
