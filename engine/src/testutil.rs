//! Shared test fixtures.

use crate::config::*;

pub fn k(i: u8) -> Button {
    Button::key(i).unwrap()
}

pub fn act(code: u8) -> Action {
    Action::Key { code: KeyCode(code), mods: Modifiers::NONE }
}

pub fn key(code: u8) -> KeyBinding {
    KeyBinding::Action(act(code))
}

pub const M1: u8 = 0;
pub const M2: u8 = 1;
pub const F1: u8 = 0x3A;

/// The example from the design discussion: m1 -> L1, m2 -> L2,
/// on L1 the m2 key becomes m3 -> L3, on L2 the m1 key becomes m4 -> L4.
/// Keys 2..12 on L0 send letters (key i sends HID usage 0x04 + i);
/// L1 overrides key 5 with F1.
pub fn example() -> Vec<Layer> {
    let mut base = Layer::empty();
    base.set(k(M1), KeyBinding::Layer(LayerId(1)))
        .set(k(M2), KeyBinding::Layer(LayerId(2)));
    for i in 2..12 {
        base.set(k(i), key(0x04 + i));
    }

    let mut l1 = Layer::transparent(LayerId(0));
    l1.set(k(M2), KeyBinding::Layer(LayerId(3))).set(k(5), key(F1));

    let mut l2 = Layer::transparent(LayerId(0));
    l2.set(k(M1), KeyBinding::Layer(LayerId(4)));

    let l3 = Layer::transparent(LayerId(1));
    let l4 = Layer::transparent(LayerId(2));

    vec![base, l1, l2, l3, l4]
}

/// A small char map: a, B (shift+b), and ^ as a dead key followed by space.
pub fn char_map() -> Vec<CharKeys> {
    let stroke = |code, mods| KeyStroke { code: KeyCode(code), mods: Modifiers(mods) };
    vec![
        CharKeys { ch: 'B', keys: vec![stroke(0x05, Modifiers::LSHIFT.0)] },
        CharKeys { ch: '^', keys: vec![stroke(0x35, 0), stroke(0x2C, 0)] },
        CharKeys { ch: 'a', keys: vec![stroke(0x04, 0)] },
    ]
}
