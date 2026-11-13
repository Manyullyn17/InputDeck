//! Colors and profile settings of a [`ConfigDocument`]: per-key colors,
//! layer and global palettes, LED/idle/timing settings and the name.

use anyhow::{Result, anyhow, bail, ensure};
use macropad_engine::led::{Color, HeldStyle, LedSettings, Palette};
use macropad_engine::notation::{parse_color, write_color};
use macropad_engine::{Config, IdleSettings, KEY_COUNT, LayerId, MacroTiming};
use serde::{Deserialize, Serialize};
use toml_edit::{DocumentMut, InlineTable, Item, Table, TableLike, Value};

use super::ConfigDocument;
use crate::toml_config::{self, BASE};

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
    pub defaults: SettingsDefaults,
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
