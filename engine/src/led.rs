//! Key LED colors.
//!
//! Resolution for a key's base color, first match wins:
//! 1. the per-key color of the layer that defines the key's binding (so a
//!    color follows its binding when inherited),
//! 2. the current layer's palette entry for the binding's [`Category`],
//!    then its parent layers' palettes,
//! 3. the global palette in [`LedSettings`].
//!
//! The base color is then scaled by key state: idle keys at
//! [`LedSettings::idle_level`], held keys per [`HeldStyle`] (or their own
//! held color), and released layer keys still in the layer path dimmed.
//! [`apply_brightness`] and [`limit_current`] turn that into LED output.

use serde::{Deserialize, Serialize};

use crate::config::{Action, Button, Config, KEY_COUNT, KeyBinding, LayerId, Source, Toggle};
use crate::engine::{Engine, KeyLight};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const BLACK: Rgb = Rgb::new(0, 0, 0);
    pub const WHITE: Rgb = Rgb::new(255, 255, 255);

    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Rgb { r, g, b }
    }

    /// Scales each channel by `level / 255`.
    pub const fn scale(self, level: u8) -> Rgb {
        const fn s(c: u8, l: u8) -> u8 {
            ((c as u16 * l as u16 + 127) / 255) as u8
        }
        Rgb::new(s(self.r, level), s(self.g, level), s(self.b, level))
    }

    pub const fn invert(self) -> Rgb {
        Rgb::new(255 - self.r, 255 - self.g, 255 - self.b)
    }
}

/// A color setting. `Inherit` defers to the next level of resolution.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Color {
    Inherit,
    Off,
    Rgb(Rgb),
}

/// Colors for one key's states. `Inherit` means: idle uses the palette,
/// held uses the [`HeldStyle`] on the idle color, active uses the held
/// color.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyColor {
    pub idle: Color,
    pub held: Color,
    /// While a toggle is on. Reserved until toggles exist.
    pub active: Color,
}

impl KeyColor {
    pub const INHERIT: KeyColor = KeyColor { idle: Color::Inherit, held: Color::Inherit, active: Color::Inherit };
}

/// Kinds of bindings that get their own palette color.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Category {
    LayerKey,
    /// A key without modifiers.
    Key,
    /// A key with modifiers (shortcuts).
    Modified,
    /// Consumer control (media keys).
    Media,
    Macro,
    Unbound,
}

impl Category {
    pub fn of(binding: KeyBinding) -> Category {
        match binding {
            KeyBinding::Layer(_) => Category::LayerKey,
            KeyBinding::Action(Action::Key { mods, .. }) if mods.0 != 0 => Category::Modified,
            KeyBinding::Action(Action::Key { .. }) => Category::Key,
            KeyBinding::Action(Action::Consumer(_)) => Category::Media,
            KeyBinding::Toggle(Toggle::Latch(a) | Toggle::Repeat { action: a, .. }) => {
                Category::of(KeyBinding::Action(a))
            }
            KeyBinding::Toggle(Toggle::Layer(_)) => Category::LayerKey,
            // System bindings share the macro color.
            KeyBinding::Macro(_) | KeyBinding::System(_) => Category::Macro,
            KeyBinding::None | KeyBinding::Inherit => Category::Unbound,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Palette {
    pub layer_key: Color,
    pub key: Color,
    pub modified: Color,
    pub media: Color,
    pub macro_key: Color,
    pub unbound: Color,
}

impl Palette {
    pub const INHERIT: Palette = Palette {
        layer_key: Color::Inherit,
        key: Color::Inherit,
        modified: Color::Inherit,
        media: Color::Inherit,
        macro_key: Color::Inherit,
        unbound: Color::Inherit,
    };

    pub const DEFAULT: Palette = Palette {
        layer_key: Color::Rgb(Rgb::new(0, 90, 255)),
        key: Color::Rgb(Rgb::new(255, 150, 60)),
        modified: Color::Rgb(Rgb::new(170, 0, 255)),
        media: Color::Rgb(Rgb::new(0, 255, 70)),
        macro_key: Color::Rgb(Rgb::new(255, 210, 0)),
        unbound: Color::Off,
    };

    pub fn get(&self, category: Category) -> Color {
        match category {
            Category::LayerKey => self.layer_key,
            Category::Key => self.key,
            Category::Modified => self.modified,
            Category::Media => self.media,
            Category::Macro => self.macro_key,
            Category::Unbound => self.unbound,
        }
    }
}

/// How a held key without its own held color is shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum HeldStyle {
    /// The idle color at full level.
    Brighten,
    White,
    /// The complement of the idle color.
    Invert,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LedSettings {
    pub enabled: bool,
    /// Overall brightness, perceptual (gamma-corrected), 0-255.
    pub brightness: u8,
    /// Level of idle keys relative to held ones, 0-255.
    pub idle_level: u8,
    pub held_style: HeldStyle,
    /// Global palette. `Inherit` entries here mean off.
    pub palette: Palette,
}

impl LedSettings {
    pub const DEFAULT: LedSettings = LedSettings {
        enabled: true,
        brightness: 160,
        idle_level: 112, // 44%, exactly representable as a whole percent
        held_style: HeldStyle::Brighten,
        palette: Palette::DEFAULT,
    };
}

/// Level of released layer keys still in the path, relative to idle.
const PATH_ANCHOR_LEVEL: u8 = 70;

/// Perceptual colors for all keys (before brightness and gamma).
/// `host_leds` is the lock state reported by the computer; configured
/// indicators replace their key's color while their lock is on.
pub fn key_colors(config: &Config, engine: &Engine, host_leds: u8) -> [Rgb; KEY_COUNT] {
    let mut colors = [Rgb::BLACK; KEY_COUNT];
    if !config.leds.enabled {
        return colors;
    }
    for (i, color) in colors.iter_mut().enumerate() {
        let button = Button::key(i as u8).unwrap();
        *color = match engine.key_light(button) {
            KeyLight::Idle => base(config, engine.layer(), button).scale(config.leds.idle_level),
            KeyLight::Held { pressed_on } | KeyLight::Running { pressed_on } => held(config, pressed_on, button),
            KeyLight::Active { pressed_on } => active(config, pressed_on, button),
            KeyLight::PathAnchor { pressed_on } => base(config, pressed_on, button)
                .scale(config.leds.idle_level)
                .scale(PATH_ANCHOR_LEVEL),
        };
    }
    for ind in &config.indicators {
        if ind.led.is_on(host_leds)
            && let Some(c) = colors.get_mut(ind.button.index())
        {
            *c = ind.color;
        }
    }
    colors
}

fn own_color(config: &Config, source: Source, button: Button) -> Option<KeyColor> {
    match source {
        Source::Layer(id) => config.layer(id).map(|l| l.key_colors[button.index()]),
        Source::Global | Source::Unbound => None,
    }
}

/// The key's idle color at full level on `layer`.
fn base(config: &Config, layer: LayerId, button: Button) -> Rgb {
    let (binding, source) = config.resolve_button_source(layer, button);
    let own = own_color(config, source, button).map_or(Color::Inherit, |c| c.idle);
    let color = match own {
        Color::Inherit => palette_color(config, layer, Category::of(binding)),
        explicit => explicit,
    };
    match color {
        Color::Rgb(rgb) => rgb,
        Color::Off | Color::Inherit => Rgb::BLACK,
    }
}

fn palette_color(config: &Config, layer: LayerId, category: Category) -> Color {
    config
        .walk(layer, |_, l| match l.palette.get(category) {
            Color::Inherit => None,
            explicit => Some(explicit),
        })
        .unwrap_or(config.leds.palette.get(category))
}

fn held(config: &Config, pressed_on: LayerId, button: Button) -> Rgb {
    let (_, source) = config.resolve_button_source(pressed_on, button);
    match own_color(config, source, button).map_or(Color::Inherit, |c| c.held) {
        Color::Rgb(rgb) => rgb,
        Color::Off => Rgb::BLACK,
        Color::Inherit => {
            let idle = base(config, pressed_on, button);
            match config.leds.held_style {
                HeldStyle::Brighten => idle,
                HeldStyle::White => Rgb::WHITE,
                HeldStyle::Invert => idle.invert(),
            }
        }
    }
}

/// Toggle on: the key's own active color, else its held color.
fn active(config: &Config, pressed_on: LayerId, button: Button) -> Rgb {
    let (_, source) = config.resolve_button_source(pressed_on, button);
    match own_color(config, source, button).map_or(Color::Inherit, |c| c.active) {
        Color::Rgb(rgb) => rgb,
        Color::Off => Rgb::BLACK,
        Color::Inherit => held(config, pressed_on, button),
    }
}

/// Gamma 2 correction: maps perceptual 0-255 to PWM 0-255, keeping
/// nonzero values nonzero.
pub const fn gamma(v: u8) -> u8 {
    (v as u16 * v as u16).div_ceil(255) as u8
}

/// Applies overall brightness (perceptual) and gamma correction.
pub fn apply_brightness(colors: &mut [Rgb], brightness: u8) {
    for c in colors {
        let s = c.scale(brightness);
        *c = Rgb::new(gamma(s.r), gamma(s.g), gamma(s.b));
    }
}

/// Approximate current of one fully lit LED channel, in milliamps.
const MA_PER_CHANNEL: u32 = 20;

/// Scales output colors down uniformly so the estimated LED current stays
/// within `max_ma`. Call after [`apply_brightness`].
pub fn limit_current(colors: &mut [Rgb], max_ma: u32) {
    let total: u32 = colors.iter().map(|c| c.r as u32 + c.g as u32 + c.b as u32).sum();
    let budget = max_ma * 255 / MA_PER_CHANNEL;
    if total <= budget {
        return;
    }
    let level = (budget * 255 / total) as u8;
    for c in colors {
        *c = c.scale(level);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Layer;
    use crate::engine::{Event, Instant};
    use crate::testutil::*;

    const RED: Rgb = Rgb::new(255, 0, 0);
    const TEAL: Rgb = Rgb::new(0, 200, 200);

    fn press(engine: &mut Engine, config: &Config, i: u8) {
        engine.handle(config, Event::Press(k(i)), Instant(0), &mut |_| {});
    }

    fn release(engine: &mut Engine, config: &Config, i: u8) {
        engine.handle(config, Event::Release(k(i)), Instant(0), &mut |_| {});
    }

    fn idle(c: Color, config: &Config) -> Rgb {
        match c {
            Color::Rgb(rgb) => rgb.scale(config.leds.idle_level),
            _ => Rgb::BLACK,
        }
    }

    #[test]
    fn categories_use_global_palette() {
        let layers = example();
        let c = Config::new(layers.clone());
        let colors = key_colors(&c, &Engine::new(), 0);
        let p = Palette::DEFAULT;
        assert_eq!(colors[M1 as usize], idle(p.layer_key, &c));
        assert_eq!(colors[4], idle(p.key, &c));
    }

    #[test]
    fn unbound_is_off_and_disabled_is_all_off() {
        let mut layers = example();
        layers[0].set(k(4), crate::config::KeyBinding::None);
        let mut c = Config::new(layers.clone());
        assert_eq!(key_colors(&c, &Engine::new(), 0)[4], Rgb::BLACK);
        c.leds.enabled = false;
        assert!(key_colors(&c, &Engine::new(), 0).iter().all(|&x| x == Rgb::BLACK));
    }

    #[test]
    fn layer_palette_is_inherited_by_child_layers() {
        let mut layers = example();
        layers[1].palette.key = Color::Rgb(TEAL);
        let c = Config::new(layers.clone());
        let mut e = Engine::new();
        press(&mut e, &c, M1);
        press(&mut e, &c, M2); // L3, parent L1
        // Key 4 is inherited from L0 but colored by L3's palette chain (L1).
        assert_eq!(key_colors(&c, &e, 0)[4], idle(Color::Rgb(TEAL), &c));
    }

    #[test]
    fn key_color_follows_inherited_binding() {
        let mut layers = example();
        layers[0].key_colors[4].idle = Color::Rgb(RED);
        let c = Config::new(layers.clone());
        let mut e = Engine::new();
        press(&mut e, &c, M1); // L1 inherits key 4 from L0
        assert_eq!(key_colors(&c, &e, 0)[4], idle(Color::Rgb(RED), &c));
    }

    #[test]
    fn held_layer_key_keeps_its_color_on_the_new_layer() {
        let layers = example();
        let c = Config::new(layers.clone());
        let mut e = Engine::new();
        press(&mut e, &c, M1);
        let colors = key_colors(&c, &e, 0);
        // Brighten: full-level layer key color, although m1 is unbound on L1.
        assert_eq!(colors[M1 as usize], Rgb::new(0, 90, 255));
    }

    #[test]
    fn explicit_held_color_and_held_styles() {
        let mut layers = example();
        layers[0].key_colors[4].held = Color::Rgb(RED);
        let mut c = Config::new(layers.clone());
        let mut e = Engine::new();
        press(&mut e, &c, 4);
        press(&mut e, &c, 5);
        assert_eq!(key_colors(&c, &e, 0)[4], RED);
        c.leds.held_style = HeldStyle::White;
        assert_eq!(key_colors(&c, &e, 0)[5], Rgb::WHITE);
    }

    #[test]
    fn released_ancestor_is_dimmed() {
        let layers = example();
        let c = Config::new(layers.clone());
        let mut e = Engine::new();
        press(&mut e, &c, M1);
        press(&mut e, &c, M2);
        release(&mut e, &c, M1);
        let expected = Rgb::new(0, 90, 255).scale(c.leds.idle_level).scale(PATH_ANCHOR_LEVEL);
        assert_eq!(key_colors(&c, &e, 0)[M1 as usize], expected);
    }

    #[test]
    fn indicator_overrides_key_while_lock_is_on() {
        let layers = example();
        let mut c = Config::new(layers.clone());
        c.indicators.push(crate::config::Indicator {
            led: crate::config::HostLed::CapsLock,
            button: k(11),
            color: RED,
        });
        let e = Engine::new();
        assert_ne!(key_colors(&c, &e, 0)[11], RED);
        assert_eq!(key_colors(&c, &e, 0b010)[11], RED); // caps lock bit
        assert_ne!(key_colors(&c, &e, 0b001)[11], RED); // num lock only
    }

    #[test]
    fn gamma_endpoints() {
        assert_eq!(gamma(0), 0);
        assert_eq!(gamma(1), 1);
        assert_eq!(gamma(255), 255);
        assert!(gamma(128) < 70);
    }

    #[test]
    fn current_limit_scales_down() {
        let mut colors = [Rgb::WHITE; KEY_COUNT];
        limit_current(&mut colors, 300);
        let total: u32 = colors.iter().map(|c| (c.r as u32 + c.g as u32 + c.b as u32) * 20 / 255).sum();
        assert!(total <= 300, "{total}");
        let mut dim = [Rgb::new(10, 10, 10); KEY_COUNT];
        limit_current(&mut dim, 300);
        assert_eq!(dim[0], Rgb::new(10, 10, 10));
    }

    #[test]
    fn transparent_layer_constructor_has_no_colors() {
        assert_eq!(Layer::transparent(LayerId(0)).palette, Palette::INHERIT);
    }
}
