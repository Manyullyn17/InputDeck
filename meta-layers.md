# Meta Layer Semantics

Decisions agreed during design discussion (2026-11). This defines V1 behavior; anything listed under "Later" is explicitly out of scope for V1.

## Layers

- Layer 0 is the base layer, active when no layer key is held.
- Every layer has its own keymap covering all 12 physical keys.
- Any physical key can be a layer key on any layer, regardless of what that key does on the parent layer.
- Layer keys are opt-in per layer. A layer has no child layers unless they are explicitly configured.
- A layer key is hold-only: the target layer is active while the key is held. A layer key sends nothing by itself.

## Ordering and combos

- Layer transitions form a graph. Paths are ordered by default: m1 → m2 and m2 → m1 can lead to different layers.
- Multiple paths may lead to the same layer. An order-independent combo (QMK-style: only "both held" matters) is expressed by having both orderings target the same layer. The configuration offers this as a per-combination setting.
- Each layer has one fixed fallthrough parent, independent of the path used to reach it.

## Non-layer keys

Per layer, a button is bound to one of: nothing, an action, a layer key, or inherit. New layers start fully inherit (transparent).

Resolution order for a button on the current layer:

1. The layer's own explicit binding (including an explicit "nothing").
2. The button's global (layer-independent) action, if one is set.
3. Fallthrough through the fixed parent chain.

Fallthrough passes down actions only. An inherited layer key resolves to nothing, so e.g. pressing the m1 key again on L3 (after releasing it) does nothing. Making such a key a new layer key is a quantum-layer idea, not V1.

## State model

The runtime state is a stack of the currently held layer keys, each entry being `(physical key, layer entered)`.

- **Press a layer key:** push an entry. The current layer becomes that entry's layer.
- **Release a layer key:** remove its entry from wherever it sits in the stack.
- **Current layer:** the layer of the top entry, or layer 0 if the stack is empty.

(An earlier formulation kept released entries in place and popped them lazily. It behaves identically, but released entries could pile up with cyclic configs, so they are removed immediately.)

Consequences (with m1 → L1, then m3 → L3 from L1):

| Sequence | Result |
| --- | --- |
| hold m1, hold m3, release m3 | L1 |
| hold m1, hold m3, release m1 | L3 (stay; m3 is still held) |
| …then release m3 | L0 |
| hold m1, m3, m5; release m3 | L5 |
| …then release m5 | L1 (m3 popped as released, m1 still held) |

Releasing an ancestor layer key never changes the current layer while a descendant layer key is still held. This is always on, not configurable.

Stack depth is bounded by the number of simultaneously held keys, so cyclic configurations cannot cause unbounded state.

## Held keys across layer switches

A non-layer key keeps the action it was pressed with until it is physically released, even if the layer changes in between. Its new meaning applies only to the next press.

## Later (not V1)

- Tap/hold behavior for any key type (e.g. tap for an action or a layer toggle, hold for a layer). The engine should accept timestamps and timer events from the start so this can be added without restructuring.
- Quantum layers (key releases that trigger transitions beyond the release rule above).
