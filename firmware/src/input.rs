//! Key debouncing and encoder decoding, polled once per millisecond.

use embassy_rp::gpio::Input;
use heapless::Vec;
use macropad_engine::{BUTTON_COUNT, Button, Direction, Event, KEY_COUNT};

/// A button's raw state must be stable this many consecutive scans (ms)
/// before a change is reported.
const DEBOUNCE_SCANS: u8 = 5;

pub struct Debouncer {
    pressed: [bool; BUTTON_COUNT],
    counter: [u8; BUTTON_COUNT],
}

impl Debouncer {
    pub const fn new() -> Self {
        Debouncer { pressed: [false; BUTTON_COUNT], counter: [0; BUTTON_COUNT] }
    }

    /// Debounced state of the button at `index` (0..12 keys, 12 encoder).
    pub fn is_pressed(&self, index: usize) -> bool {
        self.pressed[index]
    }

    pub fn scan<const N: usize>(&mut self, pins: &[Input<'_>; BUTTON_COUNT], events: &mut Vec<Event, N>) {
        for (i, pin) in pins.iter().enumerate() {
            let raw = pin.is_low();
            if raw == self.pressed[i] {
                self.counter[i] = 0;
                continue;
            }
            self.counter[i] += 1;
            if self.counter[i] >= DEBOUNCE_SCANS {
                self.counter[i] = 0;
                self.pressed[i] = raw;
                let button = button(i);
                let _ = events.push(if raw { Event::Press(button) } else { Event::Release(button) });
            }
        }
    }
}

fn button(index: usize) -> Button {
    if index < KEY_COUNT { Button::key(index as u8).unwrap() } else { Button::ENCODER }
}

/// Quadrature decoder: four valid transitions make one detent.
pub struct Encoder {
    state: u8,
    steps: i8,
}

/// Indexed by (previous AB << 2) | current AB. Invalid transitions count 0.
const TRANSITIONS: [i8; 16] = [0, -1, 1, 0, 1, 0, 0, -1, -1, 0, 0, 1, 0, 1, -1, 0];
const STEPS_PER_DETENT: i8 = 4;

impl Encoder {
    pub fn new(a: bool, b: bool) -> Self {
        Encoder { state: Self::bits(a, b), steps: 0 }
    }

    fn bits(a: bool, b: bool) -> u8 {
        ((a as u8) << 1) | b as u8
    }

    pub fn update(&mut self, a: bool, b: bool) -> Option<Event> {
        let current = Self::bits(a, b);
        self.steps += TRANSITIONS[((self.state << 2) | current) as usize];
        self.state = current;
        // On the MacroPad, negative steps are clockwise.
        if self.steps >= STEPS_PER_DETENT {
            self.steps = 0;
            Some(Event::Rotate(Direction::CounterClockwise))
        } else if self.steps <= -STEPS_PER_DETENT {
            self.steps = 0;
            Some(Event::Rotate(Direction::Clockwise))
        } else {
            None
        }
    }
}
