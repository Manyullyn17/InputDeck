//! The runtime state machine.
//!
//! [`Engine`] consumes input [`Event`]s and periodic [`Engine::tick`]s and
//! reports what happened through [`Output`]s passed to a caller-supplied
//! closure.
//!
//! Layer state is the path of layer keys used to reach the current layer,
//! each entry marked held or released. Pressing a layer key pushes it;
//! releasing one marks it released and then settles the stack: released
//! entries are popped from the top until an entry is held. A released entry
//! whose physical key is held again (and doing nothing else) is revived
//! instead of popped. The current layer is the top entry, or the base layer.
//! See `meta-layers.md`.
//!
//! Toggles and macros are described in `actions.md`. An active toggle owns
//! its physical key: the next press of that key switches it off, whatever
//! the key means on the current layer. A layer toggle is a path entry that
//! counts as held until switched off.

use alloc::vec::Vec;
use core::mem::replace;

use crate::config::*;

/// Milliseconds since boot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Instant(pub u64);

impl Instant {
    pub const fn after_ms(self, ms: u16) -> Instant {
        Instant(self.0 + ms as u64)
    }
}

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
    /// Press a key for typing text: while it is down, exactly `mods` are
    /// reported, regardless of other held modifiers.
    TypePress { code: KeyCode, mods: Modifiers },
    /// Release a key pressed by `TypePress`.
    TypeRelease { code: KeyCode, mods: Modifiers },
    /// The current layer changed.
    LayerChanged(LayerId),
    /// A system binding was pressed; the firmware carries it out.
    System(SystemAction),
}

/// What a held button is doing, fixed at the moment it was pressed. The
/// `LayerId` is the layer it was pressed on (used for its LED color).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Held {
    Up,
    /// Bound to nothing; can revive a released layer key (see `settle`).
    Nothing,
    /// The press did its work already (toggle, one-shot macro); release
    /// does nothing.
    Consumed(LayerId),
    Action(Action, LayerId),
    Layer(LayerId),
    /// Holding a while-held macro.
    MacroHold(MacroId, LayerId),
}

#[derive(Clone, Copy, Debug)]
struct Entry {
    button: Button,
    /// Layer entered by this key.
    layer: LayerId,
    /// Layer the key was pressed on.
    from: LayerId,
    held: bool,
    /// Held by a layer toggle rather than the physical key.
    toggled: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Active {
    Latch(Action),
    Repeat { action: Action, interval_ms: u16, down: bool, next: Instant },
    Layer,
    Macro(MacroId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ActiveToggle {
    kind: Active,
    pressed_on: LayerId,
}

/// A macro in progress.
#[derive(Clone, Debug)]
struct Runner {
    id: MacroId,
    button: Button,
    pressed_on: LayerId,
    step: usize,
    /// Within a tap or typed character: whether the key is currently down.
    key_down: bool,
    /// Byte offset into the current text step.
    text_pos: usize,
    /// Index of the key press within the current character's sequence.
    stroke: usize,
    next: Instant,
    /// While-held macros: whether the key is still held.
    holding: bool,
    /// Toggle macros: switched off, finishing the current run.
    stopping: bool,
    /// Actions pressed by this macro and not yet released.
    down: Vec<Action>,
    typing: Option<(KeyCode, Modifiers)>,
}

/// What a key's LED should reflect.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyLight {
    /// Not pressed: show its binding on the current layer.
    Idle,
    /// Physically held: show the binding it was pressed with.
    Held { pressed_on: LayerId },
    /// A toggle owned by this key is on.
    Active { pressed_on: LayerId },
    /// A macro started by this key is still running.
    Running { pressed_on: LayerId },
    /// Released layer key still in the layer path (falling back goes
    /// through it): shown dimmed.
    PathAnchor { pressed_on: LayerId },
}

/// Maximum path length. Held entries never exceed [`BUTTON_COUNT`]; the rest
/// is room for released ancestors. Only cyclic configs can fill it, in
/// which case the oldest released entry is dropped.
pub const MAX_DEPTH: usize = 32;
/// Maximum number of macros running at once.
pub const MAX_RUNNERS: usize = 8;
/// Maximum macro steps executed per runner per tick, so a macro without
/// delays can't stall the device.
const STEP_BUDGET: usize = 64;

#[derive(Clone, Debug)]
pub struct Engine {
    stack: [Entry; MAX_DEPTH],
    depth: usize,
    held: [Held; BUTTON_COUNT],
    toggles: [Option<ActiveToggle>; BUTTON_COUNT],
    runners: Vec<Runner>,
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

impl Engine {
    pub const fn new() -> Self {
        Engine {
            stack: [Entry { button: Button(0), layer: LayerId::BASE, from: LayerId::BASE, held: false, toggled: false };
                MAX_DEPTH],
            depth: 0,
            held: [Held::Up; BUTTON_COUNT],
            toggles: [None; BUTTON_COUNT],
            runners: Vec::new(),
        }
    }

    /// The current layer.
    pub fn layer(&self) -> LayerId {
        match self.depth {
            0 => LayerId::BASE,
            n => self.stack[n - 1].layer,
        }
    }

    /// Path of layers leading to the current one, bottom to top. Includes
    /// layers whose keys were released but that are still fallen back to.
    pub fn layer_path(&self) -> impl Iterator<Item = LayerId> + '_ {
        self.stack[..self.depth].iter().map(|e| e.layer)
    }

    pub fn is_held(&self, button: Button) -> bool {
        self.held[button.index()] != Held::Up
    }

    /// Whether time-based work is pending (macros, repeat toggles), i.e.
    /// whether [`Engine::tick`] needs to keep being called.
    pub fn is_busy(&self) -> bool {
        !self.runners.is_empty()
            || self.toggles.iter().flatten().any(|t| matches!(t.kind, Active::Repeat { .. }))
    }

    pub fn is_running(&self, id: MacroId) -> bool {
        self.runners.iter().any(|r| r.id == id)
    }

    pub fn key_light(&self, button: Button) -> KeyLight {
        let i = button.index();
        if let Some(t) = self.toggles[i] {
            return KeyLight::Active { pressed_on: t.pressed_on };
        }
        match self.held[i] {
            Held::Action(_, from) | Held::Layer(from) | Held::Consumed(from) | Held::MacroHold(_, from) => {
                return KeyLight::Held { pressed_on: from };
            }
            Held::Nothing => return KeyLight::Held { pressed_on: self.layer() },
            Held::Up => {}
        }
        if let Some(r) = self.runners.iter().find(|r| r.button == button) {
            return KeyLight::Running { pressed_on: r.pressed_on };
        }
        self.stack[..self.depth]
            .iter()
            .rev()
            .find(|e| !e.held && e.button == button)
            .map_or(KeyLight::Idle, |e| KeyLight::PathAnchor { pressed_on: e.from })
    }

    pub fn handle(&mut self, config: &Config, event: Event, now: Instant, out: &mut impl FnMut(Output)) {
        let before = self.layer();
        match event {
            Event::Press(button) => self.press(config, button, now, out),
            Event::Release(button) => match replace(&mut self.held[button.index()], Held::Up) {
                Held::Action(a, _) => out(Output::Release(a)),
                Held::Layer(_) => {
                    self.mark_released(button);
                    self.settle();
                }
                Held::MacroHold(id, _) => {
                    if let Some(r) = self.runners.iter_mut().find(|r| r.id == id) {
                        r.holding = false;
                    }
                }
                Held::Nothing | Held::Consumed(_) | Held::Up => {}
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

    fn press(&mut self, config: &Config, button: Button, now: Instant, out: &mut impl FnMut(Output)) {
        let i = button.index();
        if self.held[i] != Held::Up {
            return; // duplicate press without release; ignore
        }
        let layer = self.layer();
        if let Some(t) = self.toggles[i].take() {
            self.held[i] = Held::Consumed(layer);
            self.toggle_off(config, button, t.kind, out);
            return;
        }
        let toggle = |kind| Some(ActiveToggle { kind, pressed_on: layer });
        self.held[i] = match config.resolve_button(layer, button) {
            KeyBinding::Action(a) => {
                out(Output::Press(a));
                Held::Action(a, layer)
            }
            KeyBinding::Layer(target) => {
                self.push(button, target, layer, false);
                Held::Layer(layer)
            }
            KeyBinding::Toggle(Toggle::Latch(a)) => {
                out(Output::Press(a));
                self.toggles[i] = toggle(Active::Latch(a));
                Held::Consumed(layer)
            }
            KeyBinding::Toggle(Toggle::Repeat { action, interval_ms }) => {
                out(Output::Press(action));
                let next = now.after_ms(half(interval_ms));
                self.toggles[i] = toggle(Active::Repeat { action, interval_ms, down: true, next });
                Held::Consumed(layer)
            }
            KeyBinding::Toggle(Toggle::Layer(target)) => {
                self.push(button, target, layer, true);
                self.toggles[i] = toggle(Active::Layer);
                Held::Consumed(layer)
            }
            KeyBinding::Macro(id) => self.press_macro(config, id, button, layer, now, out),
            KeyBinding::System(action) => {
                out(Output::System(action));
                Held::Consumed(layer)
            }
            KeyBinding::None | KeyBinding::Inherit => Held::Nothing,
        };
    }

    fn press_macro(
        &mut self,
        config: &Config,
        id: MacroId,
        button: Button,
        layer: LayerId,
        now: Instant,
        out: &mut impl FnMut(Output),
    ) -> Held {
        let Some(m) = config.macro_(id) else {
            return Held::Nothing;
        };
        if let Some(r) = self.runners.iter_mut().find(|r| r.id == id) {
            // Already running.
            return match m.kind {
                MacroKind::OneShot => Held::Consumed(layer), // ignored for now
                MacroKind::WhileHeld => {
                    r.holding = true;
                    r.button = button;
                    Held::MacroHold(id, layer)
                }
                MacroKind::Toggle => {
                    // Switched back on while finishing its last run: resume.
                    r.stopping = false;
                    r.button = button;
                    self.toggles[button.index()] =
                        Some(ActiveToggle { kind: Active::Macro(id), pressed_on: layer });
                    Held::Consumed(layer)
                }
            };
        }
        if self.runners.len() >= MAX_RUNNERS {
            return Held::Consumed(layer);
        }
        self.runners.push(Runner {
            id,
            button,
            pressed_on: layer,
            step: 0,
            key_down: false,
            text_pos: 0,
            stroke: 0,
            next: now,
            holding: m.kind == MacroKind::WhileHeld,
            stopping: false,
            down: Vec::new(),
            typing: None,
        });
        let held = match m.kind {
            MacroKind::OneShot => Held::Consumed(layer),
            MacroKind::WhileHeld => Held::MacroHold(id, layer),
            MacroKind::Toggle => {
                self.toggles[button.index()] = Some(ActiveToggle { kind: Active::Macro(id), pressed_on: layer });
                Held::Consumed(layer)
            }
        };
        // Start right away so the first step isn't delayed until the next tick.
        let last = self.runners.len() - 1;
        if advance(&mut self.runners[last], config, now, out) {
            let r = self.runners.remove(last);
            release_all(r, out);
        }
        held
    }

    fn toggle_off(&mut self, config: &Config, button: Button, kind: Active, out: &mut impl FnMut(Output)) {
        match kind {
            Active::Latch(a) => out(Output::Release(a)),
            Active::Repeat { action, down, .. } => {
                if down {
                    out(Output::Release(action));
                }
            }
            Active::Layer => {
                if let Some(e) = self.stack[..self.depth].iter_mut().rev().find(|e| e.toggled && e.button == button) {
                    e.toggled = false;
                    e.held = false;
                }
                self.settle();
            }
            Active::Macro(id) => {
                let Some(pos) = self.runners.iter().position(|r| r.id == id) else {
                    return;
                };
                let stop = config.macro_(id).map_or(StopMode::Immediate, |m| m.stop);
                match stop {
                    StopMode::FinishRun => self.runners[pos].stopping = true,
                    StopMode::Immediate => release_all(self.runners.remove(pos), out),
                }
            }
        }
    }

    /// Advances time-based behavior (macros, repeat toggles). Call
    /// regularly, e.g. every millisecond. Returns whether a macro finished
    /// (so displays can refresh).
    pub fn tick(&mut self, config: &Config, now: Instant, out: &mut impl FnMut(Output)) -> bool {
        for toggle in self.toggles.iter_mut().flatten() {
            if let Active::Repeat { action, interval_ms, down, next } = &mut toggle.kind {
                // Catch up at most one edge per tick; repeats are not bursts.
                if *next <= now {
                    *down = !*down;
                    out(if *down { Output::Press(*action) } else { Output::Release(*action) });
                    *next = now.after_ms(half(*interval_ms));
                }
            }
        }
        let mut finished = false;
        let mut i = 0;
        while i < self.runners.len() {
            if advance(&mut self.runners[i], config, now, out) {
                release_all(self.runners.remove(i), out);
                finished = true;
            } else {
                i += 1;
            }
        }
        finished
    }

    /// Releases everything (held actions, toggles, macros) and returns to
    /// the base layer, e.g. for a config reload or stuck-key recovery.
    pub fn reset(&mut self, out: &mut impl FnMut(Output)) {
        let before = self.layer();
        for held in &mut self.held {
            if let Held::Action(a, _) = replace(held, Held::Up) {
                out(Output::Release(a));
            }
        }
        for t in self.toggles.iter_mut().filter_map(Option::take) {
            match t.kind {
                Active::Latch(a) | Active::Repeat { action: a, down: true, .. } => out(Output::Release(a)),
                _ => {}
            }
        }
        for r in self.runners.drain(..) {
            release_all(r, out);
        }
        self.depth = 0;
        if before != LayerId::BASE {
            out(Output::LayerChanged(LayerId::BASE));
        }
    }

    fn push(&mut self, button: Button, layer: LayerId, from: LayerId, toggled: bool) {
        if self.depth == MAX_DEPTH {
            // At most BUTTON_COUNT entries are held, so a released one exists.
            if let Some(pos) = self.stack.iter().position(|e| !e.held) {
                self.stack.copy_within(pos + 1.., pos);
                self.depth -= 1;
            }
        }
        self.stack[self.depth] = Entry { button, layer, from, held: true, toggled };
        self.depth += 1;
    }

    fn mark_released(&mut self, button: Button) {
        if let Some(entry) = self.stack[..self.depth]
            .iter_mut()
            .rev()
            .find(|e| e.held && !e.toggled && e.button == button)
        {
            entry.held = false;
        }
    }

    /// Pops released entries off the top. A released entry whose key is
    /// physically held and idle is revived as a held layer key instead.
    fn settle(&mut self) {
        while self.depth > 0 {
            let top = &mut self.stack[self.depth - 1];
            if top.held {
                return;
            }
            let held = &mut self.held[top.button.index()];
            if *held == Held::Nothing {
                *held = Held::Layer(top.from);
                top.held = true;
                return;
            }
            self.depth -= 1;
        }
    }
}

fn half(interval_ms: u16) -> u16 {
    (interval_ms / 2).max(1)
}

/// Releases everything a stopped macro still holds.
fn release_all(r: Runner, out: &mut impl FnMut(Output)) {
    if let Some((code, mods)) = r.typing {
        out(Output::TypeRelease { code, mods });
    }
    for a in r.down {
        out(Output::Release(a));
    }
}

/// Runs a macro's due steps. Returns `true` when the macro is done.
fn advance(r: &mut Runner, config: &Config, now: Instant, out: &mut impl FnMut(Output)) -> bool {
    let Some(m) = config.macro_(r.id) else {
        return true;
    };
    let tap = config.timing.tap_ms;
    let char_delay = m.char_delay_ms.unwrap_or(config.timing.char_delay_ms);
    for _ in 0..STEP_BUDGET {
        if r.next > now {
            return false;
        }
        let Some(step) = m.steps.get(r.step) else {
            // End of a run.
            let again = match m.kind {
                MacroKind::OneShot => false,
                MacroKind::WhileHeld => r.holding,
                MacroKind::Toggle => !r.stopping,
            };
            if !again {
                return true;
            }
            r.step = 0;
            // At least 1 ms per run, so a macro without delays can't spin.
            r.next = now.after_ms(1);
            continue;
        };
        match step {
            Step::Down(a) => {
                out(Output::Press(*a));
                r.down.push(*a);
                r.step += 1;
            }
            Step::Up(a) => {
                if let Some(pos) = r.down.iter().position(|d| d == a) {
                    r.down.swap_remove(pos);
                    out(Output::Release(*a));
                }
                r.step += 1;
            }
            Step::Tap(a) => {
                if r.key_down {
                    if let Some(pos) = r.down.iter().position(|d| d == a) {
                        r.down.swap_remove(pos);
                    }
                    out(Output::Release(*a));
                    r.key_down = false;
                    r.step += 1;
                } else {
                    out(Output::Press(*a));
                    r.down.push(*a);
                    r.key_down = true;
                    r.next = now.after_ms(tap);
                }
            }
            Step::Delay { ms } => {
                r.next = now.after_ms(*ms);
                r.step += 1;
            }
            Step::Text(text) => {
                let current = text[r.text_pos..].chars().next();
                let keys = current.and_then(|c| config.char_keys(c)).unwrap_or(&[]);
                if let Some((code, mods)) = r.typing.take() {
                    out(Output::TypeRelease { code, mods });
                    r.stroke += 1;
                    if r.stroke < keys.len() {
                        // Next key of a sequence (e.g. after a dead key).
                        r.next = now.after_ms(tap);
                    } else {
                        r.stroke = 0;
                        r.text_pos += current.map_or(1, char::len_utf8);
                        r.next = now.after_ms(char_delay);
                    }
                } else if let Some(c) = current {
                    match keys.get(r.stroke) {
                        Some(&KeyStroke { code, mods }) => {
                            out(Output::TypePress { code, mods });
                            r.typing = Some((code, mods));
                            r.next = now.after_ms(tap);
                        }
                        None => {
                            // Missing from the char map (rejected by validation); skip.
                            r.stroke = 0;
                            r.text_pos += c.len_utf8();
                        }
                    }
                } else {
                    r.text_pos = 0;
                    r.step += 1;
                }
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::*;

    struct Harness {
        config: Config,
        engine: Engine,
        now: u64,
    }

    impl Harness {
        fn new(layers: &[Layer]) -> Self {
            Self::with_config(Config::new(layers.to_vec()))
        }

        fn with_config(config: Config) -> Self {
            config.validate().unwrap();
            Harness { config, engine: Engine::new(), now: 0 }
        }

        fn send(&mut self, event: Event) -> Vec<Output> {
            let mut outs = Vec::new();
            self.engine.handle(&self.config, event, Instant(self.now), &mut |o| outs.push(o));
            outs
        }

        /// Advances time to `t` ms, ticking every millisecond.
        fn run_until(&mut self, t: u64) -> Vec<Output> {
            let mut outs = Vec::new();
            while self.now < t {
                self.now += 1;
                self.engine.tick(&self.config, Instant(self.now), &mut |o| outs.push(o));
            }
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
    fn released_ancestors_stay_in_path() {
        let layers = example();
        let mut h = Harness::new(&layers);
        h.press(M1);
        h.press(M2);
        h.release(M1);
        assert_eq!(h.engine.layer_path().collect::<Vec<_>>(), [LayerId(1), LayerId(3)]);
    }

    #[test]
    fn repressed_ancestor_is_fallen_back_to() {
        let layers = example();
        let mut h = Harness::new(&layers);
        h.press(M1);
        h.press(M2);
        h.release(M1);
        h.press(M1); // idle on L3
        assert_eq!(h.release(M2), [LayerChanged(LayerId(1))]);
        assert!(h.engine.is_held(k(M1)));
        assert_eq!(h.release(M1), [LayerChanged(LayerId(0))]);
    }

    #[test]
    fn repressed_key_with_an_action_does_not_revive() {
        let mut layers = example();
        layers[3].set(k(M1), key(0x1B)); // m1 types X on L3
        let mut h = Harness::new(&layers);
        h.press(M1);
        h.press(M2);
        h.release(M1);
        assert_eq!(h.press(M1), [Press(act(0x1B))]);
        assert_eq!(h.release(M2), [LayerChanged(LayerId(0))]);
        assert_eq!(h.release(M1), [Release(act(0x1B))]);
    }

    #[test]
    fn cyclic_config_is_bounded() {
        let mut layers = example();
        layers[3].set(k(M1), KeyBinding::Layer(LayerId(1))); // L1 -> L3 -> L1 -> ...
        let mut h = Harness::new(&layers);
        h.press(M1);
        for _ in 0..100 {
            h.press(M2); // -> L3
            h.release(M1);
            h.press(M1); // -> L1
            h.release(M2);
        }
        assert_eq!(h.layer(), 1);
        assert!(h.engine.layer_path().count() <= MAX_DEPTH);
        h.release(M1);
        assert_eq!(h.layer(), 0);
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

    // ---- Toggles ----

    use Output::{TypePress, TypeRelease};

    fn with_binding(button: u8, binding: KeyBinding) -> Harness {
        let mut layers = example();
        layers[0].set(k(button), binding);
        Harness::new(&layers)
    }

    #[test]
    fn latch_toggles_on_and_off() {
        let mut h = with_binding(4, KeyBinding::Toggle(Toggle::Latch(act(0x08))));
        assert_eq!(h.press(4), [Press(act(0x08))]);
        assert_eq!(h.release(4), []);
        assert_eq!(h.engine.key_light(k(4)), KeyLight::Active { pressed_on: LayerId(0) });
        assert_eq!(h.press(4), [Release(act(0x08))]);
        assert_eq!(h.release(4), []);
        assert_eq!(h.engine.key_light(k(4)), KeyLight::Idle);
    }

    #[test]
    fn active_toggle_owns_its_key_on_other_layers() {
        let mut layers = example();
        layers[0].set(k(5), KeyBinding::Toggle(Toggle::Latch(act(0x08))));
        layers[1].set(k(5), key(F1)); // key 5 means F1 on L1
        let mut h = Harness::new(&layers);
        h.press(5);
        h.release(5);
        h.press(M1);
        assert_eq!(h.press(5), [Release(act(0x08))]); // switches off, no F1
        h.release(5);
        assert_eq!(h.press(5), [Press(act(F1))]);
    }

    #[test]
    fn repeat_taps_at_interval() {
        let a = act(0x04);
        let mut h = with_binding(4, KeyBinding::Toggle(Toggle::Repeat { action: a, interval_ms: 10 }));
        assert_eq!(h.press(4), [Press(a)]);
        h.release(4);
        assert_eq!(h.run_until(5), [Release(a)]);
        assert_eq!(h.run_until(10), [Press(a)]);
        assert_eq!(h.run_until(30), [Release(a), Press(a), Release(a), Press(a)]);
        assert_eq!(h.press(4), [Release(a)]);
        assert_eq!(h.run_until(60), []);
    }

    #[test]
    fn layer_toggle_acts_like_held_layer_key() {
        let mut h = with_binding(4, KeyBinding::Toggle(Toggle::Layer(LayerId(1))));
        assert_eq!(h.press(4), [LayerChanged(LayerId(1))]);
        assert_eq!(h.release(4), []);
        assert_eq!(h.layer(), 1);
        // Layer keys on the toggled layer work as usual.
        h.press(M2);
        assert_eq!(h.layer(), 3);
        // Switching off while L3 is held keeps L3; releasing m2 then drops to L0.
        assert_eq!(h.press(4), []);
        assert_eq!(h.layer(), 3);
        h.release(4);
        assert_eq!(h.release(M2), [LayerChanged(LayerId(0))]);
    }

    #[test]
    fn layer_toggle_off_returns_to_base() {
        let mut h = with_binding(4, KeyBinding::Toggle(Toggle::Layer(LayerId(2))));
        h.press(4);
        h.release(4);
        assert_eq!(h.press(4), [LayerChanged(LayerId(0))]);
    }

    // ---- Macros ----

    fn macro_config(kind: MacroKind, stop: StopMode, steps: Vec<Step>) -> Harness {
        let mut layers = example();
        layers[0].set(k(4), KeyBinding::Macro(MacroId(0)));
        let mut config = Config::new(layers);
        config.char_map = char_map();
        config.macros.push(Macro { name: "m".into(), kind, steps, stop, char_delay_ms: None });
        config.timing = MacroTiming { tap_ms: 5, char_delay_ms: 5 };
        Harness::with_config(config)
    }

    fn tap_b_then_wait() -> Vec<Step> {
        vec![Step::Tap(act(0x05)), Step::Delay { ms: 20 }]
    }

    #[test]
    fn one_shot_runs_once_and_ignores_repress() {
        let b = act(0x05);
        let mut h = macro_config(MacroKind::OneShot, StopMode::FinishRun, tap_b_then_wait());
        assert_eq!(h.press(4), [Press(b)]);
        h.release(4);
        assert_eq!(h.engine.key_light(k(4)), KeyLight::Running { pressed_on: LayerId(0) });
        assert_eq!(h.run_until(5), [Release(b)]);
        assert_eq!(h.press(4), []); // still running (delay): ignored
        h.release(4);
        assert_eq!(h.run_until(100), []);
        assert_eq!(h.engine.key_light(k(4)), KeyLight::Idle);
        assert_eq!(h.press(4), [Press(b)]); // runs again
    }

    #[test]
    fn while_held_repeats_and_finishes_after_release() {
        let b = act(0x05);
        let mut h = macro_config(MacroKind::WhileHeld, StopMode::FinishRun, tap_b_then_wait());
        h.press(4);
        // Runs start at 0, 26 and 52 ms (5 ms tap + 20 ms delay + 1 ms gap).
        let outs = h.run_until(54);
        assert_eq!(outs.iter().filter(|o| **o == Press(b)).count(), 2);
        h.release(4);
        let rest = h.run_until(200);
        assert_eq!(rest, [Release(b)]); // the run in progress finishes, no new one
        assert!(!h.engine.is_running(MacroId(0)));
    }

    #[test]
    fn toggle_macro_finish_run() {
        let b = act(0x05);
        let mut h = macro_config(MacroKind::Toggle, StopMode::FinishRun, tap_b_then_wait());
        h.press(4);
        h.release(4);
        h.run_until(30);
        assert_eq!(h.engine.key_light(k(4)), KeyLight::Active { pressed_on: LayerId(0) });
        assert_eq!(h.press(4), []); // switching off doesn't cut the run short
        assert_eq!(h.engine.key_light(k(4)), KeyLight::Held { pressed_on: LayerId(0) });
        h.release(4);
        assert_eq!(h.engine.key_light(k(4)), KeyLight::Running { pressed_on: LayerId(0) });
        assert_eq!(h.run_until(200), [Release(b)]);
        assert!(!h.engine.is_running(MacroId(0)));
    }

    #[test]
    fn toggle_macro_immediate_stop_releases_held_keys() {
        let shift = act(0xE1);
        let steps = vec![Step::Down(shift), Step::Delay { ms: 1000 }, Step::Up(shift)];
        let mut h = macro_config(MacroKind::Toggle, StopMode::Immediate, steps);
        assert_eq!(h.press(4), [Press(shift)]);
        h.release(4);
        h.run_until(10);
        assert_eq!(h.press(4), [Release(shift)]);
        assert!(!h.engine.is_running(MacroId(0)));
    }

    #[test]
    fn text_types_with_exact_modifiers() {
        let mut h = macro_config(MacroKind::OneShot, StopMode::FinishRun, vec![Step::Text("aB".into())]);
        let mut outs = h.press(4);
        outs.extend(h.run_until(100));
        let shift = Modifiers::LSHIFT;
        assert_eq!(
            outs,
            [
                TypePress { code: KeyCode(0x04), mods: Modifiers::NONE },
                TypeRelease { code: KeyCode(0x04), mods: Modifiers::NONE },
                TypePress { code: KeyCode(0x05), mods: shift },
                TypeRelease { code: KeyCode(0x05), mods: shift },
            ]
        );
    }

    #[test]
    fn dead_key_sequences_are_typed_in_order() {
        let mut h = macro_config(MacroKind::OneShot, StopMode::FinishRun, vec![Step::Text("^a".into())]);
        let mut outs = h.press(4);
        outs.extend(h.run_until(100));
        let none = Modifiers::NONE;
        assert_eq!(
            outs,
            [
                TypePress { code: KeyCode(0x35), mods: none },
                TypeRelease { code: KeyCode(0x35), mods: none },
                TypePress { code: KeyCode(0x2C), mods: none },
                TypeRelease { code: KeyCode(0x2C), mods: none },
                TypePress { code: KeyCode(0x04), mods: none },
                TypeRelease { code: KeyCode(0x04), mods: none },
            ]
        );
    }

    #[test]
    fn text_needs_char_map_entries() {
        let mut config = Config::new(example());
        config.macros.push(Macro {
            name: String::new(),
            kind: MacroKind::OneShot,
            steps: vec![Step::Text("x".into())],
            stop: StopMode::FinishRun,
            char_delay_ms: None,
        });
        assert_eq!(config.validate(), Err(ConfigError::UntypeableChar { macro_id: MacroId(0), ch: 'x' }));
    }

    #[test]
    fn macro_without_delays_cannot_spin() {
        let mut h = macro_config(MacroKind::Toggle, StopMode::Immediate, vec![Step::Down(act(0x05)), Step::Up(act(0x05))]);
        h.press(4);
        let outs = h.run_until(10);
        // At most one run per millisecond.
        assert!(outs.len() <= 2 * 11, "{}", outs.len());
    }

    #[test]
    fn macros_run_concurrently() {
        let mut layers = example();
        layers[0].set(k(4), KeyBinding::Macro(MacroId(0)));
        layers[0].set(k(5), KeyBinding::Macro(MacroId(1)));
        let mut config = Config::new(layers);
        for code in [0x05, 0x06] {
            config.macros.push(Macro {
                name: "m".into(),
                kind: MacroKind::OneShot,
                steps: vec![Step::Delay { ms: 10 }, Step::Tap(act(code))],
                stop: StopMode::FinishRun,
                char_delay_ms: None,
            });
        }
        let mut h = Harness::with_config(config);
        h.press(4);
        h.press(5);
        let outs = h.run_until(11);
        assert!(outs.contains(&Press(act(0x05))) && outs.contains(&Press(act(0x06))));
    }

    #[test]
    fn system_bindings_are_reported_once() {
        let mut h = with_binding(4, KeyBinding::System(SystemAction::Settings));
        assert_eq!(h.press(4), [Output::System(SystemAction::Settings)]);
        assert_eq!(h.release(4), []);
    }

    #[test]
    fn reset_stops_toggles_and_macros() {
        let shift = act(0xE1);
        let steps = vec![Step::Down(shift), Step::Delay { ms: 1000 }];
        let mut h = macro_config(MacroKind::OneShot, StopMode::FinishRun, steps);
        h.config.layers[0].set(k(5), KeyBinding::Toggle(Toggle::Latch(act(0x08))));
        h.press(4);
        h.press(5);
        let mut outs = Vec::new();
        h.engine.reset(&mut |o| outs.push(o));
        assert!(outs.contains(&Release(shift)) && outs.contains(&Release(act(0x08))));
        assert!(!h.engine.is_running(MacroId(0)));
        assert_eq!(h.engine.key_light(k(5)), KeyLight::Idle);
    }
}
