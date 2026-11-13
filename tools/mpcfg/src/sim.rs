//! Runs the real engine on the desktop, for previewing a config (used by
//! the desktop app's simulator).

use std::collections::VecDeque;

use macropad_engine::led;
use macropad_engine::notation::write_action;
use macropad_engine::{
    Action, BUTTON_COUNT, Button, Config, Direction, Engine, Event, Instant, KEY_COUNT, KeyLight, Output,
    SystemAction, label,
};
use serde::Serialize;

use crate::document::hex;

const LOG_LEN: usize = 40;

pub struct Simulator {
    config: Config,
    engine: Engine,
    now: u64,
    log: VecDeque<String>,
}

#[derive(Serialize, Debug, Clone)]
pub struct SimState {
    /// Layer names from the bottom of the path to the current layer.
    pub path: Vec<String>,
    /// The 12 keys, then the encoder press.
    pub keys: Vec<SimKey>,
    /// Most recent last.
    pub log: Vec<String>,
    /// Whether macros or repeats are running (keep calling `tick`).
    pub busy: bool,
}

#[derive(Serialize, Debug, Clone)]
pub struct SimKey {
    pub label: String,
    pub held: bool,
    /// Highlighted as on the OLED: held, toggled on, or running a macro.
    pub highlighted: bool,
    /// LED color as resolved by the device (before brightness), "#rrggbb".
    pub color: String,
}

fn button(index: usize) -> Button {
    if index < KEY_COUNT { Button::key(index as u8).unwrap() } else { Button::ENCODER }
}

impl Simulator {
    pub fn new(config: Config) -> Self {
        Simulator { config, engine: Engine::new(), now: 0, log: VecDeque::new() }
    }

    /// Replaces the config (e.g. after an edit), releasing everything.
    pub fn set_config(&mut self, config: Config) {
        self.engine.reset(&mut |_| {});
        self.config = config;
    }

    fn record(&mut self, output: Output) {
        let text = match output {
            Output::Press(a) => format!("▼ {}", action(a)),
            Output::Release(a) => format!("▲ {}", action(a)),
            Output::TypePress { code, mods } => format!("▼ {}", action(Action::Key { code, mods })),
            Output::TypeRelease { code, mods } => format!("▲ {}", action(Action::Key { code, mods })),
            Output::LayerChanged(id) => {
                let mut name = String::new();
                let _ = label::write_layer(&mut name, &self.config, id);
                format!("→ layer {name}")
            }
            Output::System(a) => format!(
                "⚙ {}",
                match a {
                    SystemAction::Settings => "settings menu",
                    SystemAction::NextProfile => "next profile",
                    SystemAction::PrevProfile => "previous profile",
                    SystemAction::Bootloader => "bootloader",
                }
            ),
        };
        if self.log.len() == LOG_LEN {
            self.log.pop_front();
        }
        self.log.push_back(text);
    }

    fn event(&mut self, event: Event) {
        let mut outs = Vec::new();
        self.engine.handle(&self.config, event, Instant(self.now), &mut |o| outs.push(o));
        for o in outs {
            self.record(o);
        }
    }

    /// `index`: key 0-11, or 12 for the encoder button.
    pub fn press(&mut self, index: usize) {
        if index < BUTTON_COUNT {
            self.event(Event::Press(button(index)));
        }
    }

    pub fn release(&mut self, index: usize) {
        if index < BUTTON_COUNT {
            self.event(Event::Release(button(index)));
        }
    }

    pub fn rotate(&mut self, clockwise: bool) {
        self.event(Event::Rotate(if clockwise { Direction::Clockwise } else { Direction::CounterClockwise }));
    }

    /// Advances simulated time by `ms`, millisecond by millisecond.
    pub fn tick(&mut self, ms: u64) {
        for _ in 0..ms {
            self.now += 1;
            let mut outs = Vec::new();
            self.engine.tick(&self.config, Instant(self.now), &mut |o| outs.push(o));
            for o in outs {
                self.record(o);
            }
        }
    }

    pub fn clear_log(&mut self) {
        self.log.clear();
    }

    pub fn state(&self) -> SimState {
        let colors = led::key_colors(&self.config, &self.engine, 0);
        let layer = self.engine.layer();
        let keys = (0..BUTTON_COUNT)
            .map(|i| {
                let b = button(i);
                let light = self.engine.key_light(b);
                let mut text = String::new();
                let _ = label::write_binding(&mut text, &self.config, self.config.resolve_button(layer, b));
                SimKey {
                    label: text,
                    held: self.engine.is_held(b),
                    highlighted: matches!(
                        light,
                        KeyLight::Held { .. } | KeyLight::Active { .. } | KeyLight::Running { .. }
                    ),
                    color: colors.get(i).map_or_else(|| "#000000".to_string(), |&c| hex(c)),
                }
            })
            .collect();
        let mut path: Vec<String> = self
            .engine
            .layer_path()
            .map(|id| {
                let mut name = String::new();
                let _ = label::write_layer(&mut name, &self.config, id);
                name
            })
            .collect();
        if path.is_empty() {
            let mut name = String::new();
            let _ = label::write_layer(&mut name, &self.config, layer);
            path.push(name);
        }
        SimState { path, keys, log: self.log.iter().cloned().collect(), busy: self.engine.is_busy() }
    }
}

fn action(a: Action) -> String {
    let mut s = String::new();
    let _ = write_action(&mut s, a);
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::toml_config::parse;

    const DEMO: &str = include_str!("../../../configs/demo.toml");

    #[test]
    fn meta_layers_in_the_simulator() {
        let mut sim = Simulator::new(parse(DEMO).unwrap());
        assert_eq!(sim.state().path, ["Base"]);
        sim.press(0); // m1
        sim.press(1); // m3 on One
        let s = sim.state();
        assert_eq!(s.path, ["One", "Three"]);
        assert_eq!(s.keys[2].label, "3");
        assert!(s.keys[0].held && s.keys[0].highlighted);
        sim.release(0);
        assert_eq!(sim.state().path, ["One", "Three"]);
        sim.release(1);
        assert_eq!(sim.state().path, ["Base"]);
        assert!(sim.state().log.iter().any(|l| l == "→ layer Three"));
    }

    #[test]
    fn macros_run_with_simulated_time() {
        let mut sim = Simulator::new(parse(DEMO).unwrap());
        sim.press(12); // knob: toggle Tools
        sim.release(12);
        sim.clear_log();
        sim.press(0); // Hello macro
        sim.release(0);
        assert!(sim.state().busy);
        sim.tick(2000);
        let s = sim.state();
        assert!(!s.busy);
        // The log keeps the most recent entries: "...OS!" then Enter.
        assert_eq!(s.log.len(), LOG_LEN);
        assert_eq!(s.log.last().map(String::as_str), Some("▲ enter"));
        assert!(s.log.iter().any(|l| l == "▼ shift+o"), "{:?}", s.log);
    }
}
