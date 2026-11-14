//! Device UI beyond the engine: the settings menu and profile switching.

use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::cell::RefCell;

use inputdeck_engine::codec::{self, Bundle, FORMAT_VERSION};
use inputdeck_engine::led::{self, Rgb};
use inputdeck_engine::menu::{Item, Menu, MenuView, percent_to_level};
use inputdeck_engine::{Config, KEY_COUNT};
use smart_leds::RGB8;

use crate::storage::Storage;

/// Menu contents for the display, owned so it can cross to the display loop.
#[derive(Clone)]
pub enum MenuScreen {
    Grid { labels: [&'static str; KEY_COUNT], selected: usize, profile: String },
    Edit { title: &'static str, value: String },
    Info { lines: Vec<String> },
}

impl MenuScreen {
    pub fn of(menu: &Menu) -> Self {
        match menu.view() {
            MenuView::Grid { labels, selected, profile } => MenuScreen::Grid { labels, selected, profile },
            MenuView::Edit { title, value } => MenuScreen::Edit { title, value },
            MenuView::Info { lines } => MenuScreen::Info { lines: lines.to_vec() },
        }
    }
}

fn profile_label(config: &Config, i: usize) -> String {
    if config.name.is_empty() { format!("Profile {}", i + 1) } else { config.name.clone() }
}

/// The stored bundle, or the running config as a one-profile bundle if
/// nothing is stored (built-in demo).
fn bundle(storage: &RefCell<Storage>, current: &Config) -> Bundle {
    storage.borrow_mut().read_bundle().unwrap_or_else(|| Bundle::single(current.clone()))
}

pub fn open_menu(storage: &RefCell<Storage>, config: &Config) -> Menu {
    let b = bundle(storage, config);
    let names = b.profiles.iter().map(|c| c.name.clone()).collect();
    let info = vec![
        format!("InputDeck {}", env!("CARGO_PKG_VERSION")),
        format!("Config format v{FORMAT_VERSION}"),
        format!("Profile {}/{}", b.active + 1, b.profiles.len()),
        format!("{} layers, {} macros", config.layers.len(), config.macros.len()),
        format!("Layout: {}", if config.layout_name.is_empty() { "us" } else { &config.layout_name }),
    ];
    Menu::new(config, b.active, names, info)
}

/// Stores the menu's changes (settings go to the profile that was active
/// when the menu opened; the selected profile becomes active). Returns the
/// config to run and a notice for the display.
pub fn save_menu(storage: &RefCell<Storage>, config: &Config, menu: &Menu) -> (Config, String) {
    let mut b = bundle(storage, config);
    let current = b.active as usize;
    menu.apply(&mut b.profiles[current]);
    b.active = menu.profile.min(b.profiles.len() as u8 - 1);
    let notice = match storage.borrow_mut().save(&codec::encode(&b)) {
        Ok(()) => String::from("Saved"),
        Err(_) => String::from("Save failed"),
    };
    let active = b.active as usize;
    let label = profile_label(&b.profiles[active], active);
    let notice = if active != current { label } else { notice };
    (b.into_active_config().expect("active index was clamped"), notice)
}

/// A profile switch requested by a key binding.
#[derive(Clone, Copy)]
pub enum Switch {
    /// Next (`+1`) or previous (`-1`), wrapping around.
    Step(i32),
    /// A specific profile by index.
    To(u8),
}

/// Activates another profile. Returns the new config and its name, or a
/// message for the display if there is nothing to switch to.
pub fn switch_profile(storage: &RefCell<Storage>, config: &Config, switch: Switch) -> Result<(Config, String), String> {
    let mut b = bundle(storage, config);
    let n = b.profiles.len() as i32;
    b.active = match switch {
        Switch::Step(_) if n < 2 => return Err(String::from("Only one profile")),
        Switch::Step(delta) => (b.active as i32 + delta).rem_euclid(n) as u8,
        Switch::To(i) if i as i32 >= n => return Err(format!("No profile {}", i as u16 + 1)),
        Switch::To(i) => i,
    };
    let _ = storage.borrow_mut().save(&codec::encode(&b));
    let active = b.active as usize;
    let label = profile_label(&b.profiles[active], active);
    b.into_active_config().map(|c| (c, label)).ok_or_else(|| String::from("Bad profile"))
}

/// Key LEDs while the menu is open. Normally at the edited brightness with
/// a floor so the menu stays visible; while editing brightness or LEDs
/// on/off, the real result is shown instead.
pub fn menu_led_frame(menu: &Menu, max_ma: u32) -> [RGB8; KEY_COUNT] {
    let mut colors = menu.key_colors();
    let brightness = match menu.editing() {
        Some(Item::Leds) if !menu.leds.enabled => 0,
        Some(Item::Brightness) => menu.leds.brightness,
        _ => menu.leds.brightness.max(percent_to_level(15)),
    };
    led::apply_brightness(&mut colors, brightness);
    led::limit_current(&mut colors, max_ma);
    colors.map(|Rgb { r, g, b }| RGB8 { r, g, b })
}
