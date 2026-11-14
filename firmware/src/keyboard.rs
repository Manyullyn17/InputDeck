//! USB keyboard interface: an N-key-rollover report with a boot-protocol
//! fallback.
//!
//! The interface describes an NKRO report (modifier byte + one bit per key
//! code, see `inputdeck_engine::hid`). It is still marked as a boot keyboard:
//! if the host asks for the boot protocol (BIOS, some KVMs), the firmware
//! sends the standard 8-byte 6-key report instead.

use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};

use embassy_usb::class::hid::{HidProtocolMode, ReportId, RequestHandler};
use embassy_usb::control::OutResponse;
use inputdeck_engine::hid::NKRO_KEYS;

/// Whether the host selected the boot protocol.
static BOOT_PROTOCOL: AtomicBool = AtomicBool::new(false);
/// Host keyboard LEDs (bit 0 Num Lock, 1 Caps Lock, 2 Scroll Lock, ...).
static HOST_LEDS: AtomicU8 = AtomicU8::new(0);

pub fn boot_protocol() -> bool {
    BOOT_PROTOCOL.load(Ordering::Relaxed)
}

/// Host keyboard LED state; not used yet (planned Caps Lock indicator).
#[allow(dead_code)]
pub fn host_leds() -> u8 {
    HOST_LEDS.load(Ordering::Relaxed)
}

pub struct ProtocolHandler;

impl RequestHandler for ProtocolHandler {
    fn get_protocol(&self) -> HidProtocolMode {
        if boot_protocol() { HidProtocolMode::Boot } else { HidProtocolMode::Report }
    }

    fn set_protocol(&mut self, protocol: HidProtocolMode) -> OutResponse {
        BOOT_PROTOCOL.store(protocol == HidProtocolMode::Boot, Ordering::Relaxed);
        OutResponse::Accepted
    }

    fn set_report(&mut self, _id: ReportId, data: &[u8]) -> OutResponse {
        if let Some(&leds) = data.first() {
            HOST_LEDS.store(leds, Ordering::Relaxed);
        }
        OutResponse::Accepted
    }
}

const _: () = assert!(NKRO_KEYS == 224, "descriptor below hard-codes 224 key bits");

/// HID report descriptor for the NKRO report (29 bytes) plus the LED
/// output report.
pub const NKRO_DESCRIPTOR: &[u8] = &[
    0x05, 0x01, // Usage Page (Generic Desktop)
    0x09, 0x06, // Usage (Keyboard)
    0xA1, 0x01, // Collection (Application)
    // Modifiers: 8 bits.
    0x05, 0x07, //   Usage Page (Keyboard/Keypad)
    0x19, 0xE0, //   Usage Minimum (Left Control)
    0x29, 0xE7, //   Usage Maximum (Right GUI)
    0x15, 0x00, //   Logical Minimum (0)
    0x25, 0x01, //   Logical Maximum (1)
    0x75, 0x01, //   Report Size (1)
    0x95, 0x08, //   Report Count (8)
    0x81, 0x02, //   Input (Data, Variable, Absolute)
    // Host LEDs: 5 bits + 3 bits padding.
    0x05, 0x08, //   Usage Page (LEDs)
    0x19, 0x01, //   Usage Minimum (Num Lock)
    0x29, 0x05, //   Usage Maximum (Kana)
    0x95, 0x05, //   Report Count (5)
    0x91, 0x02, //   Output (Data, Variable, Absolute)
    0x95, 0x03, //   Report Count (3)
    0x91, 0x01, //   Output (Constant)
    // Keys: one bit per usage 0x00..=0xDF.
    0x05, 0x07, //   Usage Page (Keyboard/Keypad)
    0x19, 0x00, //   Usage Minimum (0)
    0x29, 0xDF, //   Usage Maximum (223)
    0x95, 0xE0, //   Report Count (224)
    0x81, 0x02, //   Input (Data, Variable, Absolute)
    0xC0, // End Collection
];
