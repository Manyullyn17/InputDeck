# Macropad OS — Project Handoff

**Project status:** Early concept / exploratory design  
**Hardware:** Adafruit MacroPad RP2040  
**Specification status:** Core concepts and intended user experience partially defined. Firmware architecture and implementation details largely open.

---

## 1. Project Overview

Macropad OS is a project to create a custom software environment for the Adafruit MacroPad RP2040.

The central idea is to go beyond conventional macropad functionality.

Rather than simply assigning one action to each physical key, the project uses a stateful input system with dynamically changing layers.

The goal is to make a relatively small number of physical inputs capable of representing a much larger set of logical inputs.

Two particularly important aspects are:

- **Meta layers:** A fundamental feature involving ordered, state-dependent layer stacking.
- **Configuration:** A desktop companion application for more extensive configuration, complemented by a simpler configuration interface directly on the device.

Quantum layers are another interesting concept, but remain an optional future experiment rather than a V1 requirement.

The project is still in an early design phase. There is no complete technical specification or finalized architecture yet.

## 2. Hardware

The hardware is confirmed:

**Adafruit MacroPad RP2040 kit**

Specifications:
- Microcontroller: Raspberry Pi RP2040
- Physical keys: 12 mechanical switches arranged in a 4×3 grid
- Rotary encoder: One knob with push-button functionality
- Display: 128×64 monochrome OLED
- Connectivity: USB-C
- Additional hardware: NeoPixel LEDs and STEMMA QT connector

The board provides:
- 12 independently addressable physical keys
- Rotary encoder input
- Encoder push-button input
- OLED display for dynamic information

The display and encoder are particularly useful because they provide more than just additional inputs: they allow the device to display its current state and provide contextual navigation or adjustment.

---

## 3. Core Feature: Meta Layers

Meta layers are one of the defining concepts of Macropad OS and a fundamental V1 requirement.

The basic idea is that layer transitions are ordered and state-dependent.

For example:

- M1 → M2
- M2 → M1

These are distinct sequences and can result in different logical states.

This differs from conventional modifier systems where active modifiers are generally treated as an unordered combination.

The order in which layers are activated matters.

Additionally, changing layers can change the meaning of other layer-switching keys.

A key that activates M2 in one state might activate something entirely different in another state.

This makes the system fundamentally state-dependent.

Meta layers are not simply multiple simultaneously enabled keymaps. Their ordering and transition history are meaningful parts of the resulting state.

**Important:** Meta layers are intended to be included in the initial usable version, not postponed as an experimental feature.

### Questions still open

- What exactly constitutes a meta layer?
- How are meta layers represented internally?
- Are transitions always additive, or can they replace existing state?
- How should returning to previous states work?
- Can meta layers be explicitly named and configured?
- How should held keys behave across transitions?
- What happens when multiple transitions are triggered in quick succession?

These details need to be established during implementation and experimentation.

---

## 4. Experimental Concept: Quantum Layers

Quantum layers are an additional concept worth keeping open for future experimentation.

They are not a V1 requirement.

The idea is that key releases can participate in layer transitions just like key presses.

A physical key therefore doesn't necessarily have one fixed meaning throughout its entire press/release lifecycle.

For example:

1. Press M1.
2. The system transitions into layer M1.
3. Press another key while M1 is active.
4. Release M1.
5. The release event triggers another layer transition.

This allows the system to use both key-down and key-up events as meaningful inputs for state transitions.

The interesting implication is that a key's release can be interpreted in the context of a state that is different from the state in which it was pressed.

An eventual consequence of this and ordered transitions could be an indefinitely extensible sequence of logical inputs using only a handful of physical keys.

However, this is an experimental direction, not something the initial implementation needs to support.

**V1 priority:** Get meta layers working correctly without requiring quantum layer mechanics.

The architecture should ideally avoid making future experimentation unnecessarily difficult, but should not be complicated just to accommodate an unproven feature.

---

## 5. Configuration System

Configuration is an important part of the intended product.

The project is intended to have two complementary configuration interfaces.

### 5.1 Desktop Companion Application

A desktop application will provide a more extensive and convenient way to configure Macropad OS.

Its purpose is to make configuring a potentially complex layer system manageable.

Potential functionality:
- Configure physical key assignments.
- Create and edit layers.
- Configure meta layer behavior and transitions.
- Configure encoder behavior.
- Customize display-related settings.
- Manage and transfer configurations to the device.
- Potentially visualize the layer structure and relationships.

The desktop application should make complex configurations easier to understand and edit.

The exact platform, framework, communication protocol, and feature scope are undecided.

### 5.2 Onboard Configuration

The device should also have a simpler built-in configuration interface.

This interface uses the physical buttons, rotary encoder, and OLED display.

The intention is that basic configuration and adjustments should be possible without requiring a connected desktop application.

Potential functionality:
- Navigate configuration menus.
- Change basic key assignments.
- Switch or select configurations.
- Adjust basic settings.
- Perform simple layer-related configuration.

The onboard interface does not necessarily need to expose every advanced configuration option available in the desktop application.

**Important distinction:** The desktop app is the more comprehensive configuration tool, not a mandatory dependency for configuring or using the device.

The exact division of functionality between the two interfaces remains open.

---

## 6. Display and User Interface

The OLED display should provide useful feedback about the device's current state.

Potential uses:
- Display the currently active logical state.
- Show the current interpretation of all 12 keys.
- Display layer names and contextual information.
- Show which keys are currently held.
- Provide visual feedback when a transition occurs.
- Display onboard configuration menus.

One particularly interesting idea is to show the current interpretation of the physical keys rather than displaying a static keymap.

This would make dynamically changing meta layers much easier to understand.

The rotary encoder could be used for contextual navigation, value adjustment, and interacting with onboard configuration.

---

## 7. General Software Design Implications

The concepts suggest that the firmware should be designed around an event-driven state machine.

Physical input events should be processed in sequence, with the current state determining how each event is interpreted.

Relevant event types include:
- Key down
- Key up
- Encoder clockwise rotation
- Encoder counterclockwise rotation
- Encoder button down
- Encoder button up

Important considerations:

- Event ordering must be preserved.
- Meta layer transitions must be deterministic.
- Held keys must remain tracked across layer changes.
- The meaning of an input may depend on the current state.
- State transitions should be independently testable.
- The configuration system must be able to represent the meta layer model.

A conventional implementation might use an event queue and a state-transition function, but the exact architecture is undecided.

The configuration representation should ideally be designed alongside the state model rather than bolted on afterward.

---

## 8. Potential Development and Debugging Features

These are ideas worth considering, not finalized requirements.

### Layer Visualization
Visualize the current state and how each physical input is interpreted.

### Transition Debugging
Record physical events and resulting logical state transitions.

### Sequence Replay
Allow recorded input sequences to be replayed to reproduce unexpected behavior.

### Layer Graph
Visualize reachable states and transitions as a graph.

### State-Machine Testing
Test different combinations and orderings of key presses and releases, particularly overlapping held keys.

### Configuration Validation
Validate configurations before transferring them to the device, especially when they contain complex meta layer relationships.

---

## 9. Confirmed Decisions vs. Open Questions

### Confirmed

| Subject | Decision |
|---|---|
| Project name | Macropad OS |
| Hardware | Adafruit MacroPad RP2040 |
| Microcontroller | RP2040 |
| Physical layout | 4×3 keys |
| Additional input | Rotary encoder with push button |
| Display | 128×64 OLED |
| Core input system | Stateful, dynamically changing layers |
| Meta layers | Fundamental V1 feature |
| Meta layer behavior | Ordered and state-dependent |
| Quantum layers | Optional future experiment, not V1 |
| Meta layer semantics | Defined in [meta-layers.md](meta-layers.md) |
| Firmware language | Rust (shared engine crate between firmware and desktop) |
| Desktop configuration | Companion application planned |
| Onboard configuration | Simpler built-in interface planned |
| Project maturity | Early design phase |

### Open Questions

**Firmware and runtime**
- Which programming language should be used?
- Which framework or SDK should be used?
- Arduino ecosystem, Pico SDK, or another approach?
- How should the event-processing loop work?
- How should state be represented internally?

**Meta layer system**
- Exact semantics and internal representation.
- How transitions are defined and resolved.
- How previous states are restored or exited.
- How overlapping held keys are handled.
- Whether there are restrictions on reachable states.
- How invalid or unreachable configurations are handled.

**Configuration**
- How should configurations be stored?
- What configuration format should be used?
- How does the desktop app communicate with the device?
- How are configurations transferred and validated?
- Which features are available onboard versus exclusively in the desktop app?
- Can configurations be backed up, exported, and imported?
- Should multiple configurations/profiles be supported?

**Desktop application**
- Which operating systems should be supported?
- Which framework and programming language?
- How should the UI represent meta layers?
- Should there be a graphical layer editor?
- How should device connection and configuration synchronization work?

**Onboard interface**
- What should the default screen show?
- How should menus and navigation work?
- Which configuration options are essential without a desktop?
- How should the user return to normal operation from configuration mode?

**Input and output**
- Keyboard HID, consumer control, other HID interfaces, or a combination?
- How should encoder events interact with layers?
- Should the encoder have contextual behavior?
- How should simultaneous inputs be handled?

**Reliability and testing**
- How should stuck-key scenarios be handled?
- What happens if power is lost while keys are held?
- How should state reset work?
- How can complex transition sequences be tested systematically?

**Future scope**
- How might quantum layers eventually fit into the architecture?
- Should user-defined applications or modes be separate concepts?
- How much functionality belongs in firmware versus the desktop application?

---

## 10. Suggested Development Direction

A sensible eventual development sequence:

1. Get the board running custom firmware.
2. Implement basic key, encoder, and display handling.
3. Establish the event-processing and state-transition model.
4. Define and implement meta layer semantics.
5. Test complex layer transitions and overlapping inputs.
6. Design a configuration representation capable of expressing meta layers.
7. Implement basic onboard configuration.
8. Develop the desktop companion application.
9. Add more advanced configuration and visualization features.
10. Experiment with quantum layers independently if desired.

The underlying state model and configuration format are particularly important architectural decisions. Both configuration interfaces will depend on them.

---

## 11. Project Philosophy

The interesting part of Macropad OS isn't simply having 12 programmable keys.

It's exploring how far a small physical input surface can be extended through a more expressive logical input system.

**Meta layers are central to that vision.** Their ordered, state-dependent behavior is a fundamental part of the project.

The desktop companion application and onboard configuration interface should make that flexibility accessible without requiring users to manually reason through every possible state transition.

Quantum layers are an interesting possibility for pushing the system further, but shouldn't dictate the initial architecture or delay V1.

The project should avoid prematurely constraining itself to conventional macropad behavior while keeping the underlying system understandable, deterministic, and configurable.

**Current priority:** Establish the core meta layer model and a configuration architecture that supports both desktop and onboard configuration.

---

## 12. Important Context for Future Work

- This project is separate from the user's other hardware and software projects.
- The Adafruit MacroPad RP2040 is the confirmed target hardware.
- Meta layers are a fundamental V1 requirement.
- Quantum layers are experimental and explicitly not a V1 requirement.
- A desktop companion configuration application is planned.
- A simpler onboard configuration interface is also planned.
- The desktop application should not be mandatory for basic configuration.
- Most implementation details remain undecided.
- Suggestions must be distinguished from established decisions.
- Avoid turning the early concept into an unnecessarily rigid specification.

The project is intentionally still exploratory.