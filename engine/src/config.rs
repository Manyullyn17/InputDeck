//! Configuration data model.
//!
//! A [`Config`] is a list of [`Layer`]s; index 0 is the base layer. Each layer
//! maps every [`Button`] to a [`KeyBinding`] and names a fixed fallthrough
//! parent that [`KeyBinding::Inherit`] resolves through.
//!
//! Order-independent combos need no special type: both orderings simply
//! point at the same target layer.

/// Number of mechanical keys.
pub const KEY_COUNT: usize = 12;
/// Number of buttons: the 12 keys plus the encoder push button.
pub const BUTTON_COUNT: usize = KEY_COUNT + 1;

/// A physical button. Indices 0..=11 are the keys (row-major, top-left
/// first); [`Button::ENCODER`] is the encoder push button.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Button(u8);

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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LayerId(pub u8);

impl LayerId {
    pub const BASE: LayerId = LayerId(0);
}

/// HID keyboard usage ID (e.g. 0x04 = A).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyCode(pub u8);

/// HID modifier bitmask.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// A keyboard key, optionally with modifiers held.
    Key { code: KeyCode, mods: Modifiers },
    /// A consumer-control usage (media keys, volume, ...).
    Consumer(u16),
}

/// What a button means on a given layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyBinding {
    /// Does nothing.
    None,
    /// Use the binding of this layer's fallthrough parent.
    Inherit,
    /// Perform an action.
    Action(Action),
    /// Hold to enter the target layer.
    Layer(LayerId),
}

/// What an encoder rotation step means on a given layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EncoderBinding {
    None,
    Inherit,
    /// Tapped once per detent.
    Action(Action),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Clockwise,
    CounterClockwise,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Layer {
    /// Fixed fallthrough parent for [`KeyBinding::Inherit`]. The base layer
    /// has none; `Inherit` with no parent resolves to nothing.
    pub parent: Option<LayerId>,
    pub buttons: [KeyBinding; BUTTON_COUNT],
    pub encoder_cw: EncoderBinding,
    pub encoder_ccw: EncoderBinding,
}

impl Layer {
    /// A layer where everything falls through to `parent`.
    pub const fn transparent(parent: LayerId) -> Self {
        Layer {
            parent: Some(parent),
            buttons: [KeyBinding::Inherit; BUTTON_COUNT],
            encoder_cw: EncoderBinding::Inherit,
            encoder_ccw: EncoderBinding::Inherit,
        }
    }

    /// A parentless layer where nothing is bound.
    pub const fn empty() -> Self {
        Layer {
            parent: None,
            buttons: [KeyBinding::None; BUTTON_COUNT],
            encoder_cw: EncoderBinding::None,
            encoder_ccw: EncoderBinding::None,
        }
    }

    pub fn set(&mut self, button: Button, binding: KeyBinding) -> &mut Self {
        self.buttons[button.index()] = binding;
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config<'a> {
    pub layers: &'a [Layer],
    /// Layer-independent buttons: if set, the action applies on every layer
    /// whose own binding for the button is [`KeyBinding::Inherit`]. Any
    /// explicit binding on the current layer (including `None`) wins.
    pub global: [Option<Action>; BUTTON_COUNT],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfigError {
    NoLayers,
    TooManyLayers,
    BaseHasParent,
    UnknownParent { layer: LayerId, parent: LayerId },
    UnknownLayerTarget { layer: LayerId, button: Button, target: LayerId },
    /// Following fallthrough parents from `layer` loops forever.
    ParentCycle { layer: LayerId },
}

impl<'a> Config<'a> {
    pub const fn new(layers: &'a [Layer]) -> Self {
        Config { layers, global: [None; BUTTON_COUNT] }
    }

    pub fn layer(&self, id: LayerId) -> Option<&Layer> {
        self.layers.get(id.0 as usize)
    }

    /// Effective binding of `button` on `layer`. Never returns
    /// [`KeyBinding::Inherit`].
    ///
    /// Order: the layer's own explicit binding, then the global binding,
    /// then fallthrough through the parent chain. Only actions are passed
    /// down by fallthrough; an inherited layer key resolves to `None`.
    pub fn resolve_button(&self, layer: LayerId, button: Button) -> KeyBinding {
        let i = button.index();
        let Some(start) = self.layer(layer) else {
            return KeyBinding::None;
        };
        if start.buttons[i] != KeyBinding::Inherit {
            return start.buttons[i];
        }
        if let Some(action) = self.global[i] {
            return KeyBinding::Action(action);
        }
        let Some(parent) = start.parent else {
            return KeyBinding::None;
        };
        self.walk(parent, |l| match l.buttons[i] {
            KeyBinding::Inherit => None,
            KeyBinding::Action(a) => Some(KeyBinding::Action(a)),
            KeyBinding::None | KeyBinding::Layer(_) => Some(KeyBinding::None),
        })
        .unwrap_or(KeyBinding::None)
    }

    /// Effective action for one encoder step on `layer`, after fallthrough.
    pub fn resolve_encoder(&self, layer: LayerId, dir: Direction) -> Option<Action> {
        self.walk(layer, |l| {
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
    fn walk<T>(&self, start: LayerId, mut pick: impl FnMut(&Layer) -> Option<T>) -> Option<T> {
        let mut current = Some(start);
        for _ in 0..self.layers.len() {
            let layer = self.layer(current?)?;
            if let Some(found) = pick(layer) {
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
        for (i, layer) in self.layers.iter().enumerate() {
            let id = LayerId(i as u8);
            if let Some(parent) = layer.parent
                && self.layer(parent).is_none()
            {
                return Err(ConfigError::UnknownParent { layer: id, parent });
            }
            for (b, binding) in layer.buttons.iter().enumerate() {
                if let KeyBinding::Layer(target) = *binding
                    && self.layer(target).is_none()
                {
                    return Err(ConfigError::UnknownLayerTarget {
                        layer: id,
                        button: Button(b as u8),
                        target,
                    });
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

    fn k(i: u8) -> Button {
        Button::key(i).unwrap()
    }

    fn key(code: u8) -> KeyBinding {
        KeyBinding::Action(Action::Key { code: KeyCode(code), mods: Modifiers::NONE })
    }

    const M1: u8 = 0;
    const M2: u8 = 1;

    /// The example from the design discussion: m1 -> L1, m2 -> L2,
    /// on L1 the m2 key becomes m3 -> L3, on L2 the m1 key becomes m4 -> L4.
    fn example() -> Vec<Layer> {
        let mut base = Layer::empty();
        base.set(k(M1), KeyBinding::Layer(LayerId(1)))
            .set(k(M2), KeyBinding::Layer(LayerId(2)));
        for i in 2..12 {
            base.set(k(i), key(0x04 + i)); // letters
        }

        let mut l1 = Layer::transparent(LayerId(0));
        l1.set(k(M2), KeyBinding::Layer(LayerId(3))).set(k(5), key(0x3A)); // F1

        let mut l2 = Layer::transparent(LayerId(0));
        l2.set(k(M1), KeyBinding::Layer(LayerId(4)));

        let l3 = Layer::transparent(LayerId(1));
        let l4 = Layer::transparent(LayerId(2));

        vec![base, l1, l2, l3, l4]
    }

    #[test]
    fn example_is_valid() {
        let layers = example();
        assert_eq!(Config::new(&layers).validate(), Ok(()));
    }

    #[test]
    fn layer_keys_change_meaning_per_layer() {
        let layers = example();
        let c = Config::new(&layers);
        assert_eq!(c.resolve_button(LayerId(0), k(M2)), KeyBinding::Layer(LayerId(2)));
        assert_eq!(c.resolve_button(LayerId(1), k(M2)), KeyBinding::Layer(LayerId(3)));
        assert_eq!(c.resolve_button(LayerId(2), k(M1)), KeyBinding::Layer(LayerId(4)));
    }

    #[test]
    fn fallthrough_follows_fixed_parent_chain() {
        let layers = example();
        let c = Config::new(&layers);
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
        let c = Config::new(&layers);
        // L1 and L3 inherit the m1 key, but L0's "enter L1" is not passed down.
        assert_eq!(c.resolve_button(LayerId(1), k(M1)), KeyBinding::None);
        assert_eq!(c.resolve_button(LayerId(3), k(M1)), KeyBinding::None);
    }

    #[test]
    fn global_applies_where_layer_inherits() {
        let mut layers = example();
        layers[1].set(k(11), KeyBinding::None);
        let mut c = Config::new(&layers);
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
            Config::new(&layers).validate(),
            Err(ConfigError::UnknownLayerTarget { layer: LayerId(1), button: k(7), target: LayerId(9) })
        );
    }

    #[test]
    fn rejects_parent_cycle_but_resolve_still_terminates() {
        let mut layers = example();
        layers[1].parent = Some(LayerId(3)); // L1 -> L3 -> L1
        let c = Config::new(&layers);
        assert!(matches!(c.validate(), Err(ConfigError::ParentCycle { .. })));
        assert_eq!(c.resolve_button(LayerId(3), k(8)), KeyBinding::None);
    }
}
