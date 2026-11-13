//! The human-editable TOML config format and its conversion to the
//! engine's [`Config`]. See `configs/demo.toml` for an annotated example.

use std::collections::BTreeMap;

use anyhow::{Context, Result, anyhow, bail, ensure};
use indexmap::IndexMap;
use macropad_engine::layout::KeyboardLayout;
use macropad_engine::led::{Color, HeldStyle, KeyColor, LedSettings, Palette};
use macropad_engine::notation::{parse_action, parse_binding, parse_color, parse_encoder, parse_step};
use macropad_engine::{
    Button, Config, ConfigError, IdleSettings, KEY_COUNT, Layer, LayerId, Macro, MacroId, MacroKind, MacroTiming,
    StopMode,
};
use serde::Deserialize;

/// Id of the layer that becomes layer 0.
pub const BASE: &str = "base";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct File {
    /// The computer's keyboard layout for typing text: "us" or "de".
    pub layout: Option<String>,
    #[serde(default)]
    pub timing: TimingToml,
    #[serde(default)]
    pub leds: LedsToml,
    #[serde(default)]
    pub idle: IdleToml,
    /// Layer-independent actions, keyed by key number ("1"-"12") or "encoder".
    #[serde(default)]
    pub global: BTreeMap<String, String>,
    pub layers: IndexMap<String, LayerToml>,
    #[serde(default)]
    pub macros: IndexMap<String, MacroToml>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimingToml {
    /// Milliseconds a tapped or typed key is held down.
    pub tap: Option<u16>,
    /// Default milliseconds between typed characters.
    pub char_delay: Option<u16>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MacroToml {
    pub name: Option<String>,
    /// "once" (default), "while_held" or "toggle".
    #[serde(rename = "type")]
    pub kind: Option<String>,
    /// Toggle macros: "finish" (default) or "immediate".
    pub stop: Option<String>,
    pub char_delay: Option<u16>,
    pub steps: Vec<String>,
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
    #[serde(rename = "macro")]
    pub macro_key: Option<String>,
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
        macro_key: color(&p.macro_key, &format!("{what}.macro"))?,
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
    let macro_ids: Vec<&str> = file.macros.keys().map(String::as_str).collect();
    ensure!(macro_ids.len() <= 256, "at most 256 macros are supported");
    let macro_index = |id: &str| macro_ids.iter().position(|&x| x == id).map(|i| MacroId(i as u8));

    let mut layers = Vec::with_capacity(ids.len());
    for &id in &ids {
        let src = &file.layers[id];
        layers.push(compile_layer(id, src, &index, &macro_index).with_context(|| format!("in [layers.{id}]"))?);
    }

    let mut config = Config::new(layers);

    for (id, src) in &file.macros {
        config.macros.push(compile_macro(src).with_context(|| format!("in [macros.{id}]"))?);
    }
    if let Some(layout) = &file.layout {
        config.layout = match layout.to_ascii_lowercase().as_str() {
            "us" => KeyboardLayout::Us,
            "de" => KeyboardLayout::De,
            other => bail!("layout: '{other}' (expected us or de)"),
        };
    }
    let mut timing = MacroTiming::DEFAULT;
    if let Some(v) = file.timing.tap {
        timing.tap_ms = v;
    }
    if let Some(v) = file.timing.char_delay {
        timing.char_delay_ms = v;
    }
    config.timing = timing;

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
            macro_key: pick(parsed.macro_key, d.macro_key),
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

    config.validate().map_err(|e| match e {
        ConfigError::UntypeableChar { macro_id, ch } => anyhow!(
            "in [macros.{}]: '{ch}' can't be typed with the {:?} keyboard layout",
            macro_ids[macro_id.0 as usize],
            config.layout
        ),
        other => anyhow!("invalid config: {other:?}"),
    })?;
    Ok(config)
}

fn compile_macro(src: &MacroToml) -> Result<Macro> {
    let kind = match src.kind.as_deref().map(str::to_ascii_lowercase).as_deref() {
        None | Some("once") => MacroKind::OneShot,
        Some("while_held") => MacroKind::WhileHeld,
        Some("toggle") => MacroKind::Toggle,
        Some(other) => bail!("type: '{other}' (expected once, while_held or toggle)"),
    };
    let stop = match src.stop.as_deref().map(str::to_ascii_lowercase).as_deref() {
        None | Some("finish") => StopMode::FinishRun,
        Some("immediate") => StopMode::Immediate,
        Some(other) => bail!("stop: '{other}' (expected finish or immediate)"),
    };
    let steps = src
        .steps
        .iter()
        .enumerate()
        .map(|(i, t)| parse_step(t).map_err(|e| anyhow!("step {} = '{t}': {e}", i + 1)))
        .collect::<Result<_>>()?;
    Ok(Macro { name: src.name.clone().unwrap_or_default(), kind, steps, stop, char_delay_ms: src.char_delay })
}

fn compile_layer(
    id: &str,
    src: &LayerToml,
    index: &impl Fn(&str) -> Option<LayerId>,
    macro_index: &impl Fn(&str) -> Option<MacroId>,
) -> Result<Layer> {
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
            layer.buttons[i] = parse_binding(text, index, macro_index).map_err(|e| anyhow!("key {} = '{text}': {e}", i + 1))?;
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
                parse_binding(t, index, macro_index).map_err(|e| anyhow!("encoder.press = '{t}': {e}"))?;
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
        assert_eq!(c.layers.len(), 6);
        assert_eq!(c.macros.len(), 4);
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
    fn macros_and_toggles() {
        let text = r#"
            layout = "de"
            [layers.base]
            keys = ["macro:hi", "latch:shift", "repeat:a@50", "toggle:nav",
                    "_", "_", "_", "_", "_", "_", "_", "_"]
            [layers.nav]
            [macros.hi]
            name = "Hi"
            type = "toggle"
            stop = "immediate"
            steps = ["down:shift", "h", "up:shift", "delay:100ms", "text:allo ß"]
        "#;
        let c = parse(text).unwrap();
        assert_eq!(c.layout, KeyboardLayout::De);
        assert_eq!(c.macros.len(), 1);
        assert_eq!(c.macros[0].kind, MacroKind::Toggle);
        assert_eq!(c.macros[0].stop, StopMode::Immediate);
        assert_eq!(c.macros[0].steps.len(), 5);
        assert_eq!(c.layers[0].buttons[0], KeyBinding::Macro(MacroId(0)));
    }

    #[test]
    fn untypeable_text_is_reported() {
        let text = "[layers.base]\n[macros.x]\nsteps = ['text:straße']\n";
        let msg = format!("{:#}", parse(text).unwrap_err());
        assert!(msg.contains("[macros.x]") && msg.contains("'ß'") && msg.contains("Us"), "{msg}");
        assert!(parse(&format!("layout = 'de'\n{text}")).is_ok());
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
