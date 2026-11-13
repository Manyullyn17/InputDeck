//! Hard-coded demo config until config storage exists.
//!
//! Layout (keys numbered 1-12, top-left first):
//! - L0: key 1 = m1 (-> L1), key 2 = m2 (-> L2), keys 3-12 type a-j,
//!   encoder = volume up/down, encoder press = mute.
//! - L1: key 2 = m3 (-> L3); key 3 types "1"; encoder = right/left arrow.
//! - L2: key 1 = m4 (-> L4); key 3 types "2".
//! - L3 (parent L1): key 3 types "3".
//! - L4 (parent L2): key 3 types "4".
//!
//! Everything else falls through to the parent layer.
//!
//! LEDs: global default palette (layer keys blue, keys amber, media green,
//! unbound off). L1 and its child L3 color keys teal; L2 and L4 pink.
//! Key 12 has its own colors on L0 (red, yellow while held), which follow
//! it onto every layer that inherits it.

use macropad_engine::led::{Color, Rgb};
use macropad_engine::{Action, Button, EncoderBinding, KeyBinding, KeyCode, Layer, LayerId, Modifiers};

const TEAL: Color = Color::Rgb(Rgb::new(0, 255, 180));
const PINK: Color = Color::Rgb(Rgb::new(255, 40, 140));

const A: u8 = 0x04;
const DIGIT_1: u8 = 0x1E;
const RIGHT: u8 = 0x4F;
const LEFT: u8 = 0x50;
const VOLUME_UP: u16 = 0xE9;
const VOLUME_DOWN: u16 = 0xEA;
const MUTE: u16 = 0xE2;

fn key(n: u8) -> Button {
    Button::key(n - 1).unwrap()
}

fn tap(code: u8) -> Action {
    Action::Key { code: KeyCode(code), mods: Modifiers::NONE }
}

fn digit(n: u8) -> KeyBinding {
    KeyBinding::Action(tap(DIGIT_1 + n - 1))
}

pub fn layers() -> [Layer; 5] {
    let mut l0 = Layer::empty();
    l0.set(key(1), KeyBinding::Layer(LayerId(1)))
        .set(key(2), KeyBinding::Layer(LayerId(2)));
    for n in 3..=12 {
        l0.set(key(n), KeyBinding::Action(tap(A + n - 3)));
    }
    l0.encoder_cw = EncoderBinding::Action(Action::Consumer(VOLUME_UP));
    l0.encoder_ccw = EncoderBinding::Action(Action::Consumer(VOLUME_DOWN));
    l0.set(Button::ENCODER, KeyBinding::Action(Action::Consumer(MUTE)));
    l0.key_colors[key(12).index()].idle = Color::Rgb(Rgb::new(255, 0, 0));
    l0.key_colors[key(12).index()].held = Color::Rgb(Rgb::new(255, 220, 0));

    let mut l1 = Layer::transparent(LayerId(0));
    l1.set(key(2), KeyBinding::Layer(LayerId(3))).set(key(3), digit(1));
    l1.encoder_cw = EncoderBinding::Action(tap(RIGHT));
    l1.encoder_ccw = EncoderBinding::Action(tap(LEFT));
    l1.palette.key = TEAL;

    let mut l2 = Layer::transparent(LayerId(0));
    l2.set(key(1), KeyBinding::Layer(LayerId(4))).set(key(3), digit(2));
    l2.palette.key = PINK;

    let mut l3 = Layer::transparent(LayerId(1));
    l3.set(key(3), digit(3));

    let mut l4 = Layer::transparent(LayerId(2));
    l4.set(key(3), digit(4));

    [l0, l1, l2, l3, l4]
}
