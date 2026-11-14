//! Second USB HID interface: media keys (consumer control) and a mouse
//! (16 buttons, vertical wheel, horizontal pan), told apart by report ID.
//!
//! Sharing one interface makes Linux name the input devices "<product>
//! Consumer Control" and "<product> Mouse" instead of giving every interface
//! the same bare name. The keyboard stays on its own interface because the
//! boot protocol doesn't allow report IDs.
//!
//! The mouse declares X and Y (always 0) so hosts classify it as a pointing
//! device; without them, Linux/libinput ignores the buttons.

use inputdeck_engine::{MOUSE_REPORT_LEN, MouseReport};

const CONSUMER_ID: u8 = 1;
const MOUSE_ID: u8 = 2;

const _: () = assert!(MOUSE_REPORT_LEN == 6, "descriptor below describes a 6-byte mouse report");

pub const DESCRIPTOR: &[u8] = &[
    // Consumer control first: if a keyboard collection preceded it, Linux
    // would fold it into that device.
    0x05, 0x0C, // Usage Page (Consumer)
    0x09, 0x01, // Usage (Consumer Control)
    0xA1, 0x01, // Collection (Application)
    0x85, CONSUMER_ID, // Report ID
    0x15, 0x00, //   Logical Minimum (0)
    0x26, 0xFF, 0x03, // Logical Maximum (0x3FF)
    0x19, 0x00, //   Usage Minimum (0)
    0x2A, 0xFF, 0x03, // Usage Maximum (0x3FF)
    0x75, 0x10, //   Report Size (16)
    0x95, 0x01, //   Report Count (1)
    0x81, 0x00, //   Input (Data, Array, Absolute)
    0xC0, //     End Collection
    0x05, 0x01, // Usage Page (Generic Desktop)
    0x09, 0x02, // Usage (Mouse)
    0xA1, 0x01, // Collection (Application)
    0x85, MOUSE_ID, // Report ID
    0x09, 0x01, //   Usage (Pointer)
    0xA1, 0x00, //   Collection (Physical)
    0x05, 0x09, //     Usage Page (Button)
    0x19, 0x01, //     Usage Minimum (1)
    0x29, 0x10, //     Usage Maximum (16)
    0x15, 0x00, //     Logical Minimum (0)
    0x25, 0x01, //     Logical Maximum (1)
    0x75, 0x01, //     Report Size (1)
    0x95, 0x10, //     Report Count (16)
    0x81, 0x02, //     Input (Data, Variable, Absolute)
    0x05, 0x01, //     Usage Page (Generic Desktop)
    0x09, 0x30, //     Usage (X)
    0x09, 0x31, //     Usage (Y)
    0x09, 0x38, //     Usage (Wheel)
    0x15, 0x81, //     Logical Minimum (-127)
    0x25, 0x7F, //     Logical Maximum (127)
    0x75, 0x08, //     Report Size (8)
    0x95, 0x03, //     Report Count (3)
    0x81, 0x06, //     Input (Data, Variable, Relative)
    0x05, 0x0C, //     Usage Page (Consumer)
    0x0A, 0x38, 0x02, // Usage (AC Pan)
    0x95, 0x01, //     Report Count (1)
    0x81, 0x06, //     Input (Data, Variable, Relative)
    0xC0, //       End Collection
    0xC0, //     End Collection
];

/// Longest report including its ID byte.
pub const MAX_REPORT_LEN: usize = 1 + MOUSE_REPORT_LEN;

pub fn consumer_report(usage: u16) -> [u8; 3] {
    let [lo, hi] = usage.to_le_bytes();
    [CONSUMER_ID, lo, hi]
}

pub fn mouse_report(r: MouseReport) -> [u8; MAX_REPORT_LEN] {
    let mut out = [MOUSE_ID; MAX_REPORT_LEN];
    out[1..].copy_from_slice(&r.to_bytes());
    out
}
