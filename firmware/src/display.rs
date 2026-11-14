//! SH1106 128x64 OLED over SPI1, plus the status screen.
//!
//! Wiring on the MacroPad: SCK GPIO26, MOSI GPIO27, CS GPIO22, RST GPIO23,
//! DC GPIO24. Rendering happens into a RAM framebuffer; `flush` sends it with
//! DMA so key scanning keeps running during the transfer.

use core::convert::Infallible;

use embassy_rp::gpio::Output;
use embassy_rp::peripherals::SPI1;
use embassy_rp::spi::{Async, Spi};
use embassy_time::Timer;
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::mono_font::ascii::FONT_6X10;
use embedded_graphics::pixelcolor::BinaryColor;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{Line, PrimitiveStyle, Rectangle};
use embedded_graphics::text::{Baseline, Text};
use heapless::Vec;
use alloc::string::String;
use embedded_graphics::mono_font::ascii::FONT_10X20;
use inputdeck_engine::{BUTTON_COUNT, Button, Config, KEY_COUNT, LayerId, label};

use crate::ui::MenuScreen;

const WIDTH: usize = 128;
const HEIGHT: usize = 64;
const PAGES: usize = HEIGHT / 8;
/// SH1106 RAM is 132 columns wide; the 128 visible ones start at column 2.
const COLUMN_OFFSET: u8 = 2;

pub struct Framebuffer([u8; WIDTH * PAGES]);

impl Framebuffer {
    pub const fn new() -> Self {
        Framebuffer([0; WIDTH * PAGES])
    }

    fn clear(&mut self) {
        self.0.fill(0);
    }
}

impl OriginDimensions for Framebuffer {
    fn size(&self) -> Size {
        Size::new(WIDTH as u32, HEIGHT as u32)
    }
}

impl DrawTarget for Framebuffer {
    type Color = BinaryColor;
    type Error = Infallible;

    fn draw_iter<I: IntoIterator<Item = Pixel<BinaryColor>>>(&mut self, pixels: I) -> Result<(), Infallible> {
        for Pixel(p, color) in pixels {
            if p.x < 0 || p.y < 0 || p.x >= WIDTH as i32 || p.y >= HEIGHT as i32 {
                continue;
            }
            let (x, y) = (p.x as usize, p.y as usize);
            let byte = &mut self.0[(y / 8) * WIDTH + x];
            let bit = 1 << (y % 8);
            if color.is_on() {
                *byte |= bit;
            } else {
                *byte &= !bit;
            }
        }
        Ok(())
    }
}

pub struct Sh1106 {
    spi: Spi<'static, SPI1, Async>,
    dc: Output<'static>,
    cs: Output<'static>,
}

impl Sh1106 {
    pub async fn new(
        spi: Spi<'static, SPI1, Async>,
        dc: Output<'static>,
        cs: Output<'static>,
        mut reset: Output<'static>,
    ) -> Self {
        reset.set_low();
        Timer::after_millis(10).await;
        reset.set_high();
        Timer::after_millis(10).await;
        // The reset pin must stay high; leak it so it is never dropped.
        core::mem::forget(reset);

        let mut display = Sh1106 { spi, dc, cs };
        display
            .command(&[
                0xAE, // display off
                0xD5, 0x80, // clock divide
                0xA8, 0x3F, // multiplex ratio: 64
                0xD3, 0x00, // display offset
                0x40, // start line 0
                0xAD, 0x8B, // DC-DC converter on
                0xA1, // segment remap
                0xC8, // COM scan direction: remapped
                0xDA, 0x12, // COM pins
                0x81, 0x80, // contrast
                0xD9, 0x22, // pre-charge period
                0xDB, 0x35, // VCOMH level
                0xA4, // display follows RAM
                0xA6, // normal (not inverted)
            ])
            .await;
        display
    }

    pub async fn on(&mut self) {
        self.command(&[0xAF]).await;
    }

    pub async fn off(&mut self) {
        self.command(&[0xAE]).await;
    }

    pub async fn set_contrast(&mut self, contrast: u8) {
        self.command(&[0x81, contrast]).await;
    }

    async fn command(&mut self, bytes: &[u8]) {
        self.dc.set_low();
        self.cs.set_low();
        let _ = self.spi.write(bytes).await;
        self.cs.set_high();
    }

    async fn data(&mut self, bytes: &[u8]) {
        self.dc.set_high();
        self.cs.set_low();
        let _ = self.spi.write(bytes).await;
        self.cs.set_high();
    }

    pub async fn flush(&mut self, fb: &Framebuffer) {
        for page in 0..PAGES {
            self.command(&[0xB0 | page as u8, COLUMN_OFFSET & 0x0F, 0x10 | (COLUMN_OFFSET >> 4)])
                .await;
            self.data(&fb.0[page * WIDTH..(page + 1) * WIDTH]).await;
        }
    }
}

/// Inactivity stage, shared by OLED and LEDs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Active,
    Dim,
    Off,
}

/// Everything the status screen shows, copied out of the engine.
#[derive(Clone)]
pub struct Snapshot {
    pub phase: Phase,
    pub layer: LayerId,
    pub path: Vec<LayerId, BUTTON_COUNT>,
    /// Keys drawn inverted: held, toggled on, or running a macro.
    pub held: [bool; BUTTON_COUNT],
    pub caps_lock: bool,
    /// Short message shown in the header instead of the layer path.
    pub notice: Option<String>,
}

/// What the display shows.
#[derive(Clone)]
pub enum Screen {
    Status(Snapshot),
    Menu(MenuScreen),
}

impl Screen {
    pub fn phase(&self) -> Phase {
        match self {
            Screen::Status(s) => s.phase,
            Screen::Menu(_) => Phase::Active,
        }
    }
}

pub fn render_screen(fb: &mut Framebuffer, config: &Config, screen: &Screen) {
    match screen {
        Screen::Status(snap) => render(fb, config, snap),
        Screen::Menu(menu) => render_menu(fb, menu),
    }
}

fn header_line(fb: &mut Framebuffer) {
    let _ = Line::new(Point::new(0, HEADER_HEIGHT - 2), Point::new(WIDTH as i32 - 1, HEADER_HEIGHT - 2))
        .into_styled(PrimitiveStyle::with_stroke(BinaryColor::On, 1))
        .draw(fb);
}

/// A 4x3 grid of labels below the header; `inverted` cells are highlighted.
fn draw_grid(fb: &mut Framebuffer, labels: &[&str], inverted: impl Fn(usize) -> bool) {
    let on = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);
    let off = MonoTextStyle::new(&FONT_6X10, BinaryColor::Off);
    for (index, text) in labels.iter().enumerate().take(KEY_COUNT) {
        let col = (index % COLUMNS) as i32;
        let row = (index / COLUMNS) as i32;
        let origin = Point::new(col * CELL_WIDTH + 1, HEADER_HEIGHT + row * CELL_HEIGHT);
        let style = if inverted(index) {
            let _ = Rectangle::new(origin, Size::new(CELL_WIDTH as u32 - 1, CELL_HEIGHT as u32))
                .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
                .draw(fb);
            off
        } else {
            on
        };
        let _ = Text::with_baseline(text, origin + Point::new(2, 2), style, Baseline::Top).draw(fb);
    }
}

fn render_menu(fb: &mut Framebuffer, menu: &MenuScreen) {
    fb.clear();
    let on = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);
    match menu {
        MenuScreen::Grid { labels, selected, profile } => {
            let _ = Text::with_baseline("Settings", Point::new(1, 0), on, Baseline::Top).draw(fb);
            let x = WIDTH as i32 - 1 - 6 * profile.chars().count().min(12) as i32;
            let name: String = profile.chars().take(12).collect();
            let _ = Text::with_baseline(&name, Point::new(x, 0), on, Baseline::Top).draw(fb);
            header_line(fb);
            draw_grid(fb, labels, |i| i == *selected);
        }
        MenuScreen::Edit { title, value } => {
            let _ = Text::with_baseline(title, Point::new(1, 0), on, Baseline::Top).draw(fb);
            header_line(fb);
            let big = MonoTextStyle::new(&FONT_10X20, BinaryColor::On);
            let shown: String = value.chars().take(12).collect();
            let x = (WIDTH as i32 - 10 * shown.chars().count() as i32) / 2;
            let _ = Text::with_baseline(&shown, Point::new(x.max(0), 20), big, Baseline::Top).draw(fb);
            let _ = Text::with_baseline("Turn=adjust Press=OK", Point::new(4, 53), on, Baseline::Top).draw(fb);
        }
        MenuScreen::Info { lines } => {
            for (i, line) in lines.iter().take(6).enumerate() {
                let _ = Text::with_baseline(line, Point::new(1, i as i32 * 10 + 2), on, Baseline::Top).draw(fb);
            }
        }
    }
}

const HEADER_HEIGHT: i32 = 11;
const CELL_WIDTH: i32 = 42;
const CELL_HEIGHT: i32 = 13;
const COLUMNS: usize = 3;

fn write_layer_name(out: &mut heapless::String<64>, config: &Config, id: LayerId) {
    let _ = label::write_layer(out, config, id);
}

/// Header: layer path (e.g. "L1>L3"). Below: a 4x3 grid with each key's
/// effective binding on the current layer; held keys are drawn inverted.
pub fn render(fb: &mut Framebuffer, config: &Config, snap: &Snapshot) {
    fb.clear();
    let on = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);
    let off = MonoTextStyle::new(&FONT_6X10, BinaryColor::Off);

    let mut header: heapless::String<64> = heapless::String::new();
    if let Some(notice) = &snap.notice {
        let _ = header.push_str(notice);
    } else if snap.path.is_empty() {
        write_layer_name(&mut header, config, LayerId::BASE);
    }
    for (i, &layer) in snap.path.iter().enumerate().filter(|_| snap.notice.is_none()) {
        if i > 0 {
            let _ = header.push('>');
        }
        write_layer_name(&mut header, config, layer);
    }
    let _ = Text::with_baseline(&header, Point::new(1, 0), on, Baseline::Top).draw(fb);
    if snap.caps_lock {
        // Right-aligned; covers the end of a long layer path.
        let x = WIDTH as i32 - 4 * 6 - 1;
        let _ = Rectangle::new(Point::new(x - 2, 0), Size::new(4 * 6 + 3, HEADER_HEIGHT as u32 - 2))
            .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
            .draw(fb);
        let _ = Text::with_baseline("CAPS", Point::new(x, 0), off, Baseline::Top).draw(fb);
    }
    let _ = Line::new(Point::new(0, HEADER_HEIGHT - 2), Point::new(WIDTH as i32 - 1, HEADER_HEIGHT - 2))
        .into_styled(PrimitiveStyle::with_stroke(BinaryColor::On, 1))
        .draw(fb);

    for index in 0..KEY_COUNT {
        let button = Button::key(index as u8).unwrap();
        let col = (index % COLUMNS) as i32;
        let row = (index / COLUMNS) as i32;
        let origin = Point::new(col * CELL_WIDTH + 1, HEADER_HEIGHT + row * CELL_HEIGHT);

        let style = if snap.held[index] {
            let _ = Rectangle::new(origin, Size::new(CELL_WIDTH as u32 - 1, CELL_HEIGHT as u32))
                .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
                .draw(fb);
            off
        } else {
            on
        };

        let mut text: heapless::String<16> = heapless::String::new();
        let _ = label::write_binding(&mut text, config, config.resolve_button(snap.layer, button));
        let _ = Text::with_baseline(&text, origin + Point::new(2, 2), style, Baseline::Top).draw(fb);
    }
}
