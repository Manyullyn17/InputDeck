//! Onboard settings menu.
//!
//! A pure state machine: the firmware feeds it [`MenuInput`]s and draws
//! [`Menu::view`]. Keys act as soft buttons laid out on the 4x3 grid; the
//! knob moves the selection (or adjusts a value while editing) and its
//! button selects/confirms.
//!
//! ```text
//! Bright  LEDs    IdleDim
//! IdleOff Held    Profile
//! Info    -       -
//! Cancel  -       Save
//! ```

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::config::{Config, Direction, IdleSettings, KEY_COUNT};
use crate::led::{HeldStyle, LedSettings, Rgb};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuInput {
    /// A key (0..12) was pressed.
    Key(usize),
    Rotate(Direction),
    /// The knob button was pressed.
    Select,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuOutcome {
    Stay,
    /// Leave without changing anything.
    Cancel,
    /// Leave and store the changes.
    Save,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Item {
    Brightness,
    Leds,
    IdleDim,
    IdleOff,
    HeldStyle,
    Profile,
    Info,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Cell {
    Item(Item),
    Cancel,
    Save,
}

const GRID: [Option<Cell>; KEY_COUNT] = [
    Some(Cell::Item(Item::Brightness)),
    Some(Cell::Item(Item::Leds)),
    Some(Cell::Item(Item::IdleDim)),
    Some(Cell::Item(Item::IdleOff)),
    Some(Cell::Item(Item::HeldStyle)),
    Some(Cell::Item(Item::Profile)),
    Some(Cell::Item(Item::Info)),
    None,
    None,
    Some(Cell::Cancel),
    None,
    Some(Cell::Save),
];

pub const CANCEL_KEY: usize = 9;
pub const SAVE_KEY: usize = 11;

const LABELS: [&str; KEY_COUNT] =
    ["Bright", "LEDs", "IdleDim", "IdleOff", "Held", "Profile", "Info", "", "", "Cancel", "", "Save"];

const IDLE_DIM_STEPS: [u16; 8] = [0, 10, 30, 60, 120, 300, 600, 1800];
const IDLE_OFF_STEPS: [u16; 7] = [0, 60, 300, 600, 1800, 3600, 7200];

#[derive(Clone, Debug)]
enum Mode {
    Main { selected: usize },
    /// Editing `item`; `before` restores everything on cancel.
    Edit { item: Item, before: (LedSettings, IdleSettings, u8) },
    Info,
}

pub enum MenuView<'a> {
    Grid { labels: [&'static str; KEY_COUNT], selected: usize, profile: String },
    Edit { title: &'static str, value: String },
    Info { lines: &'a [String] },
}

pub struct Menu {
    pub leds: LedSettings,
    pub idle: IdleSettings,
    /// Selected profile index.
    pub profile: u8,
    profile_names: Vec<String>,
    info: Vec<String>,
    mode: Mode,
}

/// Brightness and similar levels are shown as whole percent; these match
/// the conversion used by config files.
pub fn percent_to_level(p: u8) -> u8 {
    (p.min(100) as u16 * 255 / 100) as u8
}

pub fn level_to_percent(v: u8) -> u8 {
    (0..=100u8).find(|&p| percent_to_level(p) == v).unwrap_or(((v as u16 * 100 + 127) / 255) as u8)
}

fn step_through(steps: &[u16], current: u16, delta: i32) -> u16 {
    let i = steps.iter().position(|&s| s >= current).unwrap_or(steps.len() - 1) as i32;
    steps[(i + delta).clamp(0, steps.len() as i32 - 1) as usize]
}

fn duration(s: u16) -> String {
    match s {
        0 => "Off".into(),
        s if s % 3600 == 0 => format!("{} h", s / 3600),
        s if s % 60 == 0 => format!("{} min", s / 60),
        s => format!("{s} s"),
    }
}

impl Menu {
    /// `profile_names` lists all stored profiles; `profile` is the active one.
    pub fn new(config: &Config, profile: u8, profile_names: Vec<String>, info: Vec<String>) -> Self {
        Menu { leds: config.leds, idle: config.idle, profile, profile_names, info, mode: Mode::Main { selected: 0 } }
    }

    /// Writes the edited settings into `config`.
    pub fn apply(&self, config: &mut Config) {
        config.leds = self.leds;
        config.idle = self.idle;
    }

    pub fn profile_name(&self, i: u8) -> String {
        match self.profile_names.get(i as usize) {
            Some(n) if !n.is_empty() => n.clone(),
            _ => format!("Profile {}", i + 1),
        }
    }

    pub fn handle(&mut self, input: MenuInput) -> MenuOutcome {
        match self.mode.clone() {
            Mode::Main { selected } => match input {
                MenuInput::Rotate(dir) => {
                    self.mode = Mode::Main { selected: self.next_cell(selected, dir) };
                    MenuOutcome::Stay
                }
                MenuInput::Select => self.activate(selected),
                MenuInput::Key(i) if GRID.get(i).is_some_and(Option::is_some) => self.activate(i),
                MenuInput::Key(_) => MenuOutcome::Stay,
            },
            Mode::Edit { item, before } => {
                let cell = Self::cell_of(item);
                match input {
                    MenuInput::Rotate(dir) => {
                        self.adjust(item, if dir == Direction::Clockwise { 1 } else { -1 });
                        MenuOutcome::Stay
                    }
                    MenuInput::Select => {
                        self.mode = Mode::Main { selected: cell };
                        MenuOutcome::Stay
                    }
                    MenuInput::Key(CANCEL_KEY) => {
                        (self.leds, self.idle, self.profile) = before;
                        self.mode = Mode::Main { selected: cell };
                        MenuOutcome::Stay
                    }
                    MenuInput::Key(i) => {
                        // Confirm, then act on the key as in the main view.
                        self.mode = Mode::Main { selected: cell };
                        self.handle(MenuInput::Key(i))
                    }
                }
            }
            Mode::Info => {
                self.mode = Mode::Main { selected: Self::cell_of(Item::Info) };
                MenuOutcome::Stay
            }
        }
    }

    fn cell_of(item: Item) -> usize {
        GRID.iter().position(|c| *c == Some(Cell::Item(item))).unwrap()
    }

    fn next_cell(&self, from: usize, dir: Direction) -> usize {
        let n = GRID.len();
        let mut i = from;
        loop {
            i = match dir {
                Direction::Clockwise => (i + 1) % n,
                Direction::CounterClockwise => (i + n - 1) % n,
            };
            if GRID[i].is_some() {
                return i;
            }
        }
    }

    fn activate(&mut self, cell: usize) -> MenuOutcome {
        match GRID[cell] {
            Some(Cell::Item(Item::Info)) => self.mode = Mode::Info,
            Some(Cell::Item(item)) => {
                self.mode = Mode::Edit { item, before: (self.leds, self.idle, self.profile) };
            }
            Some(Cell::Cancel) => return MenuOutcome::Cancel,
            Some(Cell::Save) => return MenuOutcome::Save,
            None => {}
        }
        MenuOutcome::Stay
    }

    fn adjust(&mut self, item: Item, delta: i32) {
        match item {
            Item::Brightness => {
                let p = level_to_percent(self.leds.brightness) as i32 + 5 * delta;
                self.leds.brightness = percent_to_level(p.clamp(0, 100) as u8);
            }
            Item::Leds => self.leds.enabled = !self.leds.enabled,
            Item::IdleDim => self.idle.dim_after_s = step_through(&IDLE_DIM_STEPS, self.idle.dim_after_s, delta),
            Item::IdleOff => self.idle.off_after_s = step_through(&IDLE_OFF_STEPS, self.idle.off_after_s, delta),
            Item::HeldStyle => {
                let styles = [HeldStyle::Brighten, HeldStyle::White, HeldStyle::Invert];
                let i = styles.iter().position(|&s| s == self.leds.held_style).unwrap_or(0) as i32;
                self.leds.held_style = styles[(i + delta).rem_euclid(3) as usize];
            }
            Item::Profile => {
                let n = self.profile_names.len().max(1) as i32;
                self.profile = (self.profile as i32 + delta).rem_euclid(n) as u8;
            }
            Item::Info => {}
        }
    }

    pub fn view(&self) -> MenuView<'_> {
        match &self.mode {
            Mode::Main { selected } => {
                MenuView::Grid { labels: LABELS, selected: *selected, profile: self.profile_name(self.profile) }
            }
            Mode::Edit { item, .. } => {
                let (title, value) = match item {
                    Item::Brightness => ("Brightness", format!("{}%", level_to_percent(self.leds.brightness))),
                    Item::Leds => ("Key LEDs", String::from(if self.leds.enabled { "On" } else { "Off" })),
                    Item::IdleDim => ("Dim after", duration(self.idle.dim_after_s)),
                    Item::IdleOff => ("Off after", duration(self.idle.off_after_s)),
                    Item::HeldStyle => (
                        "Held keys",
                        String::from(match self.leds.held_style {
                            HeldStyle::Brighten => "Brighten",
                            HeldStyle::White => "White",
                            HeldStyle::Invert => "Invert",
                        }),
                    ),
                    Item::Profile => ("Profile", self.profile_name(self.profile)),
                    Item::Info => ("Info", String::new()),
                };
                MenuView::Edit { title, value }
            }
            Mode::Info => MenuView::Info { lines: &self.info },
        }
    }

    /// Key LED colors for the menu (before brightness): items white,
    /// the selection brighter, Cancel red, Save green.
    pub fn key_colors(&self) -> [Rgb; KEY_COUNT] {
        let selected = match &self.mode {
            Mode::Main { selected } => Some(*selected),
            Mode::Edit { item, .. } => Some(Self::cell_of(*item)),
            Mode::Info => Some(Self::cell_of(Item::Info)),
        };
        core::array::from_fn(|i| {
            let base = match GRID[i] {
                None => return Rgb::BLACK,
                Some(Cell::Cancel) => Rgb::new(255, 0, 0),
                Some(Cell::Save) => Rgb::new(0, 255, 0),
                Some(Cell::Item(_)) => Rgb::WHITE,
            };
            if selected == Some(i) { base } else { base.scale(70) }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::example;
    use alloc::vec;

    fn menu() -> Menu {
        Menu::new(&Config::new(example()), 0, vec!["Work".into(), "Games".into()], vec!["fw 0.1".into()])
    }

    fn cw() -> MenuInput {
        MenuInput::Rotate(Direction::Clockwise)
    }

    #[test]
    fn knob_skips_empty_cells_and_wraps() {
        let mut m = menu();
        for _ in 0..6 {
            m.handle(cw());
        }
        // Bright LEDs IdleDim IdleOff Held Profile -> Info (6), then Cancel (9)
        assert!(matches!(m.view(), MenuView::Grid { selected: 6, .. }));
        m.handle(cw());
        assert!(matches!(m.view(), MenuView::Grid { selected: CANCEL_KEY, .. }));
        m.handle(cw());
        m.handle(cw());
        assert!(matches!(m.view(), MenuView::Grid { selected: 0, .. }));
    }

    #[test]
    fn brightness_adjusts_in_five_percent_steps() {
        let mut m = menu();
        m.handle(MenuInput::Key(0));
        let start = level_to_percent(m.leds.brightness);
        m.handle(cw());
        m.handle(cw());
        assert_eq!(level_to_percent(m.leds.brightness), (start + 10).min(100));
        match m.view() {
            MenuView::Edit { title, value } => {
                assert_eq!(title, "Brightness");
                assert_eq!(value, format!("{}%", (start + 10).min(100)));
            }
            _ => panic!("not editing"),
        }
        m.handle(MenuInput::Select);
        assert!(matches!(m.view(), MenuView::Grid { selected: 0, .. }));
    }

    #[test]
    fn cancel_key_reverts_the_edit() {
        let mut m = menu();
        let before = m.leds;
        m.handle(MenuInput::Key(1)); // LEDs
        m.handle(cw());
        assert_ne!(m.leds, before);
        m.handle(MenuInput::Key(CANCEL_KEY));
        assert_eq!(m.leds, before);
        // Cancel in the main view leaves the menu.
        assert_eq!(m.handle(MenuInput::Key(CANCEL_KEY)), MenuOutcome::Cancel);
    }

    #[test]
    fn idle_steps_and_profile_cycling() {
        let mut m = menu();
        m.handle(MenuInput::Key(2)); // IdleDim, default 60 s
        m.handle(cw());
        assert_eq!(m.idle.dim_after_s, 120);
        for _ in 0..10 {
            m.handle(MenuInput::Rotate(Direction::CounterClockwise));
        }
        assert_eq!(m.idle.dim_after_s, 0);
        // Pressing another item's key confirms and opens it.
        m.handle(MenuInput::Key(5));
        m.handle(cw());
        assert_eq!(m.profile, 1);
        m.handle(cw());
        assert_eq!(m.profile, 0);
        assert!(matches!(m.view(), MenuView::Edit { title: "Profile", .. }));
    }

    #[test]
    fn save_and_apply() {
        let mut m = menu();
        m.handle(MenuInput::Key(4)); // held style
        m.handle(cw());
        assert_eq!(m.handle(MenuInput::Key(SAVE_KEY)), MenuOutcome::Save);
        let mut c = Config::new(example());
        m.apply(&mut c);
        assert_eq!(c.leds.held_style, HeldStyle::White);
    }

    #[test]
    fn info_returns_on_any_input() {
        let mut m = menu();
        m.handle(MenuInput::Key(6));
        assert!(matches!(m.view(), MenuView::Info { lines } if lines.len() == 1));
        m.handle(cw());
        assert!(matches!(m.view(), MenuView::Grid { selected: 6, .. }));
    }

    #[test]
    fn durations() {
        assert_eq!(duration(0), "Off");
        assert_eq!(duration(30), "30 s");
        assert_eq!(duration(600), "10 min");
        assert_eq!(duration(3600), "1 h");
    }
}
