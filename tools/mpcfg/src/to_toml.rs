//! Writes an engine [`Config`] back as a TOML config file (for `mpcfg pull`).
//!
//! Layer and macro ids aren't stored on the device, so they are derived
//! from names. Everything else is written explicitly, so the output
//! compiles back to the identical config.

use std::collections::HashSet;
use std::fmt::Write;

use macropad_engine::led::{Color, HeldStyle, KeyColor, Palette};
use macropad_engine::notation::{write_action, write_binding, write_color, write_step};
use macropad_engine::{
    Action, Button, Config, EncoderBinding, HostLed, KEY_COUNT, KeyBinding, KeyStroke, MacroKind, StopMode,
};

use crate::toml_config::{BASE, NO_PARENT};

fn quote(s: &str) -> String {
    toml::Value::String(s.to_string()).to_string()
}

/// Lowercase name with non-alphanumerics turned into `_`, made unique.
fn make_ids(names: impl Iterator<Item = String>, fallback: &str, reserved: &[&str]) -> Vec<String> {
    let mut used: HashSet<String> = reserved.iter().map(|s| s.to_string()).collect();
    names
        .enumerate()
        .map(|(i, name)| {
            let slug: String = name
                .to_lowercase()
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
                .collect::<String>()
                .trim_matches('_')
                .to_string();
            let base = if slug.is_empty() { format!("{fallback}{i}") } else { slug };
            let mut id = base.clone();
            let mut n = 2;
            while !used.insert(id.clone()) {
                id = format!("{base}_{n}");
                n += 1;
            }
            id
        })
        .collect()
}

/// The smallest whole percent that compiles back to `v` (see `percent` in
/// toml_config), or the closest one.
fn to_percent(v: u8) -> u8 {
    (0..=100u8).find(|&p| (p as u16 * 255 / 100) as u8 == v).unwrap_or(((v as u16 * 100 + 127) / 255) as u8)
}

fn color(c: Color) -> String {
    let mut s = String::new();
    write_color(&mut s, c).unwrap();
    quote(&s)
}

fn palette_entries(p: &Palette, all: bool) -> String {
    let entries = [
        ("layer_key", p.layer_key),
        ("key", p.key),
        ("modified", p.modified),
        ("media", p.media),
        ("macro", p.macro_key),
        ("unbound", p.unbound),
    ];
    entries
        .iter()
        .filter(|(_, c)| all || *c != Color::Inherit)
        .map(|(name, c)| format!("{name} = {}", color(*c)))
        .collect::<Vec<_>>()
        .join(", ")
}

pub fn write(config: &Config) -> String {
    let layer_ids: Vec<String> = std::iter::once(BASE.to_string())
        .chain(make_ids(config.layers.iter().skip(1).map(|l| l.name.clone()), "layer", &[BASE, NO_PARENT]))
        .collect();
    let macro_ids = make_ids(config.macros.iter().map(|m| m.name.clone()), "macro", &[]);

    let binding = |b: KeyBinding| {
        let mut s = String::new();
        write_binding(
            &mut s,
            b,
            |w, id| w.write_str(&layer_ids[id.0 as usize]),
            |w, id| w.write_str(&macro_ids[id.0 as usize]),
        )
        .unwrap();
        s
    };
    let encoder = |b: EncoderBinding| match b {
        EncoderBinding::Inherit => "_".to_string(),
        EncoderBinding::None => "none".to_string(),
        EncoderBinding::Action(a) => {
            let mut s = String::new();
            write_action(&mut s, a).unwrap();
            s
        }
    };

    let mut out = String::new();
    let o = &mut out;
    writeln!(o, "# Macropad OS config, pulled from the device.").unwrap();
    writeln!(o, "# See configs/demo.toml in the repository for the format.\n").unwrap();
    if !config.name.is_empty() {
        writeln!(o, "name = {}\n", quote(&config.name)).unwrap();
    }
    // The char map is written out in full, so the file doesn't depend on
    // the layout being available where it is compiled.
    writeln!(o, "[layout]").unwrap();
    if !config.layout_name.is_empty() {
        writeln!(o, "base = {}", quote(&config.layout_name)).unwrap();
    }
    let stroke = |s: &KeyStroke| {
        let mut t = String::new();
        write_action(&mut t, Action::Key { code: s.code, mods: s.mods }).unwrap();
        quote(&t)
    };
    let chars: Vec<String> = config
        .char_map
        .iter()
        .map(|c| {
            let keys = match c.keys.as_slice() {
                [one] => stroke(one),
                many => format!("[{}]", many.iter().map(stroke).collect::<Vec<_>>().join(", ")),
            };
            format!("{} = {keys}", quote(&c.ch.to_string()))
        })
        .collect();
    writeln!(o, "chars = {{ {} }}\n", chars.join(", ")).unwrap();

    writeln!(o, "[timing]\ntap = {}\nchar_delay = {}\n", config.timing.tap_ms, config.timing.char_delay_ms).unwrap();

    let l = &config.leds;
    let held_style = match l.held_style {
        HeldStyle::Brighten => "brighten",
        HeldStyle::White => "white",
        HeldStyle::Invert => "invert",
    };
    writeln!(o, "[leds]").unwrap();
    writeln!(o, "enabled = {}", l.enabled).unwrap();
    writeln!(o, "brightness = {}", to_percent(l.brightness)).unwrap();
    writeln!(o, "idle_level = {}", to_percent(l.idle_level)).unwrap();
    writeln!(o, "held_style = {}", quote(held_style)).unwrap();
    writeln!(o, "palette = {{ {} }}\n", palette_entries(&l.palette, true)).unwrap();

    writeln!(o, "[idle]\ndim_after = {}\noff_after = {}\n", config.idle.dim_after_s, config.idle.off_after_s).unwrap();

    writeln!(o, "[global]").unwrap();
    for (i, action) in config.global.iter().enumerate() {
        if let Some(a) = action {
            let key = if i < KEY_COUNT { quote(&(i + 1).to_string()) } else { "encoder".to_string() };
            let mut s = String::new();
            write_action(&mut s, *a).unwrap();
            writeln!(o, "{key} = {}", quote(&s)).unwrap();
        }
    }
    writeln!(o).unwrap();

    writeln!(o, "[indicators]").unwrap();
    for ind in &config.indicators {
        let name = match ind.led {
            HostLed::CapsLock => "caps_lock",
            HostLed::NumLock => "num_lock",
            HostLed::ScrollLock => "scroll_lock",
        };
        writeln!(o, "{name} = {{ key = {}, color = {} }}", ind.button.index() + 1, color(Color::Rgb(ind.color)))
            .unwrap();
    }
    writeln!(o).unwrap();

    for (i, layer) in config.layers.iter().enumerate() {
        writeln!(o, "[layers.{}]", layer_ids[i]).unwrap();
        if !layer.name.is_empty() {
            writeln!(o, "name = {}", quote(&layer.name)).unwrap();
        }
        if i != 0 {
            match layer.parent {
                Some(p) if p.0 == 0 => {}
                Some(p) => writeln!(o, "parent = {}", quote(&layer_ids[p.0 as usize])).unwrap(),
                None => writeln!(o, "parent = {}", quote(NO_PARENT)).unwrap(),
            }
        }
        let cells: Vec<String> = layer.buttons[..KEY_COUNT].iter().map(|b| quote(&binding(*b))).collect();
        let width = cells.iter().map(String::len).max().unwrap_or(0);
        writeln!(o, "keys = [").unwrap();
        for row in cells.chunks(3) {
            let line: Vec<String> = row.iter().map(|c| format!("{:<w$}", format!("{c},"), w = width + 1)).collect();
            writeln!(o, "  {}", line.join(" ").trim_end()).unwrap();
        }
        writeln!(o, "]").unwrap();
        writeln!(
            o,
            "encoder = {{ cw = {}, ccw = {}, press = {} }}",
            quote(&encoder(layer.encoder_cw)),
            quote(&encoder(layer.encoder_ccw)),
            quote(&binding(layer.buttons[Button::ENCODER.index()]))
        )
        .unwrap();
        let palette = palette_entries(&layer.palette, false);
        if !palette.is_empty() {
            writeln!(o, "palette = {{ {palette} }}").unwrap();
        }
        let colors: Vec<String> = layer
            .key_colors
            .iter()
            .enumerate()
            .filter(|(_, c)| **c != KeyColor::INHERIT)
            .map(|(k, c)| {
                let fields: Vec<String> = [("idle", c.idle), ("held", c.held), ("active", c.active)]
                    .iter()
                    .filter(|(_, c)| *c != Color::Inherit)
                    .map(|(n, c)| format!("{n} = {}", color(*c)))
                    .collect();
                format!("{} = {{ {} }}", k + 1, fields.join(", "))
            })
            .collect();
        if !colors.is_empty() {
            writeln!(o, "colors = {{ {} }}", colors.join(", ")).unwrap();
        }
        writeln!(o).unwrap();
    }

    for (i, m) in config.macros.iter().enumerate() {
        writeln!(o, "[macros.{}]", macro_ids[i]).unwrap();
        if !m.name.is_empty() {
            writeln!(o, "name = {}", quote(&m.name)).unwrap();
        }
        let kind = match m.kind {
            MacroKind::OneShot => "once",
            MacroKind::WhileHeld => "while_held",
            MacroKind::Toggle => "toggle",
        };
        writeln!(o, "type = {}", quote(kind)).unwrap();
        let stop = match m.stop {
            StopMode::FinishRun => "finish",
            StopMode::Immediate => "immediate",
        };
        writeln!(o, "stop = {}", quote(stop)).unwrap();
        if let Some(d) = m.char_delay_ms {
            writeln!(o, "char_delay = {d}").unwrap();
        }
        let steps: Vec<String> = m
            .steps
            .iter()
            .map(|s| {
                let mut t = String::new();
                write_step(&mut t, s).unwrap();
                quote(&t)
            })
            .collect();
        writeln!(o, "steps = [{}]\n", steps.join(", ")).unwrap();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::toml_config::parse;

    const DEMO: &str = include_str!("../../../configs/demo.toml");

    #[test]
    fn demo_roundtrips() {
        let original = parse(DEMO).unwrap();
        let text = write(&original);
        let back = parse(&text).unwrap_or_else(|e| panic!("{e:#}\n{text}"));
        assert_eq!(back, original, "\n{text}");
    }

    #[test]
    fn default_config_roundtrips() {
        let config = macropad_engine::Config::new(vec![macropad_engine::Layer::empty()]);
        let back = parse(&write(&config)).unwrap();
        assert_eq!(back, config);
    }

    #[test]
    fn char_map_with_sequences_roundtrips() {
        let text = "layout = 'de'\n[layers.base]\n[macros.x]\nsteps = ['text:^é\\n ß']\n";
        let original = parse(text).unwrap();
        let back = parse(&write(&original)).unwrap();
        assert_eq!(back, original);
    }

    #[test]
    fn ids_are_derived_from_names_and_unique() {
        let ids = make_ids(["Nav".into(), "nav".into(), "".into(), "Base".into()].into_iter(), "layer", &["base"]);
        assert_eq!(ids, ["nav", "nav_2", "layer2", "base_2"]);
    }

    #[test]
    fn percent_roundtrip() {
        for p in 0..=100u8 {
            let v = (p as u16 * 255 / 100) as u8;
            assert_eq!((to_percent(v) as u16 * 255 / 100) as u8, v);
        }
    }
}
