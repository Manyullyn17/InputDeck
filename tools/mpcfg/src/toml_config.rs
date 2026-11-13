//! The human-editable TOML config format and its conversion to the
//! engine's [`Config`]. See `configs/demo.toml` for an annotated example.

use std::collections::BTreeMap;

use anyhow::{Context, Result, anyhow, bail, ensure};
use indexmap::IndexMap;
use macropad_engine::led::{Color, HeldStyle, KeyColor, LedSettings, Palette};
use macropad_engine::notation::{parse_action, parse_binding, parse_color, parse_encoder};
use macropad_engine::{Button, Config, IdleSettings, KEY_COUNT, Layer, LayerId};
use serde::Deserialize;

/// Id of the layer that becomes layer 0.
pub const BASE: &str = "base";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct File {
    #[serde(default)]
    pub leds: LedsToml,
    #[serde(default)]
    pub idle: IdleToml,
    /// Layer-independent actions, keyed by key number ("1"-"12") or "encoder".
    #[serde(default)]
    pub global: BTreeMap<String, String>,
    pub layers: IndexMap<String, LayerToml>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LedsToml {
    pub enabled: Option<bool>,
    /// Percent.
    pub brightness: Option<u8>,
    /// Percent.
    pub idle_level: Option<u8>,
    pub held_style: Option<String>,
    pub palette: Option<PaletteToml>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdleToml {
    /// Seconds, 0 = never.
    pub dim_after: Option<u16>,
    /// Seconds, 0 = never.
    pub off_after: Option<u16>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PaletteToml {
    pub layer_key: Option<String>,
    pub key: Option<String>,
    pub modified: Option<String>,
    pub media: Option<String>,
    pub unbound: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LayerToml {
    pub name: Option<String>,
    pub parent: Option<String>,
    /// Exactly 12 bindings, row-major from the top-left key.
    pub keys: Option<Vec<String>>,
    pub encoder: Option<EncoderToml>,
    pub palette: Option<PaletteToml>,
    /// Per-key colors, keyed by key number ("1"-"12").
    #[serde(default)]
    pub colors: BTreeMap<String, KeyColorToml>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EncoderToml {
    pub cw: Option<String>,
    pub ccw: Option<String>,
    pub press: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeyColorToml {
    pub idle: Option<String>,
    pub held: Option<String>,
    pub active: Option<String>,
}

pub fn parse(text: &str) -> Result<Config> {
    let file: File = toml::from_str(text)?;
    compile(&file)
}

fn percent(p: u8, what: &str) -> Result<u8> {
    ensure!(p <= 100, "{what} must be 0-100 (percent), got {p}");
    Ok((p as u16 * 255 / 100) as u8)
}

fn color(text: &Option<String>, what: &str) -> Result<Color> {
    match text {
        None => Ok(Color::Inherit),
        Some(t) => parse_color(t).map_err(|e| anyhow!("{what}: '{t}': {e}")),
    }
}

fn palette(p: &PaletteToml, what: &str) -> Result<Palette> {
    Ok(Palette {
        layer_key: color(&p.layer_key, &format!("{what}.layer_key"))?,
        key: color(&p.key, &format!("{what}.key"))?,
        modified: color(&p.modified, &format!("{what}.modified"))?,
        media: color(&p.media, &format!("{what}.media"))?,
        unbound: color(&p.unbound, &format!("{what}.unbound"))?,
    })
}

/// "1"-"12" -> key index 0-11; "encoder" -> the encoder button.
fn button(name: &str, allow_encoder: bool) -> Result<Button> {
    if allow_encoder && name == "encoder" {
        return Ok(Button::ENCODER);
    }
    name.parse::<u8>()
        .ok()
        .filter(|n| (1..=KEY_COUNT as u8).contains(n))
        .and_then(|n| Button::key(n - 1))
        .ok_or_else(|| anyhow!("'{name}' is not a key number (1-12){}", if allow_encoder { " or 'encoder'" } else { "" }))
}

pub fn compile(file: &File) -> Result<Config> {
    // Layer ids -> indices: base first, then file order.
    ensure!(file.layers.contains_key(BASE), "missing [layers.{BASE}]");
    let ids: Vec<&str> =
        std::iter::once(BASE).chain(file.layers.keys().map(String::as_str).filter(|&id| id != BASE)).collect();
    ensure!(ids.len() <= 256, "at most 256 layers are supported");
    let index = |id: &str| ids.iter().position(|&x| x == id).map(|i| LayerId(i as u8));

    let mut layers = Vec::with_capacity(ids.len());
    for &id in &ids {
        let src = &file.layers[id];
        layers.push(compile_layer(id, src, &index).with_context(|| format!("in [layers.{id}]"))?);
    }

    let mut config = Config::new(layers);

    for (name, text) in &file.global {
        let b = button(name, true).context("in [global]")?;
        let action = parse_action(text).map_err(|e| anyhow!("in [global], {name} = '{text}': {e}"))?;
        config.global[b.index()] = Some(action);
    }

    let mut leds = LedSettings::DEFAULT;
    if let Some(v) = file.leds.enabled {
        leds.enabled = v;
    }
    if let Some(v) = file.leds.brightness {
        leds.brightness = percent(v, "leds.brightness")?;
    }
    if let Some(v) = file.leds.idle_level {
        leds.idle_level = percent(v, "leds.idle_level")?;
    }
    if let Some(style) = &file.leds.held_style {
        leds.held_style = match style.to_ascii_lowercase().as_str() {
            "brighten" => HeldStyle::Brighten,
            "white" => HeldStyle::White,
            "invert" => HeldStyle::Invert,
            other => bail!("leds.held_style: '{other}' (expected brighten, white or invert)"),
        };
    }
    if let Some(p) = &file.leds.palette {
        // Unset global entries keep their defaults rather than inheriting.
        let parsed = palette(p, "leds.palette")?;
        let pick = |c: Color, default: Color| if c == Color::Inherit { default } else { c };
        let d = Palette::DEFAULT;
        leds.palette = Palette {
            layer_key: pick(parsed.layer_key, d.layer_key),
            key: pick(parsed.key, d.key),
            modified: pick(parsed.modified, d.modified),
            media: pick(parsed.media, d.media),
            unbound: pick(parsed.unbound, d.unbound),
        };
    }
    config.leds = leds;

    let mut idle = IdleSettings::DEFAULT;
    if let Some(v) = file.idle.dim_after {
        idle.dim_after_s = v;
    }
    if let Some(v) = file.idle.off_after {
        idle.off_after_s = v;
    }
    config.idle = idle;

    config.validate().map_err(|e| anyhow!("invalid config: {e:?}"))?;
    Ok(config)
}

fn compile_layer(id: &str, src: &LayerToml, index: &impl Fn(&str) -> Option<LayerId>) -> Result<Layer> {
    let parent = match (&src.parent, id == BASE) {
        (Some(_), true) => bail!("the base layer cannot have a parent"),
        (None, true) => None,
        (None, false) => Some(LayerId::BASE),
        (Some(p), false) => Some(index(p).ok_or_else(|| anyhow!("parent '{p}' is not a layer"))?),
    };
    let mut layer = match parent {
        Some(p) => Layer::transparent(p),
        None => Layer::empty(),
    };
    layer.name = src.name.clone().unwrap_or_default();

    if let Some(keys) = &src.keys {
        ensure!(keys.len() == KEY_COUNT, "keys must list exactly {KEY_COUNT} bindings, found {}", keys.len());
        for (i, text) in keys.iter().enumerate() {
            layer.buttons[i] = parse_binding(text, index).map_err(|e| anyhow!("key {} = '{text}': {e}", i + 1))?;
        }
    }

    if let Some(enc) = &src.encoder {
        if let Some(t) = &enc.cw {
            layer.encoder_cw = parse_encoder(t).map_err(|e| anyhow!("encoder.cw = '{t}': {e}"))?;
        }
        if let Some(t) = &enc.ccw {
            layer.encoder_ccw = parse_encoder(t).map_err(|e| anyhow!("encoder.ccw = '{t}': {e}"))?;
        }
        if let Some(t) = &enc.press {
            layer.buttons[Button::ENCODER.index()] =
                parse_binding(t, index).map_err(|e| anyhow!("encoder.press = '{t}': {e}"))?;
        }
    }

    if let Some(p) = &src.palette {
        layer.palette = palette(p, "palette")?;
    }
    for (name, c) in &src.colors {
        let b = button(name, false).context("in colors")?;
        layer.key_colors[b.index()] = KeyColor {
            idle: color(&c.idle, &format!("colors.{name}.idle"))?,
            held: color(&c.held, &format!("colors.{name}.held"))?,
            active: color(&c.active, &format!("colors.{name}.active"))?,
        };
    }
    Ok(layer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use macropad_engine::led::Rgb;
    use macropad_engine::{Action, KeyBinding, KeyCode, Modifiers};

    const DEMO: &str = include_str!("../../../configs/demo.toml");

    fn key(code: u8) -> KeyBinding {
        KeyBinding::Action(Action::Key { code: KeyCode(code), mods: Modifiers::NONE })
    }

    #[test]
    fn demo_compiles_to_the_layer_example() {
        let c = parse(DEMO).unwrap();
        assert_eq!(c.layers.len(), 5);
        assert_eq!(c.layers[0].name, "Base");
        assert_eq!(c.resolve_button(LayerId(0), Button::key(0).unwrap()), KeyBinding::Layer(LayerId(1)));
        assert_eq!(c.resolve_button(LayerId(1), Button::key(1).unwrap()), KeyBinding::Layer(LayerId(3)));
        assert_eq!(c.resolve_button(LayerId(2), Button::key(0).unwrap()), KeyBinding::Layer(LayerId(4)));
        assert_eq!(c.resolve_button(LayerId(3), Button::key(2).unwrap()), key(0x20)); // "3"
        assert_eq!(c.resolve_button(LayerId(3), Button::key(4).unwrap()), key(0x06)); // inherited "c"
        assert_eq!(c.layers[1].palette.key, Color::Rgb(Rgb::new(0, 255, 180)));
        assert_eq!(c.layers[0].key_colors[11].held, Color::Rgb(Rgb::new(255, 220, 0)));
    }

    #[test]
    fn errors_name_the_location() {
        let err = parse("[layers.base]\nkeys = ['a','b','c','d','e','f','g','h','i','j','k','ctlr+c']\n")
            .unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("[layers.base]") && msg.contains("key 12") && msg.contains("modifier"), "{msg}");
    }

    #[test]
    fn rejects_structural_mistakes() {
        assert!(parse("[layers.nav]\n").is_err()); // no base
        assert!(parse("[layers.base]\nparent = 'x'\n").is_err());
        assert!(parse("[layers.base]\nkeys = ['a']\n").is_err());
        assert!(parse("[layers.base]\n[layers.nav]\nparent = 'missing'\n").is_err());
        assert!(parse("[layers.base]\nkeyz = []\n").is_err()); // typo caught
        assert!(parse("[layers.base]\n[layers.a]\nparent = 'b'\n[layers.b]\nparent = 'a'\n").is_err());
    }
}
