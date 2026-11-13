# Key LEDs

Decisions agreed during design discussion (2026-11). Implemented in `engine/src/led.rs` (color resolution) and the firmware (driver, output limits, idle).

## Color resolution

For each key on the current layer, the base color is the first match of:

1. **Per-key color** of the layer that defines the key's binding. Colors belong to bindings, so an inherited binding brings its color along.
2. **Layer palette:** the current layer's color for the binding's category, then its parent layers' palettes.
3. **Global palette.**

Every color setting is one of: a color, off, or inherit (defer to the next level).

Categories: layer key, key, modified key (shortcut), media key, unbound (default off). Macros will get their own category.

## Key states

| State | Color |
| --- | --- |
| Idle | base color at the idle level |
| Held | per-key held color, else the global held style applied to the base color (default: brighten to full level) |
| Released layer key still in the layer path | its layer-key color, dimmed further |
| Toggle on *(planned)* | per-key active color, else the held color, for as long as the toggle is on (not only while held) |
| One-shot macro still running *(planned)* | held color until the macro finishes |

A held key shows the binding it was pressed with, colored as on the layer it was pressed on. A held m1 keeps its layer-key color on L1.

## Global settings

- On/off master switch.
- Brightness (perceptual, gamma-corrected so 50% looks like half).
- Idle level (how bright idle keys are relative to held ones).
- Held style: brighten, white, or invert.

## Idle

Applies to LEDs and OLED. After `dim_after_s` (default 60 s), LEDs drop to a quarter of their brightness and the OLED to low contrast. After `off_after_s` (default 10 min), both turn off. 0 disables a stage. Any input wakes the device; the waking input still performs its action.

## Lock indicators

`[indicators]` assigns keys to Caps/Num/Scroll Lock. While the computer reports the lock as on, the key shows the indicator color instead of its normal one. The OLED header shows `CAPS` while Caps Lock is on, independent of indicators.

## Hardware limits (firmware, not configurable)

Estimated LED current is capped at 300 mA by scaling all LEDs down uniformly.

## Later

- Short flash on layer change.
- Host-driven colors via the desktop app protocol.
- Effects (breathing, etc.); `Color` can grow an effect variant.
