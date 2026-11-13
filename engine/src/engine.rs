//! The runtime state machine.
//!
//! [`Engine`] consumes input [`Event`]s one at a time and reports what
//! happened through [`Output`]s. It owns no hardware and no allocation;
//! outputs are passed to a caller-supplied closure.
//!
//! State is a stack of currently held layer keys. Pressing a layer key
//! pushes it, releasing it removes it from wherever it sits, and the current
//! layer is the top entry (or the base layer when empty). This is
//! equivalent to the rule in `meta-layers.md` and bounds the stack by the
//! number of buttons.

use core::mem::replace;

use crate::config::*;

/// Milliseconds since boot. Unused in V1; reserved for tap/hold timing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Instant(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    Press(Button),
    Release(Button),
    /// One encoder detent.
    Rotate(Direction),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Output {
    /// Start sending an action.
    Press(Action),
    /// Stop sending an action. Always matches an earlier `Press`.
    Release(Action),
    /// The current layer changed.
    LayerChanged(LayerId),
}

/// What a held button is doing, fixed at the moment it was pressed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Held {
    Up,
    Nothing,
    Action(Action),
    Layer,
}

#[derive(Clone, Copy, Debug)]
struct Entry {
    button: Button,
    layer: LayerId,
}

#[derive(Clone, Debug)]
pub struct Engine {
    stack: [Entry; BUTTON_COUNT],
    depth: usize,
    held: [Held; BUTTON_COUNT],
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

impl Engine {
    pub const fn new() -> Self {
        Engine {
            stack: [Entry { button: Button(0), layer: LayerId::BASE }; BUTTON_COUNT],
            depth: 0,
            held: [Held::Up; BUTTON_COUNT],
        }
    }

    /// The current layer.
    pub fn layer(&self) -> LayerId {
        match self.depth {
            0 => LayerId::BASE,
            n => self.stack[n - 1].layer,
        }
    }

    /// Layers entered by currently held layer keys, bottom to top.
    pub fn layer_path(&self) -> impl Iterator<Item = LayerId> + '_ {
        self.stack[..self.depth].iter().map(|e| e.layer)
    }

    pub fn is_held(&self, button: Button) -> bool {
        self.held[button.index()] != Held::Up
    }

    pub fn handle(
        &mut self,
        config: &Config,
        event: Event,
        _now: Instant,
        out: &mut impl FnMut(Output),
    ) {
        let before = self.layer();
        match event {
            Event::Press(button) => {
                let i = button.index();
                if self.held[i] != Held::Up {
                    return; // duplicate press without release; ignore
                }
                self.held[i] = match config.resolve_button(before, button) {
                    KeyBinding::Action(a) => {
                        out(Output::Press(a));
                        Held::Action(a)
                    }
                    KeyBinding::Layer(target) => {
                        self.push(button, target);
                        Held::Layer
                    }
                    KeyBinding::None | KeyBinding::Inherit => Held::Nothing,
                };
            }
            Event::Release(button) => match replace(&mut self.held[button.index()], Held::Up) {
                Held::Action(a) => out(Output::Release(a)),
                Held::Layer => self.remove(button),
                Held::Nothing | Held::Up => {}
            },
            Event::Rotate(dir) => {
                if let Some(a) = config.resolve_encoder(before, dir) {
                    out(Output::Press(a));
                    out(Output::Release(a));
                }
            }
        }
        if self.layer() != before {
            out(Output::LayerChanged(self.layer()));
        }
    }

    /// Releases every held action and returns to the base layer, e.g. for a
    /// config reload or stuck-key recovery.
    pub fn reset(&mut self, out: &mut impl FnMut(Output)) {
        let before = self.layer();
        for held in &mut self.held {
            if let Held::Action(a) = replace(held, Held::Up) {
                out(Output::Release(a));
            }
        }
        self.depth = 0;
        if before != LayerId::BASE {
            out(Output::LayerChanged(LayerId::BASE));
        }
    }

    fn push(&mut self, button: Button, layer: LayerId) {
        // Each held button has at most one entry, so this cannot overflow.
        debug_assert!(self.depth < BUTTON_COUNT);
        self.stack[self.depth] = Entry { button, layer };
        self.depth += 1;
    }

    fn remove(&mut self, button: Button) {
        if let Some(pos) = self.stack[..self.depth].iter().rposition(|e| e.button == button) {
            self.stack.copy_within(pos + 1..self.depth, pos);
            self.depth -= 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::*;

    struct Harness<'a> {
        config: Config<'a>,
        engine: Engine,
    }

    impl<'a> Harness<'a> {
        fn new(layers: &'a [Layer]) -> Self {
            let config = Config::new(layers);
            config.validate().unwrap();
            Harness { config, engine: Engine::new() }
        }

        fn send(&mut self, event: Event) -> Vec<Output> {
            let mut outs = Vec::new();
            self.engine.handle(&self.config, event, Instant(0), &mut |o| outs.push(o));
            outs
        }

        fn press(&mut self, i: u8) -> Vec<Output> {
            self.send(Event::Press(k(i)))
        }

        fn release(&mut self, i: u8) -> Vec<Output> {
            self.send(Event::Release(k(i)))
        }

        fn layer(&self) -> u8 {
            self.engine.layer().0
        }
    }

    use Output::{LayerChanged, Press, Release};

    #[test]
    fn release_top_returns_to_previous_layer() {
        let layers = example();
        let mut h = Harness::new(&layers);
        assert_eq!(h.press(M1), [LayerChanged(LayerId(1))]);
        assert_eq!(h.press(M2), [LayerChanged(LayerId(3))]);
        assert_eq!(h.release(M2), [LayerChanged(LayerId(1))]);
        assert_eq!(h.release(M1), [LayerChanged(LayerId(0))]);
    }

    #[test]
    fn release_ancestor_stays_on_current_layer() {
        let layers = example();
        let mut h = Harness::new(&layers);
        h.press(M1);
        h.press(M2);
        assert_eq!(h.release(M1), []);
        assert_eq!(h.layer(), 3);
        assert_eq!(h.release(M2), [LayerChanged(LayerId(0))]);
    }

    #[test]
    fn order_matters() {
        let layers = example();
        let mut h = Harness::new(&layers);
        h.press(M2);
        h.press(M1);
        assert_eq!(h.layer(), 4);
    }

    #[test]
    fn release_middle_of_deep_stack() {
        let mut layers = example();
        layers[3].set(k(2), KeyBinding::Layer(LayerId(5)));
        layers.push(Layer::transparent(LayerId(3)));
        let mut h = Harness::new(&layers);
        h.press(M1);
        h.press(M2);
        h.press(2);
        assert_eq!(h.layer(), 5);
        assert_eq!(h.release(M2), []);
        assert_eq!(h.layer(), 5);
        assert_eq!(h.release(2), [LayerChanged(LayerId(1))]);
    }

    #[test]
    fn repressing_released_layer_key_does_nothing() {
        let layers = example();
        let mut h = Harness::new(&layers);
        h.press(M1);
        h.press(M2);
        h.release(M1);
        assert_eq!(h.press(M1), []);
        assert_eq!(h.layer(), 3);
        assert_eq!(h.release(M1), []);
        assert_eq!(h.layer(), 3);
    }

    #[test]
    fn held_action_survives_layer_switch() {
        let layers = example();
        let mut h = Harness::new(&layers);
        assert_eq!(h.press(5), [Press(act(0x09))]);
        h.press(M1); // key 5 is F1 on L1
        assert_eq!(h.release(5), [Release(act(0x09))]);
        assert_eq!(h.press(5), [Press(act(F1))]);
        h.release(M1); // still holding F1 after leaving L1
        assert_eq!(h.release(5), [Release(act(F1))]);
    }

    #[test]
    fn encoder_taps_action() {
        let mut layers = example();
        let vol_up = Action::Consumer(0xE9);
        layers[0].encoder_cw = EncoderBinding::Action(vol_up);
        let mut h = Harness::new(&layers);
        h.press(M1); // L1 inherits the encoder
        assert_eq!(
            h.send(Event::Rotate(Direction::Clockwise)),
            [Press(vol_up), Release(vol_up)]
        );
        assert_eq!(h.send(Event::Rotate(Direction::CounterClockwise)), []);
    }

    #[test]
    fn duplicate_press_is_ignored() {
        let layers = example();
        let mut h = Harness::new(&layers);
        h.press(5);
        assert_eq!(h.press(5), []);
        assert_eq!(h.release(5), [Release(act(0x09))]);
        assert_eq!(h.release(5), []);
    }

    #[test]
    fn reset_releases_everything() {
        let layers = example();
        let mut h = Harness::new(&layers);
        h.press(4);
        h.press(M1);
        h.press(5);
        let mut outs = Vec::new();
        h.engine.reset(&mut |o| outs.push(o));
        assert_eq!(
            outs,
            [Release(act(0x08)), Release(act(F1)), LayerChanged(LayerId(0))]
        );
        assert_eq!(h.layer(), 0);
        assert!(!h.engine.is_held(k(M1)));
    }
}
