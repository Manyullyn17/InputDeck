//! Macropad OS firmware for the Adafruit MacroPad RP2040.
//!
//! Bring-up stage: scans keys and encoder, runs the shared engine with a
//! hard-coded demo config, and sends USB HID keyboard and media reports.
//! The red LED is lit whenever a layer other than the base layer is active.

#![no_std]
#![no_main]

mod demo;
mod input;

use embassy_executor::Spawner;
use embassy_futures::join::join;
use embassy_rp::bind_interrupts;
use embassy_rp::gpio::{Input, Level, Output as Pin, Pull};
use embassy_rp::peripherals::USB;
use embassy_rp::usb::{Driver, InterruptHandler};
use embassy_time::{Duration, Ticker};
use embassy_usb::Builder;
use embassy_usb::class::hid::{self, HidBootProtocol, HidSubclass, HidWriter};
use heapless::Vec;
use macropad_engine::{Config, Engine, Event, HidState, Instant, LayerId, Output};
use panic_halt as _;
use usbd_hid::descriptor::{KeyboardReport, MediaKeyboardReport, SerializedDescriptor};

use input::{Debouncer, Encoder};

bind_interrupts!(struct Irqs {
    USBCTRL_IRQ => InterruptHandler<USB>;
});

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_rp::init(Default::default());

    // ---- USB ----
    let driver = Driver::new(p.USB, Irqs);
    // pid.codes test VID/PID; fine for a personal device.
    let mut usb_config = embassy_usb::Config::new(0x1209, 0x0001);
    usb_config.manufacturer = Some("Macropad OS");
    usb_config.product = Some("Macropad OS");
    usb_config.max_power = 100;

    let mut config_descriptor = [0; 256];
    let mut bos_descriptor = [0; 256];
    let mut control_buf = [0; 64];
    // Declared before the builder so they outlive it.
    let mut keyboard_state = hid::State::new();
    let mut media_state = hid::State::new();
    let mut builder = Builder::new(
        driver,
        usb_config,
        &mut config_descriptor,
        &mut bos_descriptor,
        &mut [],
        &mut control_buf,
    );

    let mut keyboard =HidWriter::<_, 8>::new(
        &mut builder,
        &mut keyboard_state,
        hid::Config {
            report_descriptor: KeyboardReport::desc(),
            request_handler: None,
            poll_ms: 1,
            max_packet_size: 8,
            hid_subclass: HidSubclass::Boot,
            hid_boot_protocol: HidBootProtocol::Keyboard,
        },
    );

    let mut media =HidWriter::<_, 2>::new(
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

    let layers = demo::layers();
    let config = Config::new(&layers);

    let input_loop = async {
        let mut debouncer = Debouncer::new();
        let mut encoder = Encoder::new(rot_a.is_high(), rot_b.is_high());
        let mut engine = Engine::new();
        let mut hid_state = HidState::new();
        let mut sent_keyboard = [0u8; 8];
        let mut sent_media = 0u16;
        let mut ticker = Ticker::every(Duration::from_millis(1));

        loop {
            ticker.next().await;
            let now = Instant(embassy_time::Instant::now().as_millis());

            let mut events: Vec<Event, 16> = Vec::new();
            debouncer.scan(&buttons, &mut events);
            if let Some(event) = encoder.update(rot_a.is_high(), rot_b.is_high()) {
                let _ = events.push(event);
            }

            for event in events {
                let mut outputs: Vec<Output, 16> = Vec::new();
                engine.handle(&config, event, now, &mut |o| {
                    let _ = outputs.push(o);
                });

                for output in outputs {
                    if let Output::LayerChanged(layer) = output {
                        led.set_level(if layer == LayerId::BASE { Level::Low } else { Level::High });
                        continue;
                    }
                    // Send after every output so a press+release pair (encoder
                    // tap) shows up as two separate reports.
                    hid_state.apply(output);
                    let r = hid_state.keyboard_report();
                    let k = r.keys;
                    let report = [r.modifiers, 0, k[0], k[1], k[2], k[3], k[4], k[5]];
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
            }
        }
    };

    join(usb.run(), input_loop).await;
}
