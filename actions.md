# Toggles and Macros

Design discussion (2026-11). Implemented in `engine/src/engine.rs` (runtime) and `engine/src/layout.rs` (text layouts: `us`, `de`). "Decided" items come from the user; "Proposed" items are suggestions awaiting confirmation; "Open" items are deliberately undecided.

## Toggles

Toggles are separate binding types, one per behavior.

### Decided

| Type | First press | Next press |
| --- | --- | --- |
| **Key latch** | key goes down and stays down | key goes up |
| **Key repeat** (autoclicker) | key is tapped repeatedly at an interval | repeating stops |
| **Layer toggle** | layer turns on without holding | layer turns off |

Latch and repeat are separate on purpose: some applications need a key held down continuously, others need repeated presses.

While a toggle is on, its key uses its **active** LED color (see [leds.md](leds.md)).

**An active toggle owns its physical key.** The next press of that key turns the toggle off, whatever the key means on the current layer. Without this, switching layers could leave a toggle that can't be turned off from where you are.

**A layer toggle behaves like a layer key that is held without being held.** It pushes a layer-path entry that counts as held until the toggle is turned off. Turning it off is then exactly like releasing a layer key: if layers above it are active, the entry stays in the path (shown dimmed) and is popped once those layers are released, per the rules in [meta-layers.md](meta-layers.md).

### Proposed

- Key repeat has a configurable interval per binding.

### Open

- Whether there should be a "turn off all toggles" action, separate from the device reset.

## Macros

### Content (decided)

A macro is a sequence of steps:

- **key down** / **key up**, separately, so a key can be held across later steps (e.g. hold Shift for part of the sequence),
- **delay** in milliseconds,
- **text**, typed out without defining each key press by hand.

Convenience steps such as "tap" (down + up) can be expanded into down/up by the config tools.

### Types (decided)

| Type | Behavior |
| --- | --- |
| **One-shot** | runs the sequence once per press |
| **Repeat while held** | runs the sequence, and starts it again each time it finishes while the key is still held; after release, the current run finishes |
| **Toggle** | first press starts repeating it indefinitely; another press stops it |

LEDs: a running one-shot uses the held color until it finishes; an active toggle macro uses the active color.

### Settings (decided as options, defaults open)

- **Toggle macro stop:** finish the current run, or stop immediately. A per-macro setting.
- **Pressing a running one-shot again:** ignore for now. Later options may include cancel, restart, or queue another run after the current one. The config format should make room for this as a per-macro setting.

### Decided

- When a macro stops early (stop immediately, cancel, device reset), every key it is still holding down is released, so nothing gets stuck.
- **Text needs a keyboard layout setting.** USB keyboards send key positions, not characters; the computer turns them into characters using its layout (e.g. US vs. German, where Y/Z and most symbols differ). The device cannot query the computer's layout over USB, so the layout is a device setting. The desktop app can detect the active (and installed) layouts and fill it in; without the app it is chosen manually.

### Proposed

- Text typing has a default delay between characters, overridable per macro.
- Characters that don't exist in the selected layout are reported by config validation.

### Decided (concurrency)

- Several macros can run at once (up to 8); each macro runs at most once at a time.
- Text is typed with exactly the modifiers each character needs; other held modifiers (latches, other macros, physical keys) are suppressed for that keystroke. Down/up/tap steps combine with held modifiers on purpose.
- A key pressed by two sources stays down until both release it; a tap of an already-held key is not seen by the computer. Left as is.

### Open

- Whether macros can trigger layer changes, or only send keys.

## Engine implications

Macros, key repeat, and later tap/hold all need time-based behavior. The engine already accepts a timestamp with every event; it will additionally need to tell the firmware when it next wants to be called ("wake me at time T") and handle a tick event, so the engine stays a pure function that can be tested by simulating time.
