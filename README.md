# InputDeck

Custom firmware and a desktop configurator for the [Adafruit MacroPad RP2040](https://www.adafruit.com/product/5128), built around **meta layers**: layer keys that stack in the order you press them, so 12 keys and a knob can reach far more than 12 actions.

> **Status:** working prototype, in daily use on the author's device. The configurator is developed and tested on Linux (KDE Plasma). Expect the config format to change between versions.

## Features

- **Meta layers.** Each layer decides what every key means, including other layer keys, so the order of presses picks the layer: *A then B* and *B then A* can lead to different places. Layers fall through to a parent for keys they don't set. See [meta-layers.md](meta-layers.md).
- **Bindings:** keys and shortcuts, media keys, mouse buttons (up to 16) and scrolling, hold or toggle layers, latch and auto-repeat, macros, and switching profiles.
- **Macros:** tap, hold and release steps, delays, and text typed through your keyboard layout, including characters that need AltGr or dead keys. Each macro runs once, repeats while held, or toggles on and off. See [actions.md](actions.md).
- **Feedback:** the OLED shows the current layer path and what every key does right now. The per-key RGB LEDs are colored by binding type, layer or key, with lock indicators and idle dimming. See [leds.md](leds.md).
- **Configs are TOML files** you can edit by hand or in the app. They're stored on the device, which works without the app. Several profiles fit on one device.
- **Onboard settings menu:** brightness, LEDs, idle timeouts and profile selection, right on the device.
- **Desktop app:** edit layers, macros, colors and settings with undo/redo. A simulator runs the real engine, so you can try a config before uploading it. Push and pull configs over USB.

## Repository layout

```text
engine/          no_std core: config model, layer engine, HID reports, notation (shared by everything)
firmware/        embassy-rp firmware for the RP2040
tools/deckcfg/   config tooling: TOML <-> binary, device protocol, simulator, and the `deckcfg` CLI
app/             desktop configurator (Tauri 2 + SvelteKit)
configs/         example configs; demo.toml documents the format
```

## Getting started

You need [rustup](https://rustup.rs). The toolchain and the `thumbv6m-none-eabi` target are pinned in `rust-toolchain.toml` and installed automatically.

### Flash the firmware

```sh
cargo install elf2uf2-rs
cd firmware
cargo run --release
```

The flash script asks running InputDeck firmware to reboot into the bootloader. On a first install, enter the bootloader by holding the knob while plugging the MacroPad in. The script then waits for the `RPI-RP2` drive and flashes. Configs stored on the device survive firmware updates.

On Linux, install the udev rule so the app and CLI can talk to the device without root:

```sh
sudo cp tools/udev/70-inputdeck.rules /etc/udev/rules.d/
sudo udevadm control --reload
```

Then unplug the device and plug it back in.

### Desktop app

Requires Node.js and the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) (WebKitGTK 4.1 on Linux).

```sh
cd app
npm install
npm run tauri build -- --bundles appimage
../tools/install-appimage.sh    # adds InputDeck to your app menu
```

For development, use `npm run tauri dev`.

### CLI

```sh
cargo run -p deckcfg -- check configs/demo.toml      # validate, with errors pointing at the exact key
cargo run -p deckcfg -- upload configs/demo.toml     # store on the device and apply
cargo run -p deckcfg -- pull -o mine.toml            # download the device's config as TOML
```

## On the device

| Combination | Does |
| --- | --- |
| Knob + keys 1 and 3 | Open the settings menu |
| Knob + keys 10 and 12 | Reboot into the bootloader |

Both combinations work regardless of the loaded config.

## Development

`cargo test` at the repo root runs the engine and tooling tests. The engine is pure (events in, outputs out), so layer, macro and LED behavior is all tested on the host. [handoff.md](handoff.md) covers the architecture and design decisions in more detail.

## License

Licensed under either of [Apache License 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT), at your option.
