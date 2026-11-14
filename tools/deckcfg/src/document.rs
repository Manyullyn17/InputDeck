//! A TOML config opened for editing (used by the desktop app).
//!
//! Edits are applied to the TOML document in place with `toml_edit`, so
//! comments and formatting survive. After every edit the document is
//! compiled again, giving an up-to-date [`Config`] or an error message.

use anyhow::{Context, Result, anyhow, bail, ensure};
use inputdeck_engine::led::{self, Category, Rgb};
use inputdeck_engine::notation::{parse_binding, parse_encoder, parse_step};
use inputdeck_engine::{Button, Config, KEY_COUNT, LayerId, MacroId, label};
use serde::{Deserialize, Serialize};
use toml_edit::{Array, DocumentMut, InlineTable, Item, Table, Value};

use crate::toml_config::{self, BASE, NO_PARENT};

mod settings;
pub use settings::{
    CATEGORIES, CharOverride, INDICATORS, IndicatorView, KeyColors, LayoutView, PaletteEntry, SLOTS, SettingValue, SettingsView,
};

/// Index of the encoder button in key lists (after the 12 keys).
pub const ENCODER_PRESS: usize = KEY_COUNT;

pub struct ConfigDocument {
    doc: DocumentMut,
    compiled: Result<Config, String>,
}

#[derive(Serialize, Debug, Clone)]
pub struct DocumentView {
    pub name: String,
    pub layers: Vec<LayerView>,
    pub macros: Vec<MacroView>,
    pub settings: SettingsView,
    /// Compile error of the current document, if any.
    pub error: Option<String>,
}

#[derive(Serialize, Debug, Clone)]
pub struct LayerView {
    pub id: String,
    pub name: String,
    pub parent: Option<String>,
    /// The 12 keys, then the encoder press.
    pub keys: Vec<KeyView>,
    pub encoder_cw: String,
    pub encoder_ccw: String,
    /// Category colors set on this layer, and what they inherit.
    pub palette: Vec<PaletteEntry>,
}

#[derive(Serialize, Debug, Clone)]
pub struct KeyView {
    /// The binding as written in the file ("_" when unset).
    pub raw: String,
    /// Short label of the effective binding (after inheritance).
    pub label: String,
    pub inherited: bool,
    pub category: &'static str,
    /// Idle LED color (full level), "#rrggbb".
    pub color: String,
    /// Per-key colors as written in the file.
    pub colors: KeyColors,
}

#[derive(Serialize, Debug, Clone)]
pub struct MacroView {
    pub id: String,
    #[serde(flatten)]
    pub settings: MacroEdit,
    /// Where the macro is bound, e.g. "Tools · key 4".
    pub used_by: Vec<String>,
}

/// A macro's editable fields, as written in the file.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct MacroEdit {
    pub name: String,
    /// "once", "while_held" or "toggle".
    pub kind: String,
    /// "finish" or "immediate" (toggle macros only).
    pub stop: String,
    /// Milliseconds between typed characters; `None` uses the global default.
    pub char_delay: Option<u16>,
    /// Steps in notation (`tap:a`, `down:shift`, `delay:50`, `text:hi`).
    pub steps: Vec<String>,
}

const MACRO_KINDS: [&str; 3] = ["once", "while_held", "toggle"];
const STOP_MODES: [&str; 2] = ["finish", "immediate"];

fn check_id(id: &str, what: &str) -> Result<()> {
    ensure!(
        !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'),
        "{what} ids may only contain letters, digits, '_' and '-'"
    );
    Ok(())
}

/// Steps as an array: on one line when short, otherwise one per line.
fn steps_array(steps: &[String]) -> Array {
    let mut arr: Array = steps.iter().map(String::as_str).collect();
    let one_line = steps.len() <= 4 && steps.iter().map(|s| s.len() + 4).sum::<usize>() <= 70;
    if !one_line {
        for v in arr.iter_mut() {
            v.decor_mut().set_prefix("\n  ");
        }
        arr.set_trailing("\n");
        arr.set_trailing_comma(true);
    }
    arr
}

pub fn hex(c: Rgb) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
}

fn category_name(c: Category) -> &'static str {
    match c {
        Category::LayerKey => "layer",
        Category::Key => "key",
        Category::Modified => "modified",
        Category::Media => "media",
        Category::Macro => "macro",
        Category::Unbound => "unbound",
    }
}

/// Twelve values laid out as a 4x3 grid in the file.
fn grid_array(values: &[String]) -> Array {
    let mut arr = Array::new();
    for (i, v) in values.iter().enumerate() {
        let mut value = Value::from(v.as_str());
        value.decor_mut().set_prefix(if i % 3 == 0 { "\n  " } else { " " });
        arr.push_formatted(value);
    }
    arr.set_trailing("\n");
    arr.set_trailing_comma(true);
    arr
}

impl ConfigDocument {
    pub fn parse(text: &str) -> Result<Self> {
        let doc: DocumentMut = text.parse().context("TOML syntax error")?;
        let mut d = ConfigDocument { doc, compiled: Err(String::new()) };
        d.recompile();
        Ok(d)
    }

    /// The document as TOML text.
    pub fn text(&self) -> String {
        self.doc.to_string()
    }

    pub fn compiled(&self) -> Result<&Config, &str> {
        self.compiled.as_ref().map_err(String::as_str)
    }

    fn recompile(&mut self) {
        self.compiled = toml_config::parse(&self.text()).map_err(|e| format!("{e:#}"));
    }

    fn layers_table(&self) -> Option<&Table> {
        self.doc.get("layers").and_then(Item::as_table)
    }

    /// Layer ids in compile order: base first, then file order.
    pub fn layer_ids(&self) -> Vec<String> {
        let mut ids = vec![BASE.to_string()];
        if let Some(t) = self.layers_table() {
            ids.extend(t.iter().map(|(k, _)| k.to_string()).filter(|k| k != BASE));
        }
        ids
    }

    pub fn macro_ids(&self) -> Vec<String> {
        self.doc
            .get("macros")
            .and_then(Item::as_table)
            .map(|t| t.iter().map(|(k, _)| k.to_string()).collect())
            .unwrap_or_default()
    }

    fn macros_mut(&mut self) -> Result<&mut Table> {
        let macros = self.doc.entry("macros").or_insert_with(|| {
            let mut t = Table::new();
            t.set_implicit(true);
            Item::Table(t)
        });
        macros.as_table_mut().ok_or_else(|| anyhow!("'macros' is not a table"))
    }

    /// Adds an empty one-shot macro.
    pub fn add_macro(&mut self, id: &str, name: &str) -> Result<()> {
        check_id(id, "macro")?;
        ensure!(!self.macro_ids().iter().any(|m| m == id), "macro '{id}' already exists");
        let mut t = Table::new();
        if !name.is_empty() {
            t.insert("name", toml_edit::value(name));
        }
        t.insert("steps", toml_edit::value(Array::new()));
        self.macros_mut()?.insert(id, Item::Table(t));
        self.recompile();
        Ok(())
    }

    /// Removes a macro. Keys still bound to it make the document invalid
    /// until they are changed, which the view reports.
    pub fn remove_macro(&mut self, id: &str) -> Result<()> {
        self.macros_mut()?.remove(id).ok_or_else(|| anyhow!("no macro '{id}'"))?;
        self.recompile();
        Ok(())
    }

    /// Replaces a macro's settings and steps. Invalid input is rejected and
    /// leaves the document unchanged.
    pub fn set_macro(&mut self, id: &str, edit: &MacroEdit) -> Result<()> {
        ensure!(MACRO_KINDS.contains(&edit.kind.as_str()), "unknown macro type '{}'", edit.kind);
        ensure!(STOP_MODES.contains(&edit.stop.as_str()), "unknown stop mode '{}'", edit.stop);
        for (i, step) in edit.steps.iter().enumerate() {
            parse_step(step).map_err(|e| anyhow!("step {} '{step}': {e}", i + 1))?;
        }
        let table = self
            .macros_mut()?
            .get_mut(id)
            .and_then(Item::as_table_mut)
            .ok_or_else(|| anyhow!("no macro '{id}'"))?;
        let mut set = |key: &str, value: Option<Value>| match value {
            None => {
                table.remove(key);
            }
            Some(mut v) => {
                // Keep a trailing comment on the line, if there was one.
                if let Some(old) = table.get(key).and_then(Item::as_value) {
                    *v.decor_mut() = old.decor().clone();
                }
                table.insert(key, Item::Value(v));
            }
        };
        set("name", (!edit.name.is_empty()).then(|| Value::from(edit.name.as_str())));
        set("type", (edit.kind != "once").then(|| Value::from(edit.kind.as_str())));
        set("stop", (edit.kind == "toggle" && edit.stop != "finish").then(|| Value::from(edit.stop.as_str())));
        set("char_delay", edit.char_delay.map(|ms| Value::from(ms as i64)));
        set("steps", Some(Value::Array(steps_array(&edit.steps))));
        self.recompile();
        Ok(())
    }

    fn macro_view(&self, id: String, layer_names: &[(String, String)]) -> MacroView {
        let t = &self.doc["macros"][&id];
        let text = |key: &str, default: &str| t.get(key).and_then(|v| v.as_str()).unwrap_or(default).to_string();
        let steps = t
            .get("steps")
            .and_then(Item::as_array)
            .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();
        let settings = MacroEdit {
            name: text("name", ""),
            kind: text("type", "once").to_ascii_lowercase(),
            stop: text("stop", "finish").to_ascii_lowercase(),
            char_delay: t.get("char_delay").and_then(|v| v.as_integer()).map(|v| v as u16),
            steps,
        };
        let target = format!("macro:{id}");
        let mut used_by = Vec::new();
        for (layer, name) in layer_names {
            for k in 0..=ENCODER_PRESS {
                if self.raw(layer, k).trim() == target {
                    let key = if k == ENCODER_PRESS { "knob".to_string() } else { format!("key {}", k + 1) };
                    used_by.push(format!("{name} · {key}"));
                }
            }
        }
        MacroView { id, settings, used_by }
    }

    fn layer_mut(&mut self, id: &str) -> Result<&mut Table> {
        self.doc
            .get_mut("layers")
            .and_then(Item::as_table_mut)
            .and_then(|t| t.get_mut(id))
            .and_then(Item::as_table_mut)
            .ok_or_else(|| anyhow!("no layer '{id}'"))
    }

    fn check_binding(&self, text: &str) -> Result<()> {
        let layers = self.layer_ids();
        let macros = self.macro_ids();
        parse_binding(
            text,
            |id| layers.iter().position(|l| l == id).map(|i| LayerId(i as u8)),
            |id| macros.iter().position(|m| m == id).map(|i| MacroId(i as u8)),
        )
        .map(|_| ())
        .map_err(|e| anyhow!("'{text}': {e}"))
    }

    /// Sets key `index` (0-11, or [`ENCODER_PRESS`]) of a layer. Invalid
    /// bindings are rejected and leave the document unchanged.
    pub fn set_key(&mut self, layer: &str, index: usize, binding: &str) -> Result<()> {
        ensure!(index <= ENCODER_PRESS, "key index {index} out of range");
        self.check_binding(binding)?;
        let table = self.layer_mut(layer)?;
        if index == ENCODER_PRESS {
            set_inline(table, "encoder", "press", binding);
        } else {
            let keys = table.entry("keys").or_insert_with(|| {
                Item::Value(Value::Array(grid_array(&vec!["_".to_string(); KEY_COUNT])))
            });
            let arr = keys.as_array_mut().ok_or_else(|| anyhow!("keys in [layers.{layer}] is not a list"))?;
            while arr.len() < KEY_COUNT {
                arr.push("_");
            }
            arr.replace(index, binding);
        }
        self.recompile();
        Ok(())
    }

    /// Sets the encoder rotation binding (`cw` or `ccw`).
    pub fn set_encoder(&mut self, layer: &str, direction: &str, binding: &str) -> Result<()> {
        ensure!(direction == "cw" || direction == "ccw", "direction must be cw or ccw");
        parse_encoder(binding).map_err(|e| anyhow!("'{binding}': {e}"))?;
        set_inline(self.layer_mut(layer)?, "encoder", direction, binding);
        self.recompile();
        Ok(())
    }

    pub fn set_layer_name(&mut self, layer: &str, name: &str) -> Result<()> {
        let table = self.layer_mut(layer)?;
        if name.is_empty() {
            table.remove("name");
        } else {
            table.insert("name", toml_edit::value(name));
        }
        self.recompile();
        Ok(())
    }

    /// Sets a layer's fallthrough parent (`None` = the base layer default).
    pub fn set_layer_parent(&mut self, layer: &str, parent: Option<&str>) -> Result<()> {
        ensure!(layer != BASE, "the base layer has no parent");
        if let Some(p) = parent {
            ensure!(p == NO_PARENT || self.layer_ids().iter().any(|l| l == p), "no layer '{p}'");
        }
        let table = self.layer_mut(layer)?;
        match parent {
            None | Some(BASE) => {
                table.remove("parent");
            }
            Some(p) => {
                table.insert("parent", toml_edit::value(p));
            }
        }
        self.recompile();
        Ok(())
    }

    /// Adds an empty layer (all keys inherit) after the existing ones.
    pub fn add_layer(&mut self, id: &str, name: &str) -> Result<()> {
        check_id(id, "layer")?;
        ensure!(id != NO_PARENT, "'{NO_PARENT}' is reserved");
        ensure!(!self.layer_ids().iter().any(|l| l == id), "layer '{id}' already exists");
        let layers = self.doc.entry("layers").or_insert_with(|| {
            let mut t = Table::new();
            t.set_implicit(true);
            Item::Table(t)
        });
        let layers = layers.as_table_mut().ok_or_else(|| anyhow!("'layers' is not a table"))?;
        let mut t = Table::new();
        if !name.is_empty() {
            t.insert("name", toml_edit::value(name));
        }
        t.insert("keys", Item::Value(Value::Array(grid_array(&vec!["_".to_string(); KEY_COUNT]))));
        layers.insert(id, Item::Table(t));
        self.recompile();
        Ok(())
    }

    /// Removes a layer. Bindings that still point at it make the document
    /// invalid until they are changed, which the view reports.
    pub fn remove_layer(&mut self, id: &str) -> Result<()> {
        if id == BASE {
            bail!("the base layer can't be removed");
        }
        let layers = self.doc.get_mut("layers").and_then(Item::as_table_mut).ok_or_else(|| anyhow!("no layers"))?;
        layers.remove(id).ok_or_else(|| anyhow!("no layer '{id}'"))?;
        self.recompile();
        Ok(())
    }

    fn raw(&self, layer: &str, index: usize) -> String {
        let table = self.layers_table().and_then(|t| t.get(layer));
        let item = match index {
            ENCODER_PRESS => table.and_then(|t| t.get("encoder")).and_then(|e| e.get("press")),
            i => table.and_then(|t| t.get("keys")).and_then(|k| k.get(i)),
        };
        item.and_then(|v| v.as_str()).unwrap_or("_").to_string()
    }

    fn raw_encoder(&self, layer: &str, direction: &str) -> String {
        self.layers_table()
            .and_then(|t| t.get(layer))
            .and_then(|l| l.get("encoder"))
            .and_then(|e| e.get(direction))
            .and_then(|v| v.as_str())
            .unwrap_or("_")
            .to_string()
    }

    pub fn view(&self) -> DocumentView {
        let config = self.compiled.as_ref().ok();
        let layers = self
            .layer_ids()
            .iter()
            .enumerate()
            .filter(|(_, id)| self.layers_table().is_some_and(|t| t.contains_key(id)))
            .map(|(i, id)| {
                let table = self.layers_table().and_then(|t| t.get(id));
                let keys = (0..=ENCODER_PRESS)
                    .map(|k| {
                        let raw = self.raw(id, k);
                        let button = if k == ENCODER_PRESS { Button::ENCODER } else { Button::key(k as u8).unwrap() };
                        let (label, category, color) = match config {
                            Some(c) => {
                                let b = c.resolve_button(LayerId(i as u8), button);
                                let mut text = String::new();
                                let _ = label::write_binding(&mut text, c, b);
                                let color = if k == ENCODER_PRESS {
                                    Rgb::BLACK
                                } else {
                                    led::idle_color(c, LayerId(i as u8), button)
                                };
                                (text, category_name(Category::of(b)), hex(color))
                            }
                            None => (String::new(), "unbound", hex(Rgb::BLACK)),
                        };
                        let inherited = raw == "_";
                        let colors = if k == ENCODER_PRESS { KeyColors::default() } else { self.key_colors_raw(id, k) };
                        KeyView { raw, label, inherited, category, color, colors }
                    })
                    .collect();
                LayerView {
                    id: id.clone(),
                    name: table.and_then(|t| t.get("name")).and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    parent: if i == 0 {
                        None
                    } else {
                        Some(
                            table
                                .and_then(|t| t.get("parent"))
                                .and_then(|v| v.as_str())
                                .unwrap_or(BASE)
                                .to_string(),
                        )
                    },
                    keys,
                    encoder_cw: self.raw_encoder(id, "cw"),
                    encoder_ccw: self.raw_encoder(id, "ccw"),
                    palette: self.layer_palette(id, i),
                }
            })
            .collect::<Vec<LayerView>>();
        let layer_names: Vec<(String, String)> = layers
            .iter()
            .map(|l: &LayerView| (l.id.clone(), if l.name.is_empty() { l.id.clone() } else { l.name.clone() }))
            .collect();
        let macros = self.macro_ids().into_iter().map(|id| self.macro_view(id, &layer_names)).collect();
        DocumentView {
            name: self.doc.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            layers,
            macros,
            settings: self.settings_view(),
            error: self.compiled.as_ref().err().cloned(),
        }
    }
}

/// Sets `table.<key>.<field>`, creating `key` as an inline table if needed.
fn set_inline(table: &mut Table, key: &str, field: &str, value: &str) {
    let item = table.entry(key).or_insert_with(|| Item::Value(Value::InlineTable(InlineTable::new())));
    if let Some(t) = item.as_inline_table_mut() {
        t.insert(field, Value::from(value));
    } else if let Some(t) = item.as_table_mut() {
        t.insert(field, toml_edit::value(value));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEMO: &str = include_str!("../../../configs/demo.toml");

    #[test]
    fn view_of_demo() {
        let d = ConfigDocument::parse(DEMO).unwrap();
        let v = d.view();
        assert_eq!(v.error, None);
        assert_eq!(v.name, "Demo");
        assert_eq!(v.layers[0].id, "base");
        assert_eq!(v.layers[0].keys[0].raw, "layer:one");
        assert_eq!(v.layers[0].keys[0].label, ">One");
        assert_eq!(v.layers[0].keys[0].category, "layer");
        // Layer "one" inherits key 4 ("b") from base.
        let one = &v.layers[1];
        assert_eq!(one.parent.as_deref(), Some("base"));
        assert!(one.keys[3].inherited);
        assert_eq!(one.keys[3].label, "B");
        assert_eq!(v.layers[0].keys[ENCODER_PRESS].raw, "toggle:tools");
        assert!(v.macros.iter().any(|m| m.id == "hello"));
    }

    #[test]
    fn edits_preserve_comments_and_recompile() {
        let mut d = ConfigDocument::parse(DEMO).unwrap();
        d.set_key("one", 4, "ctrl+c").unwrap();
        let text = d.text();
        assert!(text.contains("# InputDeck demo config"));
        assert!(text.contains("\"ctrl+c\""));
        let v = d.view();
        assert_eq!(v.layers[1].keys[4].label, "C-C");
        assert!(!v.layers[1].keys[4].inherited);
        assert_eq!(v.error, None);
    }

    #[test]
    fn invalid_bindings_are_rejected_without_changes() {
        let mut d = ConfigDocument::parse(DEMO).unwrap();
        let before = d.text();
        assert!(d.set_key("base", 0, "layer:nope").is_err());
        assert!(d.set_key("base", 0, "ctlr+c").is_err());
        assert!(d.set_encoder("base", "cw", "layer:one").is_err());
        assert_eq!(d.text(), before);
    }

    #[test]
    fn encoder_and_layer_management() {
        let mut d = ConfigDocument::parse(DEMO).unwrap();
        d.add_layer("nav", "Nav").unwrap();
        assert!(d.add_layer("nav", "").is_err());
        d.set_key("nav", ENCODER_PRESS, "mute").unwrap();
        d.set_encoder("nav", "cw", "right").unwrap();
        d.set_layer_parent("nav", Some("one")).unwrap();
        d.set_key("base", 2, "layer:nav").unwrap();
        let v = d.view();
        assert_eq!(v.error, None, "{}", d.text());
        let nav = v.layers.iter().find(|l| l.id == "nav").unwrap();
        assert_eq!(nav.name, "Nav");
        assert_eq!(nav.parent.as_deref(), Some("one"));
        assert_eq!(nav.encoder_cw, "right");
        assert_eq!(nav.keys[ENCODER_PRESS].raw, "mute");
        // Removing a referenced layer leaves an error to fix.
        d.remove_layer("nav").unwrap();
        assert!(d.view().error.is_some());
        assert!(d.remove_layer("base").is_err());
    }

    #[test]
    fn new_layer_keys_are_a_grid() {
        let mut d = ConfigDocument::parse("[layers.base]\n").unwrap();
        d.add_layer("x", "").unwrap();
        d.set_key("x", 5, "a").unwrap();
        let text = d.text();
        assert!(text.contains("keys = [\n  \"_\", \"_\", \"_\",\n  \"_\", \"_\", \"a\","), "{text}");
        assert!(ConfigDocument::parse(&text).unwrap().view().error.is_none());
    }

    #[test]
    fn macro_editing() {
        let mut d = ConfigDocument::parse(DEMO).unwrap();
        let v = d.view();
        let ws = v.macros.iter().find(|m| m.id == "word_skip").unwrap();
        assert_eq!(ws.settings.kind, "while_held");
        assert_eq!(ws.settings.steps, ["ctrl+right", "delay:120"]);
        assert_eq!(v.macros.iter().find(|m| m.id == "hello").unwrap().used_by, ["Tools · key 1"]);

        let mut edit = ws.settings.clone();
        edit.steps.push("text:hi there".into());
        edit.kind = "toggle".into();
        edit.stop = "immediate".into();
        d.set_macro("word_skip", &edit).unwrap();
        let text = d.text();
        assert!(text.contains("# jump word by word while held"), "{text}");
        assert!(text.contains("stop = \"immediate\""), "{text}");
        assert_eq!(d.view().error, None);
        assert_eq!(d.view().macros.iter().find(|m| m.id == "word_skip").unwrap().settings, edit);

        // Invalid steps are rejected without changes.
        let before = d.text();
        edit.steps.push("tap:ctlr+x".into());
        assert!(d.set_macro("word_skip", &edit).unwrap_err().to_string().contains("step 4"));
        edit.steps.pop();
        edit.kind = "sometimes".into();
        assert!(d.set_macro("word_skip", &edit).is_err());
        assert_eq!(d.text(), before);

        // Back to defaults removes the optional fields.
        edit.kind = "once".into();
        edit.stop = "finish".into();
        d.set_macro("word_skip", &edit).unwrap();
        assert!(!d.text().contains("stop = \"immediate\""));
        assert!(!d.text().contains("type = \"once\""));

        d.add_macro("new_one", "New").unwrap();
        assert!(d.add_macro("new_one", "").is_err());
        assert!(d.add_macro("bad id", "").is_err());
        d.set_key("base", 3, "macro:new_one").unwrap();
        assert_eq!(d.view().error, None, "{}", d.text());
        d.remove_macro("new_one").unwrap();
        assert!(d.view().error.is_some());
    }

    #[test]
    fn long_step_lists_go_on_separate_lines() {
        let mut d = ConfigDocument::parse("[layers.base]\n").unwrap();
        d.add_macro("m", "").unwrap();
        let steps: Vec<String> = (0..6).map(|i| format!("delay:{i}")).collect();
        let edit = MacroEdit { name: String::new(), kind: "once".into(), stop: "finish".into(), char_delay: None, steps };
        d.set_macro("m", &edit).unwrap();
        assert!(d.text().contains("steps = [\n  \"delay:0\",\n  \"delay:1\","), "{}", d.text());
        assert_eq!(d.view().error, None);
    }

    #[test]
    fn view_serializes() {
        let d = ConfigDocument::parse(DEMO).unwrap();
        let json = serde_json::to_string(&d.view()).unwrap();
        assert!(json.contains("\"layers\""));
    }
}
