//! Configuration data model.
//!
//! A [`Config`] is a list of [`Layer`]s; index 0 is the base layer. Each layer
//! maps every [`Button`] to a [`KeyBinding`] and names a fixed fallthrough
//! parent that [`KeyBinding::Inherit`] resolves through.
//!
//! Order-independent combos need no special type: both orderings simply
//! point at the same target layer.

use alloc::string::String;
use alloc::vec::Vec;

use serde::{Deserialize, Serialize};

use crate::led::{KeyColor, LedSettings, Palette, Rgb};

/// Number of mechanical keys.
pub const KEY_COUNT: usize = 12;
/// Number of buttons: the 12 keys plus the encoder push button.
pub const BUTTON_COUNT: usize = KEY_COUNT + 1;

/// A physical button. Indices 0..=11 are the keys (row-major, top-left
/// first); [`Button::ENCODER`] is the encoder push button.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Button(pub(crate) u8);

impl Button {
    pub const ENCODER: Button = Button(KEY_COUNT as u8);

    /// The key at `index` (0..12), or `None` if out of range.
    pub const fn key(index: u8) -> Option<Button> {
        if (index as usize) < KEY_COUNT {
            Some(Button(index))
        } else {
            None
        }
    }

    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

/// Index of a layer in [`Config::layers`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LayerId(pub u8);

impl LayerId {
    pub const BASE: LayerId = LayerId(0);
}

/// HID keyboard usage ID (e.g. 0x04 = A).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyCode(pub u8);

/// HID modifier bitmask.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Modifiers(pub u8);

impl Modifiers {
    pub const NONE: Modifiers = Modifiers(0);
    pub const LCTRL: Modifiers = Modifiers(0x01);
    pub const LSHIFT: Modifiers = Modifiers(0x02);
    pub const LALT: Modifiers = Modifiers(0x04);
    pub const LGUI: Modifiers = Modifiers(0x08);
    pub const RCTRL: Modifiers = Modifiers(0x10);
    pub const RSHIFT: Modifiers = Modifiers(0x20);
    pub const RALT: Modifiers = Modifiers(0x40);
    pub const RGUI: Modifiers = Modifiers(0x80);
}

/// Something a button does when pressed (and undoes when released).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Action {
    /// A keyboard key, optionally with modifiers held.
    Key { code: KeyCode, mods: Modifiers },
    /// A consumer-control usage (media keys, volume, ...).
    Consumer(u16),
}

/// What a button means on a given layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeyBinding {
    /// Does nothing.
    None,
    /// Use the binding of this layer's fallthrough parent.
    Inherit,
    /// Perform an action.
    Action(Action),
    /// Hold to enter the target layer.
    Layer(LayerId),
    /// Press to switch on, press again to switch off.
    Toggle(Toggle),
    /// Run a macro from [`Config::macros`].
    Macro(MacroId),
    /// A device function (settings menu, profile switching, ...).
    System(SystemAction),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SystemAction {
    /// Open the onboard settings menu.
    Settings,
    NextProfile,
    PrevProfile,
    /// Reboot into the UF2 bootloader.
    Bootloader,
    /// Activate the profile at this index (0-based; `profile:1` is index 0).
    Profile(u8),
}

impl KeyBinding {
    /// Whether fallthrough passes this binding down to child layers. Layer
    /// keys and layer toggles stay on their own layer.
    pub fn is_inheritable(self) -> bool {
        matches!(
            self,
            KeyBinding::Action(_)
                | KeyBinding::Macro(_)
                | KeyBinding::System(_)
                | KeyBinding::Toggle(Toggle::Latch(_) | Toggle::Repeat { .. })
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Toggle {
    /// Holds the action down until switched off.
    Latch(Action),
    /// Taps the action every `interval_ms` until switched off.
    Repeat { action: Action, interval_ms: u16 },
    /// Keeps the layer active as if its key were held.
    Layer(LayerId),
}

/// Index of a macro in [`Config::macros`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MacroId(pub u8);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Macro {
    /// Label on the OLED. Empty means "M<index>".
    pub name: String,
    pub kind: MacroKind,
    pub steps: Vec<Step>,
    /// For toggle macros: what switching off does.
    pub stop: StopMode,
    /// Delay between typed characters; `None` uses [`MacroTiming`].
    pub char_delay_ms: Option<u16>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MacroKind {
    /// Runs once per press. Pressing again while it runs is ignored.
    OneShot,
    /// Runs repeatedly while held; finishes the current run after release.
    WhileHeld,
    /// Press to start repeating indefinitely, press again to stop.
    Toggle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum StopMode {
    /// Finish the current run, then stop.
    FinishRun,
    /// Stop right away (releasing anything the macro holds).
    Immediate,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Step {
    /// Press and keep holding.
    Down(Action),
    /// Release something pressed by an earlier `Down`.
    Up(Action),
    /// Press, then release after [`MacroTiming::tap_ms`].
    Tap(Action),
    Delay { ms: u16 },
    /// Type text using the configured keyboard layout.
    Text(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MacroTiming {
    /// How long taps and typed characters are held down.
    pub tap_ms: u16,
    /// Default pause between typed characters.
    pub char_delay_ms: u16,
}

impl MacroTiming {
    pub const DEFAULT: MacroTiming = MacroTiming { tap_ms: 8, char_delay_ms: 8 };
}

/// What an encoder rotation step means on a given layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EncoderBinding {
    None,
    Inherit,
    /// Tapped once per detent.
    Action(Action),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Direction {
    Clockwise,
    CounterClockwise,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Layer {
    /// Display name (e.g. on the OLED). Empty means "L<index>".
    pub name: String,
    /// Fixed fallthrough parent for [`KeyBinding::Inherit`]. The base layer
    /// has none; `Inherit` with no parent resolves to nothing.
    pub parent: Option<LayerId>,
    pub buttons: [KeyBinding; BUTTON_COUNT],
    pub encoder_cw: EncoderBinding,
    pub encoder_ccw: EncoderBinding,
    /// Per-category LED colors for this layer; `Inherit` entries fall back
    /// to the parent layer's palette, then the global one.
    pub palette: Palette,
    /// Per-key LED colors. They belong to this layer's bindings and follow
    /// them when another layer inherits the binding.
    pub key_colors: [KeyColor; KEY_COUNT],
}

impl Layer {
    /// A layer where everything falls through to `parent`.
    pub const fn transparent(parent: LayerId) -> Self {
        Layer {
            name: String::new(),
            parent: Some(parent),
            buttons: [KeyBinding::Inherit; BUTTON_COUNT],
            encoder_cw: EncoderBinding::Inherit,
            encoder_ccw: EncoderBinding::Inherit,
            palette: Palette::INHERIT,
            key_colors: [KeyColor::INHERIT; KEY_COUNT],
        }
    }

    /// A parentless layer where nothing is bound.
    pub const fn empty() -> Self {
        Layer {
            name: String::new(),
            parent: None,
            buttons: [KeyBinding::None; BUTTON_COUNT],
            encoder_cw: EncoderBinding::None,
            encoder_ccw: EncoderBinding::None,
            palette: Palette::INHERIT,
            key_colors: [KeyColor::INHERIT; KEY_COUNT],
        }
    }

    pub fn set(&mut self, button: Button, binding: KeyBinding) -> &mut Self {
        self.buttons[button.index()] = binding;
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    /// Profile name (shown when switching profiles). Empty means "Profile <n>".
    pub name: String,
    pub layers: Vec<Layer>,
    /// Layer-independent buttons: if set, the action applies on every layer
    /// whose own binding for the button is [`KeyBinding::Inherit`]. Any
    /// explicit binding on the current layer (including `None`) wins.
    pub global: [Option<Action>; BUTTON_COUNT],
    pub leds: LedSettings,
    pub idle: IdleSettings,
    pub macros: Vec<Macro>,
    pub timing: MacroTiming,
    /// Name of the computer's keyboard layout (informational; `char_map`
    /// is what's used).
    pub layout_name: String,
    /// How to type each character used in text steps, sorted by `ch`.
    /// Generated by the config tools from the layout.
    pub char_map: Vec<CharKeys>,
    /// Keys that light up while a host lock (Caps Lock, ...) is on.
    pub indicators: Vec<Indicator>,
}

/// One key press of a typed character.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyStroke {
    pub code: KeyCode,
    pub mods: Modifiers,
}

/// The key presses that type `ch` (several for dead-key sequences).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CharKeys {
    pub ch: char,
    pub keys: Vec<KeyStroke>,
}

/// Lock state LEDs the computer reports to keyboards (bit positions in the
/// HID LED report).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum HostLed {
    NumLock = 0,
    CapsLock = 1,
    ScrollLock = 2,
}

impl HostLed {
    pub fn is_on(self, host_leds: u8) -> bool {
        host_leds & (1 << self as u8) != 0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Indicator {
    pub led: HostLed,
    /// A key (not the encoder button, which has no LED).
    pub button: Button,
    pub color: Rgb,
}

/// Inactivity timeouts for LEDs and OLED, in seconds. 0 disables a stage.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdleSettings {
    pub dim_after_s: u16,
    pub off_after_s: u16,
}

impl IdleSettings {
    pub const DEFAULT: IdleSettings = IdleSettings { dim_after_s: 60, off_after_s: 600 };
}

/// Where an effective binding came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Source {
    /// The binding is defined on this layer.
    Layer(LayerId),
    /// The button's global (layer-independent) action.
    Global,
    /// Nothing is bound anywhere.
    Unbound,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConfigError {
    NoLayers,
    TooManyLayers,
    BaseHasParent,
    UnknownParent { layer: LayerId, parent: LayerId },
    UnknownLayerTarget { layer: LayerId, button: Button, target: LayerId },
    /// Following fallthrough parents from `layer` loops forever.
    ParentCycle { layer: LayerId },
    TooManyMacros,
    UnknownMacro { layer: LayerId, button: Button, target: MacroId },
    /// A text step contains a character missing from `char_map`.
    UntypeableChar { macro_id: MacroId, ch: char },
    /// `char_map` is not sorted by character, has duplicates or empty entries.
    BadCharMap,
    /// An indicator on a button without an LED.
    BadIndicator,
}

impl Config {
    pub fn new(layers: Vec<Layer>) -> Self {
        Config {
            name: String::new(),
            layers,
            global: [None; BUTTON_COUNT],
            leds: LedSettings::DEFAULT,
            idle: IdleSettings::DEFAULT,
            macros: Vec::new(),
            timing: MacroTiming::DEFAULT,
            layout_name: String::new(),
            char_map: Vec::new(),
            indicators: Vec::new(),
        }
    }

    pub fn layer(&self, id: LayerId) -> Option<&Layer> {
        self.layers.get(id.0 as usize)
    }

    pub fn macro_(&self, id: MacroId) -> Option<&Macro> {
        self.macros.get(id.0 as usize)
    }

    /// The key presses that type `ch`, if the char map has it.
    pub fn char_keys(&self, ch: char) -> Option<&[KeyStroke]> {
        self.char_map.binary_search_by_key(&ch, |c| c.ch).ok().map(|i| self.char_map[i].keys.as_slice())
    }

    /// Effective binding of `button` on `layer`. Never returns
    /// [`KeyBinding::Inherit`].
    ///
    /// Order: the layer's own explicit binding, then the global binding,
    /// then fallthrough through the parent chain. Only actions are passed
    /// down by fallthrough; an inherited layer key resolves to `None`.
    pub fn resolve_button(&self, layer: LayerId, button: Button) -> KeyBinding {
        self.resolve_button_source(layer, button).0
    }

    /// Like [`Config::resolve_button`], also reporting where the binding was
    /// defined (used to pick the key's LED color).
    pub fn resolve_button_source(&self, layer: LayerId, button: Button) -> (KeyBinding, Source) {
        const UNBOUND: (KeyBinding, Source) = (KeyBinding::None, Source::Unbound);
        let i = button.index();
        let Some(start) = self.layer(layer) else {
            return UNBOUND;
        };
        match start.buttons[i] {
            KeyBinding::Inherit => {}
            KeyBinding::None => return UNBOUND,
            explicit => return (explicit, Source::Layer(layer)),
        }
        if let Some(action) = self.global[i] {
            return (KeyBinding::Action(action), Source::Global);
        }
        let Some(parent) = start.parent else {
            return UNBOUND;
        };
        self.walk(parent, |id, l| match l.buttons[i] {
            KeyBinding::Inherit => None,
            b if b.is_inheritable() => Some((b, Source::Layer(id))),
            _ => Some(UNBOUND),
        })
        .unwrap_or(UNBOUND)
    }

    /// Effective action for one encoder step on `layer`, after fallthrough.
    pub fn resolve_encoder(&self, layer: LayerId, dir: Direction) -> Option<Action> {
        self.walk(layer, |_, l| {
            let binding = match dir {
                Direction::Clockwise => l.encoder_cw,
                Direction::CounterClockwise => l.encoder_ccw,
            };
            match binding {
                EncoderBinding::Inherit => None,
                EncoderBinding::None => Some(None),
                EncoderBinding::Action(a) => Some(Some(a)),
            }
        })
        .flatten()
    }

    /// Follows the parent chain from `start` until `pick` returns `Some`.
    /// Bounded by the layer count so an invalid (cyclic) config can't hang.
    pub(crate) fn walk<T>(
        &self,
        start: LayerId,
        mut pick: impl FnMut(LayerId, &Layer) -> Option<T>,
    ) -> Option<T> {
        let mut current = Some(start);
        for _ in 0..self.layers.len() {
            let id = current?;
            let layer = self.layer(id)?;
            if let Some(found) = pick(id, layer) {
                return Some(found);
            }
            current = layer.parent;
        }
        None
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.layers.is_empty() {
            return Err(ConfigError::NoLayers);
        }
        if self.layers.len() > u8::MAX as usize + 1 {
            return Err(ConfigError::TooManyLayers);
        }
        if self.layers[0].parent.is_some() {
            return Err(ConfigError::BaseHasParent);
        }
        if self.macros.len() > u8::MAX as usize + 1 {
            return Err(ConfigError::TooManyMacros);
        }
        if self.indicators.iter().any(|ind| ind.button.index() >= KEY_COUNT) {
            return Err(ConfigError::BadIndicator);
        }
        if self.char_map.windows(2).any(|w| w[0].ch >= w[1].ch) || self.char_map.iter().any(|c| c.keys.is_empty()) {
            return Err(ConfigError::BadCharMap);
        }
        for (i, m) in self.macros.iter().enumerate() {
            for step in &m.steps {
                if let Step::Text(text) = step
                    && let Some(ch) = text.chars().find(|&c| self.char_keys(c).is_none())
                {
                    return Err(ConfigError::UntypeableChar { macro_id: MacroId(i as u8), ch });
                }
            }
        }
        for (i, layer) in self.layers.iter().enumerate() {
            let id = LayerId(i as u8);
            if let Some(parent) = layer.parent
                && self.layer(parent).is_none()
            {
                return Err(ConfigError::UnknownParent { layer: id, parent });
            }
            for (b, binding) in layer.buttons.iter().enumerate() {
                let button = Button(b as u8);
                match *binding {
                    KeyBinding::Layer(target) | KeyBinding::Toggle(Toggle::Layer(target))
                        if self.layer(target).is_none() =>
                    {
                        return Err(ConfigError::UnknownLayerTarget { layer: id, button, target });
                    }
                    KeyBinding::Macro(target) if self.macro_(target).is_none() => {
                        return Err(ConfigError::UnknownMacro { layer: id, button, target });
                    }
                    _ => {}
                }
            }
        }
        for (i, layer) in self.layers.iter().enumerate() {
            let mut current = layer.parent;
            let mut steps = 0;
            while let Some(parent) = current {
                steps += 1;
                if steps > self.layers.len() {
                    return Err(ConfigError::ParentCycle { layer: LayerId(i as u8) });
                }
                current = self.layers[parent.0 as usize].parent;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::*;

    #[test]
    fn example_is_valid() {
        let layers = example();
        assert_eq!(Config::new(layers.clone()).validate(), Ok(()));
    }

    #[test]
    fn layer_keys_change_meaning_per_layer() {
        let layers = example();
        let c = Config::new(layers.clone());
        assert_eq!(c.resolve_button(LayerId(0), k(M2)), KeyBinding::Layer(LayerId(2)));
        assert_eq!(c.resolve_button(LayerId(1), k(M2)), KeyBinding::Layer(LayerId(3)));
        assert_eq!(c.resolve_button(LayerId(2), k(M1)), KeyBinding::Layer(LayerId(4)));
    }

    #[test]
    fn fallthrough_follows_fixed_parent_chain() {
        let layers = example();
        let c = Config::new(layers.clone());
        // L3 -> L1 overrides key 5.
        assert_eq!(c.resolve_button(LayerId(3), k(5)), key(0x3A));
        // L4 -> L2 -> L0 for key 5.
        assert_eq!(c.resolve_button(LayerId(4), k(5)), key(0x09));
        // Encoder button is unbound everywhere.
        assert_eq!(c.resolve_button(LayerId(3), Button::ENCODER), KeyBinding::None);
    }

    #[test]
    fn layer_keys_are_not_inherited() {
        let layers = example();
        let c = Config::new(layers.clone());
        // L1 and L3 inherit the m1 key, but L0's "enter L1" is not passed down.
        assert_eq!(c.resolve_button(LayerId(1), k(M1)), KeyBinding::None);
        assert_eq!(c.resolve_button(LayerId(3), k(M1)), KeyBinding::None);
    }

    #[test]
    fn global_applies_where_layer_inherits() {
        let mut layers = example();
        layers[1].set(k(11), KeyBinding::None);
        let mut c = Config::new(layers.clone());
        let mute = Action::Consumer(0xE2);
        c.global[k(11).index()] = Some(mute);
        // L0 binds key 11 explicitly: its own binding wins.
        assert_eq!(c.resolve_button(LayerId(0), k(11)), key(0x04 + 11));
        // L2 inherits: global beats the inherited L0 binding.
        assert_eq!(c.resolve_button(LayerId(2), k(11)), KeyBinding::Action(mute));
        // L1 explicitly disables it.
        assert_eq!(c.resolve_button(LayerId(1), k(11)), KeyBinding::None);
        // L3 inherits: global applies before falling through to L1.
        assert_eq!(c.resolve_button(LayerId(3), k(11)), KeyBinding::Action(mute));
    }

    #[test]
    fn rejects_unknown_target() {
        let mut layers = example();
        layers[1].set(k(7), KeyBinding::Layer(LayerId(9)));
        assert_eq!(
            Config::new(layers.clone()).validate(),
            Err(ConfigError::UnknownLayerTarget { layer: LayerId(1), button: k(7), target: LayerId(9) })
        );
    }

    #[test]
    fn rejects_parent_cycle_but_resolve_still_terminates() {
        let mut layers = example();
        layers[1].parent = Some(LayerId(3)); // L1 -> L3 -> L1
        let c = Config::new(layers.clone());
        assert!(matches!(c.validate(), Err(ConfigError::ParentCycle { .. })));
        assert_eq!(c.resolve_button(LayerId(3), k(8)), KeyBinding::None);
    }
}
