# Macropad OS — Project Handoff

**Project status:** Working prototype. Firmware runs on the device with the meta layer engine, USB keyboard/media output, OLED status screen, and configurable key LEDs. Configs are written in TOML, uploaded over USB with `mpcfg`, and stored in flash.  
**Hardware:** Adafruit MacroPad RP2040  
**Last updated:** 2026-11-13

---

## 1. Project Overview

Macropad OS is a custom software environment for the Adafruit MacroPad RP2040.

The central idea is to go beyond conventional macropad functionality. Rather than assigning one action to each physical key, it uses a stateful input system with dynamically changing layers, so a small number of physical inputs can represent a much larger set of logical inputs.

Defining aspects:

- **Meta layers:** ordered, state-dependent layer stacking. The order in which layer keys are pressed matters, and each layer decides what every key (including other layer keys) means.
- **Configuration:** a desktop companion app for extensive configuration, plus a simpler onboard interface. The desktop app is not required to use the device.
- **Rich feedback:** the OLED and per-key RGB LEDs show the current state.

Quantum layers (key releases triggering transitions) remain an optional future experiment, not a V1 requirement.

## 2. Design Documents

| Document | Contents |
| --- | --- |
| [meta-layers.md](meta-layers.md) | Meta layer semantics: layers, fallthrough, global keys, the layer-path stack, held keys across layer switches |
| [leds.md](leds.md) | Key LED color resolution, key states, brightness, idle dimming |
| [actions.md](actions.md) | Toggles and macros (designed, not implemented) |

## 3. Hardware

**Adafruit MacroPad RP2040**: RP2040, 12 mechanical keys (4×3), rotary encoder with push button, 128×64 monochrome OLED (SH1106), 12 RGB LEDs (one per key, none for the encoder), USB-C, STEMMA QT, speaker, red status LED, 8 MB flash.

Pin assignments used by the firmware:

| Function | Pins |
| --- | --- |
| Keys 1–12 (row-major from top-left, active low) | GPIO1–12 |
| Encoder button | GPIO0 |
| Encoder A / B | GPIO17 / GPIO18 |
| Key LEDs (WS2812, GRB) | GPIO19 |
| OLED (SPI1) | SCK 26, MOSI 27, CS 22, RST 23, DC 24 |
| Red status LED | GPIO13 |

Encoder: 4 quadrature steps per detent; the decoder's raw direction is inverted relative to the physical rotation.

## 4. Software Architecture

Rust, as a Cargo workspace with the firmware as a separate workspace:

```text
engine/     no_std library, no hardware code; shared by firmware and future desktop app
  config.rs   config data model, fallthrough resolution, validation
  engine.rs   state machine: events in, outputs out
  hid.rs      engine outputs -> USB HID report contents
  label.rs    short labels for bindings (OLED, later desktop app)
  led.rs      key LED color resolution, brightness, gamma, current limit
firmware/   embassy-rp firmware for the RP2040 (own workspace)
  main.rs     USB, scan loop, display and LED loops, idle handling
  input.rs    debouncing (1 ms polling, 5 ms) and encoder decoding
  display.rs  SH1106 driver and status screen
  storage.rs  two alternating config slots at the end of flash
  serial.rs   USB serial endpoint for config tools
  demo.rs     built-in fallback config when nothing is stored
tools/mpcfg/  host CLI: TOML config -> binary, upload/pull, bootloader
configs/      example configs (demo.toml documents the format)
```

Additional engine modules: `codec.rs` (versioned binary config format with CRC), `notation.rs` (short binding/color notation like `ctrl+c`, `layer:nav`, `#ff0000`), `protocol.rs` (serial framing).

Key principles:

- The engine is pure: `(state, event, time) -> (state, outputs)`. Everything about semantics is unit-tested on the host (`cargo test` at the repo root).
- The firmware is thin: hardware to events, engine outputs to USB/OLED/LEDs.
- Display and LED updates run concurrently with scanning (DMA/PIO) and only ever show the latest state.

### Building and flashing

- Toolchain pinned by `rust-toolchain.toml` (Rust 1.100.0 + `thumbv6m-none-eabi`), installed via rustup.
- In `firmware/`: `cargo run --release`. The runner (`flash.sh`) asks running firmware to reboot into the bootloader (via `mpcfg`), waits for the drive, mounts it if needed, and flashes with `elf2uf2-rs`. Stored configs survive firmware updates.
- Manual bootloader entry: **knob + keys 10 and 12** (hard-wired in firmware, independent of config), or hold the knob while plugging in.
- The udev rule in `tools/udev/` gives the logged-in user access to the device's serial port.

### Configs

- `cargo run -p mpcfg -- check configs/demo.toml` validates a config and points to the exact layer/key on errors.
- `cargo run -p mpcfg -- upload configs/demo.toml` stores it on the device, which reboots to apply it.
- `mpcfg info` shows the stored config; `mpcfg pull -o file.bin` downloads it in binary form.

## 5. Confirmed Decisions

| Subject | Decision |
| --- | --- |
| Project name | Macropad OS |
| Hardware | Adafruit MacroPad RP2040 |
| Language | Rust; engine shared between firmware and desktop app |
| Firmware framework | embassy-rp |
| Meta layers | V1 feature; semantics in [meta-layers.md](meta-layers.md) |
| Layer keys | Hold-only in V1; tap/hold behaviors are a later expansion for any key type |
| Fallthrough | Fixed parent per layer; passes down actions only, not layer keys |
| Global keys | Apply where the current layer inherits; explicit bindings win |
| Held keys | Keep their original action until released |
| USB output | Boot keyboard (6KRO) + consumer control (media keys) |
| OLED | Layer path header + 4×3 grid of effective bindings, held keys inverted |
| Key LEDs | Configurable per key / per layer / globally; see [leds.md](leds.md) |
| Idle | LEDs and OLED dim, then turn off; any input wakes without being swallowed |
| Toggles | Key latch, key repeat, layer toggle as separate types; see [actions.md](actions.md) |
| Macros | Down/up/delay/text steps; one-shot, repeat-while-held, toggle; see [actions.md](actions.md) |
| Quantum layers | Optional future experiment, not V1 |
| Config format | TOML with short binding strings for editing; versioned binary (postcard + CRC) on the device |
| Config storage | Two alternating flash slots, verified after writing; fallback to built-in demo |
| Config transfer | USB serial (CDC-ACM) with a small framed protocol |
| Profiles | Format supports several; one used for now |
| Desktop configuration | Companion application planned, not mandatory |
| Onboard configuration | Simpler built-in interface planned |

## 6. Open Questions

### Configuration

- Migration between binary format versions (the device only reads its own version; tools should convert).
- `mpcfg pull` to TOML (currently binary only).
- Applying an uploaded config without a reboot.
- Profile switching (the format already holds multiple profiles; only one is used).

### Toggles and macros

- See the open and proposed items in [actions.md](actions.md), in particular how layer toggles fit into the layer path and how text typing handles keyboard layouts.

### Desktop application

- Platforms, framework (the Rust engine suggests e.g. Tauri, or a web app via WebAssembly).
- How to visualize and edit meta layers and their relationships.

### Onboard interface

- How to enter and leave configuration mode.
- Which settings are essential without a desktop (brightness, profile switching, simple key assignments).

### Input and output

- Should the encoder participate in layers beyond per-layer rotation bindings (e.g. as a layer key)? It already can, as button 13.
- Use of the speaker and STEMMA QT port.

### Reliability

- Stuck-key handling beyond `Engine::reset` (which exists but has no trigger yet).

## 7. Development Progress

1. ~~Get the board running custom firmware.~~
2. ~~Basic key, encoder, and display handling.~~
3. ~~Event-processing and state-transition model.~~
4. ~~Meta layer semantics.~~
5. ~~Test complex layer transitions and overlapping inputs.~~ (host tests + on-device checks)
6. ~~Key LEDs and idle dimming.~~
7. ~~Configuration storage and format.~~
8. **Toggles and macros** ← next
9. Basic onboard configuration.
10. Desktop companion application.
11. Advanced configuration and visualization (layer graph, transition debugging, sequence replay).
12. Quantum layer experiments, if desired.

## 8. Project Philosophy

The interesting part of Macropad OS isn't having 12 programmable keys. It's exploring how far a small physical input surface can be extended through a more expressive logical input system.

Meta layers are central to that vision. The desktop app and onboard configuration should make that flexibility accessible without requiring users to reason through every possible state transition.

The project should avoid prematurely constraining itself to conventional macropad behavior while keeping the system understandable, deterministic, and configurable.

## 9. Context for Future Work

- This project is separate from the user's other hardware and software projects.
- Suggestions must be distinguished from established decisions; design docs mark items as decided, proposed, or open.
- Avoid turning the concept into an unnecessarily rigid specification.
- The user knows Python and chose Rust partly for the learning experience; explain non-obvious Rust concepts briefly.
