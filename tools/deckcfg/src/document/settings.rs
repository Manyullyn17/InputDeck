//! Colors and profile settings of a [`ConfigDocument`]: per-key colors,
//! layer and global palettes, LED/idle/timing settings and the name.

use anyhow::{Context, Result, anyhow, bail, ensure};
use inputdeck_engine::led::{Color, HeldStyle, LedSettings, Palette};
use inputdeck_engine::notation::{parse_action, parse_color, write_color};
use inputdeck_engine::{Config, IdleSettings, KEY_COUNT, LayerId, MacroTiming};
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

use toml_edit::{Array, DocumentMut, InlineTable, Item, Table, TableLike, Value};

use super::ConfigDocument;
use crate::layouts;
use crate::toml_config::{self, BASE, DEFAULT_LAYOUT};

/// Palette categories as named in the file, in display order.
pub const CATEGORIES: [&str; 6] = ["layer_key", "key", "modified", "media", "macro", "unbound"];
/// Per-key color slots.
pub const SLOTS: [&str; 3] = ["idle", "held", "active"];

#[derive(Serialize, Debug, Clone)]
pub struct PaletteEntry {
    pub category: &'static str,
    /// As written in the file; `None` = not set here.
    pub raw: Option<String>,
    /// What applies when not set here ("#rrggbb" or "off"; empty if the
    /// document doesn't compile).
    pub inherited: String,
}

#[derive(Serialize, Debug, Clone, Default)]
pub struct KeyColors {
    pub idle: Option<String>,
    pub held: Option<String>,
    pub active: Option<String>,
}

/// Profile-wide settings as written in the file (`None` = default), with
/// the defaults for display.
#[derive(Serialize, Debug, Clone)]
pub struct SettingsView {
    pub name: String,
    pub leds_enabled: Option<bool>,
    pub brightness: Option<i64>,
    pub idle_level: Option<i64>,
    pub held_style: Option<String>,
    pub dim_after: Option<i64>,
    pub off_after: Option<i64>,
    pub tap: Option<i64>,
    pub char_delay: Option<i64>,
    pub palette: Vec<PaletteEntry>,
    pub layout: LayoutView,
    pub indicators: Vec<IndicatorView>,
    /// Global actions as written: keys 1-12, then the knob ("" = none).
    pub global: Vec<String>,
    pub defaults: SettingsDefaults,
}

#[derive(Serialize, Debug, Clone)]
pub struct LayoutView {
    /// As written (`None` = the default, "us").
    pub base: Option<String>,
    /// What `system` resolves to on this computer, or why it can't.
    pub detected: Result<String, String>,
    /// Per-character overrides, in file order.
    pub chars: Vec<CharOverride>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct CharOverride {
    pub ch: String,
    /// One key, or a sequence (dead key, then letter).
    pub keys: Vec<String>,
}

#[derive(Serialize, Debug, Clone)]
pub struct IndicatorView {
    /// "caps_lock", "num_lock" or "scroll_lock".
    pub led: &'static str,
    /// Key number 1-12 (`None` = no indicator).
    pub key: Option<u8>,
    pub color: Option<String>,
}

/// Lock indicators as named in the file.
pub const INDICATORS: [&str; 3] = ["caps_lock", "num_lock", "scroll_lock"];

/// The detected system layout, looked up once per run.
fn detected_layout() -> Result<String, String> {
    static DETECTED: OnceLock<Result<String, String>> = OnceLock::new();
    DETECTED.get_or_init(|| layouts::detect_system().map_err(|e| format!("{e:#}"))).clone()
}

/// The `[global]` key for button index 0-12.
fn global_key(index: usize) -> Result<String> {
    match index {
        0..KEY_COUNT => Ok((index + 1).to_string()),
        KEY_COUNT => Ok("encoder".to_string()),
        _ => bail!("button index {index} out of range"),
    }
}

/// Replaces `layout = "x"` by a `[layout]` table with `base = "x"`.
fn layout_as_table(doc: &mut DocumentMut) -> &mut Table {
    let base = doc.get("layout").and_then(|v| v.as_str()).map(String::from);
    if !doc.get("layout").is_some_and(Item::is_table) {
        let mut t = Table::new();
        if let Some(b) = base {
            t.insert("base", toml_edit::value(b));
        }
        doc.insert("layout", Item::Table(t));
    }
    doc["layout"].as_table_mut().expect("just made a table")
}

/// Turns a `[layout]` table without overrides back into `layout = "x"`
/// (or removes it when nothing is left).
fn tidy_layout(doc: &mut DocumentMut) {
    let Some(t) = doc.get("layout").and_then(Item::as_table) else { return };
    let has_chars = t.get("chars").and_then(Item::as_table_like).is_some_and(|c| !c.is_empty());
    if has_chars {
        return;
    }
    match t.get("base").and_then(|v| v.as_str()).map(String::from) {
        Some(base) => {
            doc.insert("layout", toml_edit::value(base));
        }
        None => {
            doc.remove("layout");
        }
    }
}

#[derive(Serialize, Debug, Clone)]
pub struct SettingsDefaults {
    pub brightness: u8,
    pub idle_level: u8,
    pub held_style: &'static str,
    pub dim_after: u16,
    pub off_after: u16,
    pub tap: u16,
    pub char_delay: u16,
    pub layout: &'static str,
}

/// A setting value from the app.
#[derive(Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum SettingValue {
    Bool(bool),
    Int(i64),
    Text(String),
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Bool,
    Int,
    Text,
}

/// Settings the app may change: (path, type).
const SETTINGS: &[(&str, Kind)] = &[
    ("name", Kind::Text),
    ("leds.enabled", Kind::Bool),
    ("leds.brightness", Kind::Int),
    ("leds.idle_level", Kind::Int),
    ("leds.held_style", Kind::Text),
    ("idle.dim_after", Kind::Int),
    ("idle.off_after", Kind::Int),
    ("timing.tap", Kind::Int),
    ("timing.char_delay", Kind::Int),
];

fn percent(level: u8) -> u8 {
    (level as u16 * 100).div_ceil(255) as u8
}

fn color_text(c: Color) -> String {
    let mut s = String::new();
    let _ = write_color(&mut s, c);
    s
}

fn palette_get(p: &Palette, category: &str) -> Color {
    match category {
        "layer_key" => p.layer_key,
        "key" => p.key,
        "modified" => p.modified,
        "media" => p.media,
        "macro" => p.macro_key,
        _ => p.unbound,
    }
}

/// The palette color that applies on `layer` (walking up the parents,
/// then the global palette).
fn effective(config: &Config, layer: Option<LayerId>, category: &str) -> Color {
    let mut id = layer;
    for _ in 0..config.layers.len() {
        let Some(l) = id.and_then(|i| config.layer(i)) else { break };
        let c = palette_get(&l.palette, category);
        if c != Color::Inherit {
            return c;
        }
        id = l.parent;
    }
    palette_get(&config.leds.palette, category)
}

/// A color from the app: `None`, empty or "_" clear it.
fn checked_color(color: Option<&str>) -> Result<Option<Value>> {
    match color.map(str::trim) {
        None | Some("") | Some("_") => Ok(None),
        Some(t) => {
            let c = parse_color(t).map_err(|e| anyhow!("'{t}': {e}"))?;
            Ok(Some(Value::from(color_text(c))))
        }
    }
}

/// Sets (or with `None`, removes) `path` below `table`, creating missing
/// levels: regular tables at the document root, inline tables below.
/// Levels left empty by a removal are removed too.
fn set_path(table: &mut dyn TableLike, path: &[&str], value: Option<Value>, at_root: bool) {
    let (first, rest) = path.split_first().expect("non-empty path");
    if rest.is_empty() {
        match value {
            None => {
                table.remove(first);
            }
            Some(mut v) => {
                // Keep a trailing comment on the line, if there was one.
                if let Some(old) = table.get(first).and_then(Item::as_value) {
                    *v.decor_mut() = old.decor().clone();
                }
                table.insert(first, Item::Value(v));
            }
        }
        return;
    }
    if value.is_none() && table.get(first).is_none() {
        return;
    }
    let child = table.entry(first).or_insert(if at_root {
        Item::Table(Table::new())
    } else {
        Item::Value(Value::InlineTable(InlineTable::new()))
    });
    let empty = match child.as_table_like_mut() {
        Some(t) => {
            set_path(t, rest, value, false);
            t.is_empty()
        }
        None => false,
    };
    if empty {
        table.remove(first);
    }
}

fn get_path<'a>(item: &'a Item, path: &[&str]) -> Option<&'a Item> {
    path.iter().try_fold(item, |item, key| item.get(key))
}

impl ConfigDocument {
    /// Applies `f` to a copy of the document and keeps it, unless it breaks
    /// a document that compiled before (then nothing changes).
    fn try_edit(&mut self, f: impl FnOnce(&mut DocumentMut) -> Result<()>) -> Result<()> {
        let mut doc = self.doc.clone();
        f(&mut doc)?;
        let compiled = toml_config::parse(&doc.to_string()).map_err(|e| format!("{e:#}"));
        if let (Err(e), Ok(_)) = (&compiled, &self.compiled) {
            bail!("{e}");
        }
        self.doc = doc;
        self.compiled = compiled;
        Ok(())
    }

    fn layer_table_mut<'a>(doc: &'a mut DocumentMut, layer: &str) -> Result<&'a mut Table> {
        doc.get_mut("layers")
            .and_then(Item::as_table_mut)
            .and_then(|t| t.get_mut(layer))
            .and_then(Item::as_table_mut)
            .ok_or_else(|| anyhow!("no layer '{layer}'"))
    }

    /// Sets a key's color for a slot ("idle", "held", "active"); `None`
    /// goes back to the palette / automatic color.
    pub fn set_key_color(&mut self, layer: &str, index: usize, slot: &str, color: Option<&str>) -> Result<()> {
        ensure!(index < KEY_COUNT, "key index {index} out of range");
        ensure!(SLOTS.contains(&slot), "unknown color slot '{slot}'");
        let value = checked_color(color)?;
        let key = (index + 1).to_string();
        self.try_edit(|doc| {
            set_path(Self::layer_table_mut(doc, layer)?, &["colors", &key, slot], value, false);
            Ok(())
        })
    }

    /// Sets a palette color: of a layer, or the global one (`layer: None`).
    /// `None` inherits (layers) or restores the default (global).
    pub fn set_palette(&mut self, layer: Option<&str>, category: &str, color: Option<&str>) -> Result<()> {
        ensure!(CATEGORIES.contains(&category), "unknown color category '{category}'");
        let value = checked_color(color)?;
        self.try_edit(|doc| {
            match layer {
                Some(l) => set_path(Self::layer_table_mut(doc, l)?, &["palette", category], value, false),
                None => set_path(doc.as_table_mut(), &["leds", "palette", category], value, true),
            }
            Ok(())
        })
    }

    /// Sets a profile setting by path (e.g. "leds.brightness"); `None`
    /// removes it (the default applies).
    pub fn set_setting(&mut self, path: &str, value: Option<SettingValue>) -> Result<()> {
        let &(_, kind) = SETTINGS.iter().find(|(p, _)| *p == path).ok_or_else(|| anyhow!("unknown setting '{path}'"))?;
        let value = match value {
            None => None,
            Some(SettingValue::Bool(b)) if kind == Kind::Bool => Some(Value::from(b)),
            Some(SettingValue::Int(i)) if kind == Kind::Int => {
                ensure!(i >= 0, "{path} can't be negative");
                Some(Value::from(i))
            }
            Some(SettingValue::Text(t)) if kind == Kind::Text => {
                let t = t.trim();
                if t.is_empty() { None } else { Some(Value::from(t)) }
            }
            Some(_) => bail!("wrong type for {path}"),
        };
        let parts: Vec<&str> = path.split('.').collect();
        self.try_edit(|doc| {
            set_path(doc.as_table_mut(), &parts, value, true);
            Ok(())
        })
    }

    /// Sets the keyboard layout used for typing text (`None` = default).
    pub fn set_layout(&mut self, name: Option<&str>) -> Result<()> {
        let name = name.map(str::trim).filter(|n| !n.is_empty());
        if let Some(n) = name {
            layouts::load(n).with_context(|| format!("layout '{n}'"))?;
        }
        self.try_edit(|doc| {
            if doc.get("layout").is_some_and(Item::is_table) {
                let t = doc["layout"].as_table_mut().unwrap();
                match name {
                    Some(n) => t.insert("base", toml_edit::value(n)),
                    None => t.remove("base"),
                };
                tidy_layout(doc);
            } else {
                match name {
                    Some(n) => {
                        let mut v = Value::from(n);
                        if let Some(old) = doc.get("layout").and_then(Item::as_value) {
                            *v.decor_mut() = old.decor().clone();
                        }
                        doc.insert("layout", Item::Value(v));
                    }
                    None => {
                        doc.remove("layout");
                    }
                }
            }
            Ok(())
        })
    }

    /// Sets how one character is typed (`None` or no keys: use the layout).
    pub fn set_layout_char(&mut self, ch: &str, keys: Option<&[String]>) -> Result<()> {
        let mut chars = ch.chars();
        let (Some(c), None) = (chars.next(), chars.next()) else { bail!("'{ch}' must be a single character") };
        let keys: Vec<&str> = keys.unwrap_or_default().iter().map(|k| k.trim()).filter(|k| !k.is_empty()).collect();
        for k in &keys {
            toml_config::stroke(k)?;
        }
        let ch = c.to_string();
        self.try_edit(|doc| {
            if keys.is_empty() {
                if let Some(chars) = doc.get_mut("layout").and_then(|l| l.get_mut("chars")).and_then(Item::as_table_like_mut) {
                    chars.remove(&ch);
                }
            } else {
                let value = match keys.as_slice() {
                    [one] => Value::from(*one),
                    many => Value::Array(many.iter().copied().collect::<Array>()),
                };
                let layout = layout_as_table(doc);
                let chars = layout.entry("chars").or_insert(Item::Table(Table::new()));
                chars.as_table_like_mut().ok_or_else(|| anyhow!("layout.chars is not a table"))?.insert(&ch, Item::Value(value));
            }
            tidy_layout(doc);
            Ok(())
        })
    }

    /// Shows a host lock state on a key (`key: None` removes the indicator).
    pub fn set_indicator(&mut self, led: &str, key: Option<u8>, color: &str) -> Result<()> {
        ensure!(INDICATORS.contains(&led), "unknown indicator '{led}'");
        let value = match key {
            None => None,
            Some(k) => {
                ensure!((1..=KEY_COUNT as u8).contains(&k), "key must be 1-{KEY_COUNT}");
                let Ok(Color::Rgb(_)) = parse_color(color) else { bail!("indicator color must be #rrggbb") };
                let mut t = InlineTable::new();
                t.insert("key", Value::from(k as i64));
                t.insert("color", Value::from(color.to_ascii_lowercase()));
                Some(Value::InlineTable(t))
            }
        };
        self.try_edit(|doc| {
            set_path(doc.as_table_mut(), &["indicators", led], value, true);
            Ok(())
        })
    }

    /// Sets a global (layer-independent) action for button index 0-12.
    pub fn set_global(&mut self, index: usize, action: Option<&str>) -> Result<()> {
        let key = global_key(index)?;
        let action = action.map(str::trim).filter(|a| !a.is_empty() && *a != "_" && *a != "none");
        if let Some(a) = action {
            parse_action(a).map_err(|e| anyhow!("'{a}': {e} (global keys can only be keys and media keys)"))?;
        }
        self.try_edit(|doc| {
            set_path(doc.as_table_mut(), &["global", &key], action.map(Value::from), true);
            Ok(())
        })
    }

    fn layout_view(&self) -> LayoutView {
        let layout = self.doc.get("layout");
        let base = match layout {
            Some(l) if l.is_table() => l.get("base").and_then(|v| v.as_str()).map(String::from),
            Some(l) => l.as_str().map(String::from),
            None => None,
        };
        let chars = layout
            .and_then(|l| l.get("chars"))
            .and_then(Item::as_table_like)
            .map(|t| {
                t.iter()
                    .map(|(ch, v)| CharOverride {
                        ch: ch.to_string(),
                        keys: match v.as_array() {
                            Some(a) => a.iter().filter_map(|k| k.as_str().map(String::from)).collect(),
                            None => v.as_str().map(String::from).into_iter().collect(),
                        },
                    })
                    .collect()
            })
            .unwrap_or_default();
        LayoutView { base, detected: detected_layout(), chars }
    }

    pub(super) fn key_colors_raw(&self, layer: &str, index: usize) -> KeyColors {
        let key = (index + 1).to_string();
        let colors = get_path(self.doc.as_item(), &["layers", layer, "colors", &key]);
        let slot = |s: &str| colors.and_then(|c| c.get(s)).and_then(|v| v.as_str()).map(String::from);
        KeyColors { idle: slot("idle"), held: slot("held"), active: slot("active") }
    }

    /// A layer's own palette entries and what they inherit from its parents.
    pub(super) fn layer_palette(&self, layer: &str, index: usize) -> Vec<PaletteEntry> {
        let table = get_path(self.doc.as_item(), &["layers", layer, "palette"]);
        let config = self.compiled.as_ref().ok();
        CATEGORIES
            .iter()
            .map(|&category| {
                let raw = table.and_then(|t| t.get(category)).and_then(|v| v.as_str()).map(String::from);
                let inherited = config
                    .map(|c| {
                        // The parent's effective color (base: the global palette).
                        let parent =
                            if layer == BASE { None } else { c.layer(LayerId(index as u8)).and_then(|l| l.parent) };
                        color_text(effective(c, parent, category))
                    })
                    .unwrap_or_default();
                PaletteEntry { category, raw, inherited }
            })
            .collect()
    }

    pub(super) fn settings_view(&self) -> SettingsView {
        let root = self.doc.as_item();
        let int = |p: &[&str]| get_path(root, p).and_then(|v| v.as_integer());
        let palette = get_path(root, &["leds", "palette"]);
        let d = LedSettings::DEFAULT;
        SettingsView {
            name: get_path(root, &["name"]).and_then(|v| v.as_str()).unwrap_or("").to_string(),
            leds_enabled: get_path(root, &["leds", "enabled"]).and_then(|v| v.as_bool()),
            brightness: int(&["leds", "brightness"]),
            idle_level: int(&["leds", "idle_level"]),
            held_style: get_path(root, &["leds", "held_style"]).and_then(|v| v.as_str()).map(String::from),
            dim_after: int(&["idle", "dim_after"]),
            off_after: int(&["idle", "off_after"]),
            tap: int(&["timing", "tap"]),
            char_delay: int(&["timing", "char_delay"]),
            palette: CATEGORIES
                .iter()
                .map(|&category| PaletteEntry {
                    category,
                    raw: palette.and_then(|p| p.get(category)).and_then(|v| v.as_str()).map(String::from),
                    inherited: color_text(palette_get(&Palette::DEFAULT, category)),
                })
                .collect(),
            layout: self.layout_view(),
            indicators: INDICATORS
                .iter()
                .map(|&led| {
                    let t = get_path(root, &["indicators", led]);
                    IndicatorView {
                        led,
                        key: t.and_then(|t| t.get("key")).and_then(|v| v.as_integer()).map(|k| k as u8),
                        color: t.and_then(|t| t.get("color")).and_then(|v| v.as_str()).map(String::from),
                    }
                })
                .collect(),
            global: (0..=KEY_COUNT)
                .map(|i| {
                    let key = global_key(i).unwrap();
                    get_path(root, &["global", &key]).and_then(|v| v.as_str()).unwrap_or("").to_string()
                })
                .collect(),
            defaults: SettingsDefaults {
                brightness: percent(d.brightness),
                idle_level: percent(d.idle_level),
                held_style: match d.held_style {
                    HeldStyle::Brighten => "brighten",
                    HeldStyle::White => "white",
                    HeldStyle::Invert => "invert",
                },
                dim_after: IdleSettings::DEFAULT.dim_after_s,
                off_after: IdleSettings::DEFAULT.off_after_s,
                tap: MacroTiming::DEFAULT.tap_ms,
                char_delay: MacroTiming::DEFAULT.char_delay_ms,
                layout: DEFAULT_LAYOUT,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEMO: &str = include_str!("../../../../configs/demo.toml");

    #[test]
    fn key_colors_are_set_and_cleared() {
        let mut d = ConfigDocument::parse(DEMO).unwrap();
        d.set_key_color("one", 2, "idle", Some("#123456")).unwrap();
        d.set_key_color("one", 2, "held", Some("OFF")).unwrap();
        let text = d.text();
        assert!(text.contains("colors = { 3 = { idle = \"#123456\", held = \"off\" } }"), "{text}");
        assert_eq!(d.view().layers[1].keys[2].color, "#123456");
        assert_eq!(d.view().layers[1].keys[2].colors.held.as_deref(), Some("off"));

        d.set_key_color("one", 2, "idle", None).unwrap();
        d.set_key_color("one", 2, "held", Some("_")).unwrap();
        assert!(!d.text().contains("3 = {"), "{}", d.text());
        assert_eq!(d.text().matches("colors = {").count(), DEMO.matches("colors = {").count());
        assert!(d.set_key_color("one", 2, "idle", Some("#12345")).is_err());
        assert!(d.set_key_color("one", 2, "glow", Some("#123456")).is_err());
        assert!(d.set_key_color("one", 12, "idle", Some("#123456")).is_err());
        assert_eq!(d.view().error, None);
    }

    #[test]
    fn palettes_inherit_through_parents() {
        let mut d = ConfigDocument::parse(DEMO).unwrap();
        // "three" falls back to "one", whose palette sets key = #00ffb4.
        let v = d.view();
        let three = v.layers.iter().find(|l| l.id == "three").unwrap();
        let key = three.palette.iter().find(|e| e.category == "key").unwrap();
        assert_eq!(key.raw, None);
        assert_eq!(key.inherited, "#00ffb4");
        let base = &v.layers[0];
        assert_eq!(base.palette.iter().find(|e| e.category == "media").unwrap().inherited, "#00ff46");

        d.set_palette(Some("three"), "key", Some("#010203")).unwrap();
        d.set_palette(None, "media", Some("#0000ff")).unwrap();
        let v = d.view();
        let three = v.layers.iter().find(|l| l.id == "three").unwrap();
        assert_eq!(three.palette.iter().find(|e| e.category == "key").unwrap().raw.as_deref(), Some("#010203"));
        assert_eq!(v.layers[0].palette.iter().find(|e| e.category == "media").unwrap().inherited, "#0000ff");
        assert!(d.text().contains("media = \"#0000ff\""));

        // Clearing the last entry of a layer palette removes the palette.
        d.set_palette(Some("three"), "key", None).unwrap();
        assert!(!d.text().contains("#010203"));
        assert!(d.set_palette(None, "sparkle", Some("#000000")).is_err());
    }

    #[test]
    fn settings_round_trip_and_validate() {
        let mut d = ConfigDocument::parse(DEMO).unwrap();
        assert_eq!(d.view().settings.brightness, Some(60));
        d.set_setting("leds.brightness", Some(SettingValue::Int(30))).unwrap();
        d.set_setting("idle.dim_after", Some(SettingValue::Int(5))).unwrap();
        d.set_setting("name", Some(SettingValue::Text("Work".into()))).unwrap();
        let text = d.text();
        assert!(text.contains("brightness = 30     # percent"), "{text}");
        let s = d.view().settings;
        assert_eq!((s.brightness, s.dim_after, s.name.as_str()), (Some(30), Some(5), "Work"));
        assert_eq!(d.compiled().unwrap().idle.dim_after_s, 5);

        // Out of range / wrong type / unknown: rejected, nothing changes.
        let before = d.text();
        assert!(d.set_setting("leds.brightness", Some(SettingValue::Int(150))).is_err());
        assert!(d.set_setting("leds.held_style", Some(SettingValue::Text("sparkle".into()))).is_err());
        assert!(d.set_setting("leds.enabled", Some(SettingValue::Int(1))).is_err());
        assert!(d.set_setting("layers", None).is_err());
        assert_eq!(d.text(), before);

        d.set_setting("leds.brightness", None).unwrap();
        assert_eq!(d.view().settings.brightness, None);
        assert_eq!(d.view().settings.defaults.brightness, 63);
    }

    #[test]
    fn layout_overrides_switch_between_string_and_table() {
        let mut d = ConfigDocument::parse(DEMO).unwrap();
        assert_eq!(d.view().settings.layout.base.as_deref(), Some("us"));
        d.set_layout_char("é", Some(&["0x34".into(), "e".into()])).unwrap();
        d.set_layout_char("ß", Some(&["ralt+s".into()])).unwrap();
        let text = d.text();
        assert!(text.contains("[layout]\nbase = \"us\"\n"), "{text}");
        assert!(text.contains("[layout.chars]\n\"é\" = [\"0x34\", \"e\"]\n\"ß\" = \"ralt+s\""), "{text}");
        let l = d.view().settings.layout;
        assert_eq!(l.chars[0], CharOverride { ch: "é".into(), keys: vec!["0x34".into(), "e".into()] });
        assert_eq!(d.view().error, None);

        d.set_layout(Some("de")).unwrap_or_else(|e| panic!("{e:#}"));
        assert_eq!(d.view().settings.layout.base.as_deref(), Some("de"));
        assert!(d.set_layout_char("ab", Some(&["a".into()])).is_err());
        assert!(d.set_layout_char("x", Some(&["vol+".into()])).is_err());
        assert!(d.set_layout(Some("klingon")).is_err());

        // Removing the overrides goes back to `layout = "de"`.
        d.set_layout_char("é", None).unwrap();
        d.set_layout_char("ß", Some(&[])).unwrap();
        assert!(d.text().contains("layout = \"de\""), "{}", d.text());
        assert!(!d.text().contains("[layout"));
        d.set_layout(None).unwrap();
        assert!(!d.text().contains("\nlayout ="), "{}", d.text());
        assert_eq!(d.view().error, None);
    }

    #[test]
    fn indicators_and_global_keys() {
        let mut d = ConfigDocument::parse(DEMO).unwrap();
        let s = d.view().settings;
        assert_eq!((s.indicators[0].key, s.indicators[0].color.as_deref()), (Some(12), Some("#ffffff")));
        assert_eq!(s.global.len(), 13);

        d.set_indicator("num_lock", Some(3), "#00FF00").unwrap();
        assert!(d.text().contains("num_lock = { key = 3, color = \"#00ff00\" }"), "{}", d.text());
        assert!(d.set_indicator("num_lock", Some(13), "#00ff00").is_err());
        assert!(d.set_indicator("num_lock", Some(3), "off").is_err());
        d.set_indicator("caps_lock", None, "").unwrap();
        assert!(!d.text().contains("caps_lock = {"));

        d.set_global(11, Some("mute")).unwrap();
        d.set_global(12, Some("playpause")).unwrap();
        assert!(d.set_global(0, Some("layer:one")).is_err());
        assert!(d.set_global(13, Some("a")).is_err());
        let s = d.view().settings;
        assert_eq!((s.global[11].as_str(), s.global[12].as_str()), ("mute", "playpause"));
        assert_eq!(d.view().error, None);
        d.set_global(11, None).unwrap();
        assert_eq!(d.view().settings.global[11], "");
    }

    #[test]
    fn settings_on_a_bare_file_create_sections() {
        let mut d = ConfigDocument::parse("[layers.base]\n").unwrap();
        d.set_setting("leds.brightness", Some(SettingValue::Int(20))).unwrap();
        d.set_palette(None, "key", Some("#ff0000")).unwrap();
        let text = d.text();
        assert!(text.contains("[leds]\nbrightness = 20\npalette = { key = \"#ff0000\" }"), "{text}");
        assert_eq!(d.view().error, None);
        d.set_setting("leds.brightness", None).unwrap();
        d.set_palette(None, "key", None).unwrap();
        assert!(!d.text().contains("[leds]"), "{}", d.text());
    }
}
