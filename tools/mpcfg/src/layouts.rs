//! Keyboard layouts for typing text: which keys produce which characters.
//!
//! Layouts are read in XKB symbols format, the format Linux uses for its
//! layout definitions (`/usr/share/X11/xkb/symbols/`). The built-in `us`
//! and `de` layouts are small XKB snippets embedded below, so every layout
//! goes through the same code.
//!
//! Each key lists up to four levels: plain, Shift, AltGr, AltGr+Shift.
//! Characters reachable directly are preferred; characters only reachable
//! via dead keys become sequences (dead key + space, or dead key + letter).

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use macropad_engine::{KeyCode, KeyStroke, Modifiers};

const XKB_DIR: &str = "/usr/share/X11/xkb/symbols";

const SHIFT: u8 = Modifiers::LSHIFT.0;
const ALTGR: u8 = Modifiers::RALT.0;
const LEVEL_MODS: [u8; 4] = [0, SHIFT, ALTGR, ALTGR | SHIFT];

/// Characters → key press sequences.
pub type CharTable = HashMap<char, Vec<KeyStroke>>;

/// Loads a layout by name: `us`, `de`, `xkb:<file>` / `xkb:<file>(<variant>)`,
/// or `system` (the active desktop layout).
pub fn load(name: &str) -> Result<CharTable> {
    match name {
        "us" => Ok(build(&parse_symbols(US, None, &mut |_| bail!("no includes in built-in layouts"))?)),
        "de" => Ok(build(&parse_symbols(DE, None, &mut |_| bail!("no includes in built-in layouts"))?)),
        "system" => {
            let detected = detect_system()?;
            load(&detected).with_context(|| format!("detected system layout '{detected}'"))
        }
        _ => match name.strip_prefix("xkb:") {
            Some(spec) => load_xkb(Path::new(XKB_DIR), spec),
            None => bail!("unknown layout '{name}' (expected us, de, system, or xkb:<name>)"),
        },
    }
}

/// The active layout as an `xkb:` name. Currently reads KDE's setting.
pub fn detect_system() -> Result<String> {
    let home = std::env::var_os("HOME").ok_or_else(|| anyhow!("HOME is not set"))?;
    let kxkbrc = PathBuf::from(home).join(".config/kxkbrc");
    let text = fs::read_to_string(&kxkbrc)
        .map_err(|_| anyhow!("can't detect the system layout (only KDE is supported); set it explicitly"))?;
    let value = |key: &str| {
        text.lines().find_map(|l| l.strip_prefix(key).map(|v| v.split(',').next().unwrap_or("").trim().to_string()))
    };
    let layout = value("LayoutList=").filter(|l| !l.is_empty()).ok_or_else(|| anyhow!("no LayoutList in {}", kxkbrc.display()))?;
    Ok(match value("VariantList=").filter(|v| !v.is_empty()) {
        Some(variant) => format!("xkb:{layout}({variant})"),
        None => format!("xkb:{layout}"),
    })
}

fn load_xkb(dir: &Path, spec: &str) -> Result<CharTable> {
    Ok(build(&load_keys(dir, spec, 0)?))
}

/// Keys of `file(section)`; includes are resolved in the same directory.
fn load_keys(dir: &Path, spec: &str, depth: usize) -> Result<Vec<KeyDef>> {
    if depth > 8 {
        bail!("include nesting too deep at '{spec}'");
    }
    let (file, section) = split_spec(spec);
    let path = dir.join(file);
    let text = fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    parse_symbols(&text, section, &mut |inc| load_keys(dir, inc, depth + 1))
}

/// `de(nodeadkeys)` → ("de", Some("nodeadkeys")).
fn split_spec(spec: &str) -> (&str, Option<&str>) {
    match spec.split_once('(') {
        Some((file, rest)) => (file.trim(), Some(rest.trim_end_matches(')').trim())),
        None => (spec.trim(), None),
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Sym {
    Char(char),
    Dead(char),
}

#[derive(Clone, Debug)]
struct KeyDef {
    code: KeyCode,
    levels: Vec<Option<Sym>>,
}

/// Parses one section (the named one, else the `default` one, else the
/// first) of an XKB symbols file. Later definitions of a key override
/// earlier ones, including those from includes.
fn parse_symbols(
    text: &str,
    section: Option<&str>,
    include: &mut dyn FnMut(&str) -> Result<Vec<KeyDef>>,
) -> Result<Vec<KeyDef>> {
    // Strip comments; `include` lines have no trailing ';', so add one to
    // split statements uniformly.
    let text: String = text
        .lines()
        .map(|l| {
            let l = l.split("//").next().unwrap_or("");
            if l.trim_start().starts_with("include") && !l.contains(';') { format!("{l};") } else { l.to_string() }
        })
        .collect::<Vec<_>>()
        .join("\n");
    let body = find_section(&text, section)?;
    let mut keys: Vec<KeyDef> = Vec::new();
    let add = |def: KeyDef, keys: &mut Vec<KeyDef>| match keys.iter_mut().find(|k| k.code == def.code) {
        Some(existing) => {
            for (i, level) in def.levels.into_iter().enumerate() {
                if level.is_some() {
                    if existing.levels.len() <= i {
                        existing.levels.resize(i + 1, None);
                    }
                    existing.levels[i] = level;
                }
            }
        }
        None => keys.push(def),
    };
    for stmt in body.split(';') {
        let stmt = stmt.trim();
        if let Some(rest) = stmt.strip_prefix("include") {
            let spec = rest.trim().trim_matches('"');
            // "level3(ralt_switch)" and similar only change modifier behavior.
            if !spec.starts_with("level3") && !spec.starts_with("level5") && !spec.is_empty() {
                for part in spec.split('+') {
                    for def in include(part)? {
                        add(def, &mut keys);
                    }
                }
            }
            continue;
        }
        let Some(rest) = stmt.strip_prefix("key") else { continue };
        let Some(name) = rest.trim_start().strip_prefix('<').and_then(|r| r.split('>').next()) else { continue };
        let Some(code) = key_code(name) else { continue };
        // The first [...] list in the key block holds the symbols of group 1.
        let Some(list) = rest.split('[').nth(1).and_then(|r| r.split(']').next()) else { continue };
        let levels = list.split(',').take(4).map(|s| keysym(s.trim())).collect();
        add(KeyDef { code, levels }, &mut keys);
    }
    Ok(keys)
}

fn find_section<'a>(text: &'a str, section: Option<&str>) -> Result<&'a str> {
    let mut first = None;
    let mut default = None;
    let mut rest = text;
    while let Some(pos) = rest.find("xkb_symbols") {
        let header_start = rest[..pos].rfind('\n').map_or(0, |n| n + 1);
        let header = &rest[header_start..pos];
        let after = &rest[pos..];
        let name = after.split('"').nth(1).unwrap_or("");
        let Some(open) = after.find('{') else { break };
        // Find the matching closing brace.
        let mut depth = 0;
        let mut end = None;
        for (i, ch) in after[open..].char_indices() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(open + i);
                        break;
                    }
                }
                _ => {}
            }
        }
        let Some(end) = end else { break };
        let body = &after[open + 1..end];
        if section == Some(name) {
            return Ok(body);
        }
        if first.is_none() {
            first = Some(body);
        }
        if default.is_none() && header.contains("default") {
            default = Some(body);
        }
        rest = &after[end..];
    }
    match section {
        Some(s) => bail!("layout variant '{s}' not found"),
        None => default.or(first).ok_or_else(|| anyhow!("no xkb_symbols section found")),
    }
}

/// XKB key names (main block, pc105) → HID usage.
fn key_code(name: &str) -> Option<KeyCode> {
    const ROW_D: [u8; 12] = [0x14, 0x1A, 0x08, 0x15, 0x17, 0x1C, 0x18, 0x0C, 0x12, 0x13, 0x2F, 0x30];
    const ROW_C: [u8; 12] = [0x04, 0x16, 0x07, 0x09, 0x0A, 0x0B, 0x0D, 0x0E, 0x0F, 0x33, 0x34, 0x32];
    const ROW_B: [u8; 10] = [0x1D, 0x1B, 0x06, 0x19, 0x05, 0x11, 0x10, 0x36, 0x37, 0x38];
    const ROW_E: [u8; 12] = [0x1E, 0x1F, 0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x2D, 0x2E];
    let code = match name {
        "TLDE" => 0x35,
        "BKSL" => 0x31,
        "LSGT" => 0x64,
        "SPCE" => 0x2C,
        _ => {
            let (row, n) = name.split_at(2);
            let i = n.parse::<usize>().ok()?.checked_sub(1)?;
            *match row {
                "AE" => ROW_E.get(i),
                "AD" => ROW_D.get(i),
                "AC" => ROW_C.get(i),
                "AB" => ROW_B.get(i),
                _ => None,
            }?
        }
    };
    Some(KeyCode(code))
}

/// Latin-1 keysym names, in code point order from U+00A0.
const LATIN1_HIGH: [&str; 96] = [
    "nobreakspace", "exclamdown", "cent", "sterling", "currency", "yen", "brokenbar", "section", "diaeresis",
    "copyright", "ordfeminine", "guillemotleft", "notsign", "hyphen", "registered", "macron", "degree",
    "plusminus", "twosuperior", "threesuperior", "acute", "mu", "paragraph", "periodcentered", "cedilla",
    "onesuperior", "masculine", "guillemotright", "onequarter", "onehalf", "threequarters", "questiondown",
    "Agrave", "Aacute", "Acircumflex", "Atilde", "Adiaeresis", "Aring", "AE", "Ccedilla", "Egrave", "Eacute",
    "Ecircumflex", "Ediaeresis", "Igrave", "Iacute", "Icircumflex", "Idiaeresis", "ETH", "Ntilde", "Ograve",
    "Oacute", "Ocircumflex", "Otilde", "Odiaeresis", "multiply", "Oslash", "Ugrave", "Uacute", "Ucircumflex",
    "Udiaeresis", "Yacute", "THORN", "ssharp", "agrave", "aacute", "acircumflex", "atilde", "adiaeresis",
    "aring", "ae", "ccedilla", "egrave", "eacute", "ecircumflex", "ediaeresis", "igrave", "iacute",
    "icircumflex", "idiaeresis", "eth", "ntilde", "ograve", "oacute", "ocircumflex", "otilde", "odiaeresis",
    "division", "oslash", "ugrave", "uacute", "ucircumflex", "udiaeresis", "yacute", "thorn", "ydiaeresis",
];

/// ASCII punctuation keysym names, with their characters.
const ASCII_NAMES: &[(&str, char)] = &[
    ("space", ' '), ("exclam", '!'), ("quotedbl", '"'), ("numbersign", '#'), ("dollar", '$'),
    ("percent", '%'), ("ampersand", '&'), ("apostrophe", '\''), ("parenleft", '('), ("parenright", ')'),
    ("asterisk", '*'), ("plus", '+'), ("comma", ','), ("minus", '-'), ("period", '.'), ("slash", '/'),
    ("colon", ':'), ("semicolon", ';'), ("less", '<'), ("equal", '='), ("greater", '>'), ("question", '?'),
    ("at", '@'), ("bracketleft", '['), ("backslash", '\\'), ("bracketright", ']'), ("asciicircum", '^'),
    ("underscore", '_'), ("grave", '`'), ("braceleft", '{'), ("bar", '|'), ("braceright", '}'),
    ("asciitilde", '~'),
];

const OTHER_NAMES: &[(&str, char)] = &[
    ("EuroSign", '€'), ("oe", 'œ'), ("OE", 'Œ'), ("emdash", '—'), ("endash", '–'), ("ellipsis", '…'),
    ("leftsinglequotemark", '‘'), ("rightsinglequotemark", '’'), ("singlelowquotemark", '‚'),
    ("leftdoublequotemark", '“'), ("rightdoublequotemark", '”'), ("doublelowquotemark", '„'),
    ("guillemetleft", '«'), ("guillemetright", '»'), ("ordmasculine", 'º'), ("Ooblique", 'Ø'), ("ooblique", 'ø'),
];

/// Dead keys and the spacing character they produce when followed by space.
const DEAD_KEYS: &[(&str, char)] = &[
    ("dead_grave", '`'), ("dead_acute", '´'), ("dead_circumflex", '^'), ("dead_tilde", '~'),
    ("dead_diaeresis", '¨'), ("dead_abovering", '˚'), ("dead_caron", 'ˇ'), ("dead_macron", '¯'),
    ("dead_cedilla", '¸'),
];

fn keysym(name: &str) -> Option<Sym> {
    if let Some(&(_, c)) = DEAD_KEYS.iter().find(|(n, _)| *n == name) {
        return Some(Sym::Dead(c));
    }
    let mut chars = name.chars();
    if let (Some(c), None) = (chars.next(), chars.next())
        && c.is_ascii_alphanumeric()
    {
        return Some(Sym::Char(c));
    }
    if let Some(hex) = name.strip_prefix('U').filter(|h| h.len() >= 4)
        && let Some(c) = u32::from_str_radix(hex, 16).ok().and_then(char::from_u32)
    {
        return Some(Sym::Char(c));
    }
    if let Some(i) = LATIN1_HIGH.iter().position(|&n| n == name) {
        return char::from_u32(0xA0 + i as u32).map(Sym::Char);
    }
    ASCII_NAMES.iter().chain(OTHER_NAMES).find(|(n, _)| *n == name).map(|&(_, c)| Sym::Char(c))
}

/// Dead key (by its spacing char) + base letter → composed character.
const COMPOSE: &[(char, &str, &str)] = &[
    ('´', "aeiouyAEIOUY", "áéíóúýÁÉÍÓÚÝ"),
    ('`', "aeiouAEIOU", "àèìòùÀÈÌÒÙ"),
    ('^', "aeiouAEIOU", "âêîôûÂÊÎÔÛ"),
    ('~', "anoANO", "ãñõÃÑÕ"),
    ('¨', "aeiouyAEIOU", "äëïöüÿÄËÏÖÜ"),
    ('˚', "aA", "åÅ"),
    ('¸', "cC", "çÇ"),
    ('ˇ', "cszCSZ", "čšžČŠŽ"),
];

fn build(keys: &[KeyDef]) -> CharTable {
    let mut table = CharTable::new();
    let stroke = |code: KeyCode, mods: u8| KeyStroke { code, mods: Modifiers(mods) };
    table.insert('\n', vec![stroke(KeyCode(0x28), 0)]);
    table.insert('\t', vec![stroke(KeyCode(0x2B), 0)]);
    table.insert(' ', vec![stroke(KeyCode(0x2C), 0)]);
    let mut dead: Vec<(char, KeyStroke)> = Vec::new();
    // Lower levels first, so e.g. 'a' is typed without modifiers.
    for (level, &mods) in LEVEL_MODS.iter().enumerate() {
        for key in keys {
            match key.levels.get(level) {
                Some(Some(Sym::Char(c))) => {
                    table.entry(*c).or_insert_with(|| vec![stroke(key.code, mods)]);
                }
                Some(Some(Sym::Dead(c))) => dead.push((*c, stroke(key.code, mods))),
                _ => {}
            }
        }
    }
    let space = table[&' '][0];
    for &(spacing, dead_key) in &dead {
        table.entry(spacing).or_insert_with(|| vec![dead_key, space]);
        if let Some(&(_, bases, results)) = COMPOSE.iter().find(|(d, _, _)| *d == spacing) {
            for (base, result) in bases.chars().zip(results.chars()) {
                if let Some(base_keys) = table.get(&base).filter(|k| k.len() == 1).cloned()
                    && !table.contains_key(&result)
                {
                    table.insert(result, [vec![dead_key], base_keys].concat());
                }
            }
        }
    }
    table
}

const US: &str = r#"
default xkb_symbols "basic" {
    key <TLDE> {[ grave, asciitilde ]};
    key <AE01> {[ 1, exclam ]};       key <AE02> {[ 2, at ]};
    key <AE03> {[ 3, numbersign ]};   key <AE04> {[ 4, dollar ]};
    key <AE05> {[ 5, percent ]};      key <AE06> {[ 6, asciicircum ]};
    key <AE07> {[ 7, ampersand ]};    key <AE08> {[ 8, asterisk ]};
    key <AE09> {[ 9, parenleft ]};    key <AE10> {[ 0, parenright ]};
    key <AE11> {[ minus, underscore ]}; key <AE12> {[ equal, plus ]};
    key <AD01> {[ q, Q ]}; key <AD02> {[ w, W ]}; key <AD03> {[ e, E ]}; key <AD04> {[ r, R ]};
    key <AD05> {[ t, T ]}; key <AD06> {[ y, Y ]}; key <AD07> {[ u, U ]}; key <AD08> {[ i, I ]};
    key <AD09> {[ o, O ]}; key <AD10> {[ p, P ]};
    key <AD11> {[ bracketleft, braceleft ]}; key <AD12> {[ bracketright, braceright ]};
    key <AC01> {[ a, A ]}; key <AC02> {[ s, S ]}; key <AC03> {[ d, D ]}; key <AC04> {[ f, F ]};
    key <AC05> {[ g, G ]}; key <AC06> {[ h, H ]}; key <AC07> {[ j, J ]}; key <AC08> {[ k, K ]};
    key <AC09> {[ l, L ]};
    key <AC10> {[ semicolon, colon ]}; key <AC11> {[ apostrophe, quotedbl ]};
    key <BKSL> {[ backslash, bar ]};
    key <AB01> {[ z, Z ]}; key <AB02> {[ x, X ]}; key <AB03> {[ c, C ]}; key <AB04> {[ v, V ]};
    key <AB05> {[ b, B ]}; key <AB06> {[ n, N ]}; key <AB07> {[ m, M ]};
    key <AB08> {[ comma, less ]}; key <AB09> {[ period, greater ]}; key <AB10> {[ slash, question ]};
};
"#;

const DE: &str = r#"
default xkb_symbols "basic" {
    key <TLDE> {[ dead_circumflex, degree ]};
    key <AE01> {[ 1, exclam, onesuperior ]};    key <AE02> {[ 2, quotedbl, twosuperior ]};
    key <AE03> {[ 3, section, threesuperior ]}; key <AE04> {[ 4, dollar ]};
    key <AE05> {[ 5, percent ]};                key <AE06> {[ 6, ampersand ]};
    key <AE07> {[ 7, slash, braceleft ]};       key <AE08> {[ 8, parenleft, bracketleft ]};
    key <AE09> {[ 9, parenright, bracketright ]}; key <AE10> {[ 0, equal, braceright ]};
    key <AE11> {[ ssharp, question, backslash ]}; key <AE12> {[ dead_acute, dead_grave ]};
    key <AD01> {[ q, Q, at ]}; key <AD02> {[ w, W ]}; key <AD03> {[ e, E, EuroSign ]}; key <AD04> {[ r, R ]};
    key <AD05> {[ t, T ]}; key <AD06> {[ z, Z ]}; key <AD07> {[ u, U ]}; key <AD08> {[ i, I ]};
    key <AD09> {[ o, O ]}; key <AD10> {[ p, P ]};
    key <AD11> {[ udiaeresis, Udiaeresis ]}; key <AD12> {[ plus, asterisk, asciitilde ]};
    key <AC01> {[ a, A ]}; key <AC02> {[ s, S ]}; key <AC03> {[ d, D ]}; key <AC04> {[ f, F ]};
    key <AC05> {[ g, G ]}; key <AC06> {[ h, H ]}; key <AC07> {[ j, J ]}; key <AC08> {[ k, K ]};
    key <AC09> {[ l, L ]};
    key <AC10> {[ odiaeresis, Odiaeresis ]}; key <AC11> {[ adiaeresis, Adiaeresis ]};
    key <BKSL> {[ numbersign, apostrophe ]};
    key <LSGT> {[ less, greater, bar ]};
    key <AB01> {[ y, Y ]}; key <AB02> {[ x, X ]}; key <AB03> {[ c, C ]}; key <AB04> {[ v, V ]};
    key <AB05> {[ b, B ]}; key <AB06> {[ n, N ]}; key <AB07> {[ m, M, mu ]};
    key <AB08> {[ comma, semicolon ]}; key <AB09> {[ period, colon ]}; key <AB10> {[ minus, underscore ]};
};
"#;

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(table: &CharTable, c: char) -> Vec<(u8, u8)> {
        table.get(&c).map(|k| k.iter().map(|s| (s.code.0, s.mods.0)).collect()).unwrap_or_default()
    }

    #[test]
    fn us_covers_printable_ascii() {
        let t = load("us").unwrap();
        for c in (0x20u8..0x7F).map(char::from) {
            assert!(t.contains_key(&c), "{c:?}");
        }
        assert_eq!(keys(&t, 'a'), [(0x04, 0)]);
        assert_eq!(keys(&t, 'Z'), [(0x1D, SHIFT)]);
        assert_eq!(keys(&t, '@'), [(0x1F, SHIFT)]);
    }

    #[test]
    fn german_direct_and_dead_keys() {
        let t = load("de").unwrap();
        assert_eq!(keys(&t, 'z'), [(0x1C, 0)]);
        assert_eq!(keys(&t, '@'), [(0x14, ALTGR)]);
        assert_eq!(keys(&t, 'ä'), [(0x34, 0)]);
        // ^ only exists as a dead key: dead_circumflex, then space.
        assert_eq!(keys(&t, '^'), [(0x35, 0), (0x2C, 0)]);
        // é: dead acute, then e.
        assert_eq!(keys(&t, 'é'), [(0x2E, 0), (0x08, 0)]);
        // È: dead grave (shift+´), then shift+e.
        assert_eq!(keys(&t, 'È'), [(0x2E, SHIFT), (0x08, SHIFT)]);
        for c in (0x20u8..0x7F).map(char::from) {
            assert!(t.contains_key(&c), "{c:?}");
        }
    }

    #[test]
    fn parses_sections_includes_and_overrides() {
        let base = r#"
            xkb_symbols "other" { key <AC01> {[ x ]}; };
            default xkb_symbols "basic" { key <AC01> {[ a, A ]}; key <AC02> {[ s, S ]}; };
        "#;
        let variant = r#"
            xkb_symbols "fancy" {
                include "base(basic)"
                include "level3(ralt_switch)"
                key <AC02> {[ s, S, ssharp ]}; // AltGr+s
            };
        "#;
        let mut include = |spec: &str| {
            assert_eq!(spec, "base(basic)");
            parse_symbols(base, Some("basic"), &mut |_| unreachable!())
        };
        let t = build(&parse_symbols(variant, Some("fancy"), &mut include).unwrap());
        assert_eq!(keys(&t, 'a'), [(0x04, 0)]);
        assert_eq!(keys(&t, 'ß'), [(0x16, ALTGR)]);
        assert!(!t.contains_key(&'x'));
    }

    #[test]
    fn eurkey_from_system_if_installed() {
        let path = Path::new(XKB_DIR).join("eu");
        if !path.exists() {
            return;
        }
        let t = load("xkb:eu").unwrap();
        assert_eq!(keys(&t, 'ä'), [(0x04, ALTGR)]);
        assert_eq!(keys(&t, 'Ä'), [(0x04, ALTGR | SHIFT)]);
        assert_eq!(keys(&t, 'ß'), [(0x16, ALTGR)]);
        assert_eq!(keys(&t, '€'), [(0x22, ALTGR)]);
        assert_eq!(keys(&t, 'a'), [(0x04, 0)]);
        assert_eq!(keys(&t, '^'), [(0x23, SHIFT)]); // direct on EurKEY, not dead
    }

    #[test]
    fn keysym_names() {
        assert_eq!(keysym("adiaeresis"), Some(Sym::Char('ä')));
        assert_eq!(keysym("EuroSign"), Some(Sym::Char('€')));
        assert_eq!(keysym("U2026"), Some(Sym::Char('…')));
        assert_eq!(keysym("dead_acute"), Some(Sym::Dead('´')));
        assert_eq!(keysym("7"), Some(Sym::Char('7')));
        assert_eq!(keysym("NoSymbol"), None);
    }
}
