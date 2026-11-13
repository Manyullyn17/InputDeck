//! Macropad OS firmware for the Adafruit MacroPad RP2040.
//!
//! Scans keys and encoder, runs the shared engine with the config stored in
//! flash (or a built-in demo if none is stored), sends USB HID keyboard and
//! media reports, drives the OLED and key LEDs, and offers a USB serial
//! port for config tools (`mpcfg`). Uploaded configs apply immediately.
//! The red LED is lit whenever a layer other than the base layer is active.
//!
//! Hard-wired combos (independent of the config):
//! - knob + keys 1 and 3 (top corners): settings menu
//! - knob + keys 10 and 12 (bottom corners): reboot into the UF2 bootloader

#![no_std]
#![no_main]

extern crate alloc;

mod demo;
mod display;
mod input;
mod keyboard;
mod serial;
mod storage;
mod ui;

use embassy_executor::Spawner;
use embassy_futures::join::{join, join4};
use embassy_rp::flash::Flash;
use embassy_usb::class::cdc_acm::{self, CdcAcmClass};
use embassy_rp::gpio::{Input, Level, Output as Pin, Pull};
use embassy_rp::peripherals::{DMA_CH0, DMA_CH1, PIO0, USB};
use embassy_rp::pio::Pio;
use embassy_rp::pio_programs::ws2812::{PioWs2812, PioWs2812Program};
use embassy_rp::{bind_interrupts, dma, pio, spi, usb};
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::signal::Signal;
use embassy_time::{Duration, Ticker};
use embassy_usb::Builder;
use embassy_usb::class::hid::{self, HidBootProtocol, HidSubclass, HidWriter};
use heapless::Vec;
use macropad_engine::led::{self, Rgb};
use core::cell::RefCell;

use alloc::string::String;
use macropad_engine::menu::{Menu, MenuInput, MenuOutcome};
use macropad_engine::{
    BUTTON_COUNT, Button, Config, Engine, Event, HidState, HostLed, Instant, KEY_COUNT, KeyLight, LayerId,
    NKRO_REPORT_LEN, Output, SystemAction,
};
use smart_leds::RGB8;
use panic_halt as _;
use usbd_hid::descriptor::{MediaKeyboardReport, SerializedDescriptor};

use display::{Framebuffer, Phase, Screen, Sh1106, Snapshot};
use ui::MenuScreen;
use input::{Debouncer, Encoder};

#[global_allocator]
static HEAP: embedded_alloc::LlffHeap = embedded_alloc::LlffHeap::empty();
const HEAP_SIZE: usize = 64 * 1024;

bind_interrupts!(struct Irqs {
    USBCTRL_IRQ => usb::InterruptHandler<USB>;
    DMA_IRQ_0 => dma::InterruptHandler<DMA_CH0>, dma::InterruptHandler<DMA_CH1>;
    PIO0_IRQ_0 => pio::InterruptHandler<PIO0>;
});

/// Hard cap on estimated LED current, independent of config. USB 2.0 ports
/// guarantee 500 mA for the whole device.
const LED_MAX_MA: u32 = 300;
/// OLED contrast when active and when dimmed.
const CONTRAST_ACTIVE: u8 = 0x80;
const CONTRAST_DIM: u8 = 0x08;
/// How long a notice (profile name, "Saved") replaces the layer path.
const NOTICE_MS: u64 = 1500;

const KNOB: usize = 12;
/// Knob + keys 1 and 3: settings menu.
const SETTINGS_COMBO: [usize; 3] = [KNOB, 0, 2];
/// Knob + keys 10 and 12: bootloader.
const BOOTLOADER_COMBO: [usize; 3] = [KNOB, 9, 11];

fn phase(config: &Config, idle_ms: u64) -> Phase {
    let after = |s: u16| s != 0 && idle_ms >= s as u64 * 1000;
    if after(config.idle.off_after_s) {
        Phase::Off
    } else if after(config.idle.dim_after_s) {
        Phase::Dim
    } else {
        Phase::Active
    }
}

/// Final LED output: config colors, then brightness (reduced when dimmed),
/// gamma, and the current cap.
fn led_frame(config: &Config, engine: &Engine, phase: Phase) -> [RGB8; KEY_COUNT] {
    let mut colors = led::key_colors(config, engine, keyboard::host_leds());
    let brightness = match phase {
        Phase::Active => config.leds.brightness,
        Phase::Dim => config.leds.brightness / 4,
        Phase::Off => 0,
    };
    led::apply_brightness(&mut colors, brightness);
    led::limit_current(&mut colors, LED_MAX_MA);
    colors.map(|Rgb { r, g, b }| RGB8 { r, g, b })
}

fn snapshot(engine: &Engine, phase: Phase, notice: Option<String>) -> Snapshot {
    // Highlighted on the OLED: held, toggled on, or running a macro.
    let mut held = [false; BUTTON_COUNT];
    for (i, h) in held.iter_mut().enumerate() {
        *h = matches!(
            engine.key_light(input::button(i)),
            KeyLight::Held { .. } | KeyLight::Active { .. } | KeyLight::Running { .. }
        );
    }
    let caps_lock = HostLed::CapsLock.is_on(keyboard::host_leds());
    Snapshot { phase, layer: engine.layer(), path: engine.layer_path().collect(), held, caps_lock, notice }
}

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    // SAFETY: called once, before anything allocates.
    unsafe {
        embedded_alloc::init!(HEAP, HEAP_SIZE);
    }
    let p = embassy_rp::init(Default::default());

    // ---- USB ----
    let driver = usb::Driver::new(p.USB, Irqs);
    // pid.codes test VID/PID; fine for a personal device.
    let mut usb_config = embassy_usb::Config::new(0x1209, 0x0001);
    usb_config.manufacturer = Some("Macropad OS");
    usb_config.product = Some("Macropad OS");
    usb_config.max_power = 100;
    // Composite device (HID + CDC serial) needs interface association.
    usb_config.composite_with_iads = true;
    usb_config.device_class = 0xEF;
    usb_config.device_sub_class = 0x02;
    usb_config.device_protocol = 0x01;

    let mut config_descriptor = [0; 256];
    let mut bos_descriptor = [0; 256];
    let mut control_buf = [0; 64];
    // Declared before the builder so they outlive it.
    let mut keyboard_state = hid::State::new();
    let mut media_state = hid::State::new();
    let mut serial_state = cdc_acm::State::new();
    let mut keyboard_handler = keyboard::ProtocolHandler;
    let mut builder = Builder::new(
        driver,
        usb_config,
        &mut config_descriptor,
        &mut bos_descriptor,
        &mut [],
        &mut control_buf,
    );

    let mut keyboard = HidWriter::<_, 32>::new(
        &mut builder,
        &mut keyboard_state,
        hid::Config {
            report_descriptor: keyboard::NKRO_DESCRIPTOR,
            request_handler: Some(&mut keyboard_handler),
            poll_ms: 1,
            max_packet_size: 32,
            hid_subclass: HidSubclass::Boot,
            hid_boot_protocol: HidBootProtocol::Keyboard,
        },
    );

    let mut media = HidWriter::<_, 2>::new(
        &mut builder,
        &mut media_state,
        hid::Config {
            report_descriptor: MediaKeyboardReport::desc(),
            request_handler: None,
            poll_ms: 1,
            max_packet_size: 8,
            hid_subclass: HidSubclass::No,
            hid_boot_protocol: HidBootProtocol::None,
        },
    );

    let mut serial_port = CdcAcmClass::new(&mut builder, &mut serial_state, 64);

    let mut usb = builder.build();

    // ---- Inputs ----
    // Keys 1-12 are GPIO1-12 (row-major from top-left), the encoder button is
    // GPIO0. All are active low.
    let buttons: [Input<'static>; 13] = [
        Input::new(p.PIN_1, Pull::Up),
        Input::new(p.PIN_2, Pull::Up),
        Input::new(p.PIN_3, Pull::Up),
        Input::new(p.PIN_4, Pull::Up),
        Input::new(p.PIN_5, Pull::Up),
        Input::new(p.PIN_6, Pull::Up),
        Input::new(p.PIN_7, Pull::Up),
        Input::new(p.PIN_8, Pull::Up),
        Input::new(p.PIN_9, Pull::Up),
        Input::new(p.PIN_10, Pull::Up),
        Input::new(p.PIN_11, Pull::Up),
        Input::new(p.PIN_12, Pull::Up),
        Input::new(p.PIN_0, Pull::Up),
    ];
    let rot_a = Input::new(p.PIN_17, Pull::Up);
    let rot_b = Input::new(p.PIN_18, Pull::Up);
    let mut led = Pin::new(p.PIN_13, Level::Low);

    // ---- Display ----
    let mut spi_config = spi::Config::default();
    spi_config.frequency = 10_000_000;
    let oled_spi = spi::Spi::new_txonly(p.SPI1, p.PIN_26, p.PIN_27, p.DMA_CH0, Irqs, spi_config);
    let oled_cs = Pin::new(p.PIN_22, Level::High);
    let oled_reset = Pin::new(p.PIN_23, Level::High);
    let oled_dc = Pin::new(p.PIN_24, Level::Low);

    // ---- Key LEDs (WS2812 on GPIO19) ----
    let Pio { mut common, sm0, .. } = Pio::new(p.PIO0, Irqs);
    let ws2812_program = PioWs2812Program::new(&mut common);
    let mut key_leds = PioWs2812::new(&mut common, sm0, p.DMA_CH1, Irqs, p.PIN_19, &ws2812_program);

    // ---- Config ----
    let mut flash_store = storage::Storage::new(Flash::new_blocking(p.FLASH));
    let initial = flash_store.load().unwrap_or_else(|| Config::new(demo::layers().into()));
    // Shared by all loops. Borrows are never held across an await; uploads
    // arrive through `new_config` and are swapped in by the input loop.
    let store = RefCell::new(flash_store);
    let config = RefCell::new(initial);
    let new_config: Signal<NoopRawMutex, Config> = Signal::new();

    let serial_loop = serial::run(&mut serial_port, &store, &new_config);

    // Latest state for display and LEDs; newer values overwrite unread ones.
    let screen: Signal<NoopRawMutex, Screen> = Signal::new();
    let lights: Signal<NoopRawMutex, [RGB8; KEY_COUNT]> = Signal::new();

    let led_loop = async {
        let first = led_frame(&config.borrow(), &Engine::new(), Phase::Active);
        key_leds.write(&first).await;
        loop {
            key_leds.write(&lights.wait().await).await;
        }
    };

    let display_loop = async {
        let mut oled = Sh1106::new(oled_spi, oled_dc, oled_cs, oled_reset).await;
        let mut fb = Framebuffer::new();
        display::render(&mut fb, &config.borrow(), &snapshot(&Engine::new(), Phase::Active, None));
        oled.flush(&fb).await;
        oled.set_contrast(CONTRAST_ACTIVE).await;
        oled.on().await;
        let mut shown = Phase::Active;
        loop {
            let next = screen.wait().await;
            let phase = next.phase();
            if phase != Phase::Off {
                display::render_screen(&mut fb, &config.borrow(), &next);
                oled.flush(&fb).await;
            }
            if phase != shown {
                match phase {
                    Phase::Active => oled.set_contrast(CONTRAST_ACTIVE).await,
                    Phase::Dim => oled.set_contrast(CONTRAST_DIM).await,
                    Phase::Off => oled.off().await,
                }
                if shown == Phase::Off {
                    oled.on().await;
                }
                shown = phase;
            }
        }
    };

    let input_loop = async {
        let mut debouncer = Debouncer::new();
        let mut encoder = Encoder::new(rot_a.is_high(), rot_b.is_high());
        let mut engine = Engine::new();
        let mut hid_state = HidState::new();
        // Last keyboard report sent, in whichever protocol was active.
        let mut sent_keyboard: heapless::Vec<u8, NKRO_REPORT_LEN> = heapless::Vec::new();
        let mut sent_media = 0u16;
        let mut ticker = Ticker::every(Duration::from_millis(1));
        let mut last_activity = embassy_time::Instant::now();
        let mut current_phase = Phase::Active;
        let mut host_leds = keyboard::host_leds();
        let mut menu: Option<Menu> = None;
        // After opening the menu, inputs are ignored until all keys are up.
        let mut menu_armed = false;
        let mut notice: Option<(String, u64)> = None;

        loop {
            ticker.next().await;
            let now = Instant(embassy_time::Instant::now().as_millis());

            let mut events: Vec<Event, 16> = Vec::new();
            debouncer.scan(&buttons, &mut events);
            if BOOTLOADER_COMBO.iter().all(|&b| debouncer.is_pressed(b)) {
                embassy_rp::rom_data::reset_to_usb_boot(0, 0);
            }
            if let Some(event) = encoder.update(rot_a.is_high(), rot_b.is_high()) {
                let _ = events.push(event);
            }

            let mut screen_dirty = false;
            // Heap-allocated so a burst of outputs can't be truncated
            // (a dropped release would leave a key stuck).
            let mut outputs: alloc::vec::Vec<Output> = alloc::vec::Vec::new();
            let mut open_settings = menu.is_none() && SETTINGS_COMBO.iter().all(|&b| debouncer.is_pressed(b));
            let mut switch: Option<i32> = None;
            let mut replace_config: Option<(Config, Option<String>)> = None;

            // A newly uploaded config: leave the menu, release everything, swap.
            if let Some(next) = new_config.try_take() {
                menu = None;
                replace_config = Some((next, None));
            }
            let leds_now = keyboard::host_leds();
            if leds_now != host_leds {
                host_leds = leds_now;
                screen_dirty = true;
            }
            if !events.is_empty() {
                last_activity = embassy_time::Instant::now();
            }
            if let Some((_, until)) = &notice
                && now.0 >= *until
            {
                notice = None;
                screen_dirty = true;
            }

            if let Some(m) = menu.as_mut() {
                // ---- Settings menu ----
                if !menu_armed {
                    menu_armed = (0..BUTTON_COUNT).all(|b| !debouncer.is_pressed(b));
                    events.clear();
                }
                let mut outcome = MenuOutcome::Stay;
                for event in events {
                    let input = match event {
                        Event::Press(b) if b == Button::ENCODER => MenuInput::Select,
                        Event::Press(b) => MenuInput::Key(b.index()),
                        Event::Rotate(dir) => MenuInput::Rotate(dir),
                        Event::Release(_) => continue,
                    };
                    outcome = m.handle(input);
                    screen_dirty = true;
                    if outcome != MenuOutcome::Stay {
                        break;
                    }
                }
                match outcome {
                    MenuOutcome::Stay => {}
                    MenuOutcome::Cancel => {
                        menu = None;
                        screen_dirty = true;
                    }
                    MenuOutcome::Save => {
                        let (next, text) = ui::save_menu(&store, &config.borrow(), m);
                        menu = None;
                        replace_config = Some((next, Some(text)));
                    }
                }
                current_phase = Phase::Active;
            } else {
                // ---- Normal operation ----
                let config = config.borrow();
                // Any input wakes the device; the waking input is not swallowed.
                let new_phase = phase(&config, last_activity.elapsed().as_millis());
                if new_phase != current_phase {
                    current_phase = new_phase;
                    screen_dirty = true;
                }
                if !open_settings {
                    screen_dirty |= engine.tick(&config, now, &mut |o| outputs.push(o));
                    for event in events {
                        screen_dirty |= matches!(event, Event::Press(_) | Event::Release(_));
                        engine.handle(&config, event, now, &mut |o| outputs.push(o));
                    }
                }
                for output in &outputs {
                    match output {
                        Output::System(SystemAction::Settings) => open_settings = true,
                        Output::System(SystemAction::NextProfile) => switch = Some(1),
                        Output::System(SystemAction::PrevProfile) => switch = Some(-1),
                        Output::System(SystemAction::Bootloader) => embassy_rp::rom_data::reset_to_usb_boot(0, 0),
                        _ => {}
                    }
                }
            }

            if let Some(delta) = switch {
                let switched = ui::switch_profile(&store, &config.borrow(), delta);
                match switched {
                    Some((next, name)) => replace_config = Some((next, Some(name))),
                    None => {
                        notice = Some((String::from("Only one profile"), now.0 + NOTICE_MS));
                        screen_dirty = true;
                    }
                }
            }
            if open_settings && menu.is_none() && replace_config.is_none() {
                engine.reset(&mut |o| outputs.push(o));
                menu = Some(ui::open_menu(&store, &config.borrow()));
                menu_armed = false;
                screen_dirty = true;
            }
            if let Some((next, text)) = replace_config {
                engine.reset(&mut |o| outputs.push(o));
                *config.borrow_mut() = next;
                notice = text.map(|t| (t, now.0 + NOTICE_MS));
                last_activity = embassy_time::Instant::now();
                screen_dirty = true;
            }

            for output in outputs {
                match output {
                    Output::LayerChanged(layer) => {
                        led.set_level(if layer == LayerId::BASE { Level::Low } else { Level::High });
                        continue;
                    }
                    Output::System(_) => continue,
                    _ => {}
                }
                // Send after every output so a press+release pair (encoder
                // tap) shows up as two separate reports.
                hid_state.apply(output);
                let report: heapless::Vec<u8, NKRO_REPORT_LEN> = if keyboard::boot_protocol() {
                    let r = hid_state.keyboard_report();
                    let k = r.keys;
                    heapless::Vec::from_slice(&[r.modifiers, 0, k[0], k[1], k[2], k[3], k[4], k[5]]).unwrap()
                } else {
                    heapless::Vec::from_slice(&hid_state.nkro_report()).unwrap()
                };
                if report != sent_keyboard {
                    let _ = keyboard.write(&report).await;
                    sent_keyboard = report;
                }
                let m = hid_state.consumer_report();
                if m != sent_media {
                    let _ = media.write(&m.to_le_bytes()).await;
                    sent_media = m;
                }
            }
            if screen_dirty {
                match &menu {
                    Some(m) => {
                        screen.signal(Screen::Menu(MenuScreen::of(m)));
                        lights.signal(ui::menu_led_frame(m, LED_MAX_MA));
                    }
                    None => {
                        let text = notice.as_ref().map(|(t, _)| t.clone());
                        screen.signal(Screen::Status(snapshot(&engine, current_phase, text)));
                        lights.signal(led_frame(&config.borrow(), &engine, current_phase));
                    }
                }
            }
        }
    };

    join(join4(usb.run(), input_loop, display_loop, led_loop), serial_loop).await;
}
