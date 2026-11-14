// Key catalog for the pickers (keys.json; a Rust test checks every name
// parses), plus capturing a pressed key combination as notation.
import catalog from "./keys.json";

export interface KeyEntry {
  name: string;
  label: string;
  /** KeyboardEvent.code of the physical key, if it has one. */
  code?: string;
}

export interface KeyGroup {
  title: string;
  keys: KeyEntry[];
  media?: boolean;
  /** Mouse buttons and scrolling (macro steps only; bindings have their own types). */
  mouse?: boolean;
  note?: string;
}

export const KEY_GROUPS: KeyGroup[] = catalog;

const MODIFIER_NAMES = ["ctrl", "shift", "alt", "gui", "rctrl", "rshift", "ralt", "rgui"];

/** Plain key names (no media, mouse or modifiers), for autocomplete. */
export const KEY_NAMES: string[] = KEY_GROUPS.filter((g) => !g.media && !g.mouse)
  .flatMap((g) => g.keys.map((k) => k.name))
  .filter((n) => !MODIFIER_NAMES.includes(n));

/** Media and launch keys, for the media picker. */
export const MEDIA_KEYS: KeyEntry[] = KEY_GROUPS.filter((g) => g.media).flatMap((g) => g.keys);

const BY_CODE = new Map(KEY_GROUPS.flatMap((g) => g.keys.filter((k) => k.code).map((k) => [k.code!, k] as const)));

/** Older `code` names some engines (WebKitGTK) still report. */
const CODE_ALIASES: Record<string, string> = {
  OSLeft: "MetaLeft",
  OSRight: "MetaRight",
  VolumeUp: "AudioVolumeUp",
  VolumeDown: "AudioVolumeDown",
  VolumeMute: "AudioVolumeMute",
};

/** Whether `code` is a modifier key (Ctrl, Shift, Alt, Super; either side). */
export function isModifierCode(code: string): boolean {
  const entry = BY_CODE.get(CODE_ALIASES[code] ?? code);
  return !!entry && MODIFIER_NAMES.includes(entry.name);
}

export type Capture = { name: string; media: boolean } | { unknown: string };

/** Notation for a key event, e.g. "ctrl+shift+t", "rshift", "vol+".
 * `held` are the codes currently down, used in addition to the event's
 * modifier flags (WebKitGTK doesn't always set `metaKey` for Super). */
export function captureKey(e: KeyboardEvent, held: Iterable<string> = []): Capture {
  const entry = BY_CODE.get(CODE_ALIASES[e.code] ?? e.code);
  if (!entry) return { unknown: e.code || e.key };
  const media = KEY_GROUPS.some((g) => g.media && g.keys.includes(entry));
  if (media) return { name: entry.name, media: true };
  const altGr = e.getModifierState("AltGraph");
  const downMods = new Set(
    [...held].map((c) => BY_CODE.get(CODE_ALIASES[c] ?? c)?.name.replace(/^r/, "")).filter((n) => n && MODIFIER_NAMES.includes(n)),
  );
  const flag = (name: string, f: boolean) => (f || downMods.has(name) ? name : null);
  const mods = [
    altGr ? null : flag("ctrl", e.ctrlKey),
    flag("shift", e.shiftKey),
    altGr ? null : flag("alt", e.altKey),
    flag("gui", e.metaKey || e.getModifierState("Super") || e.getModifierState("OS")),
    altGr ? "ralt" : null,
  ].filter((m): m is string => m !== null);
  if (MODIFIER_NAMES.includes(entry.name)) {
    // A modifier on its own: keep its side, plus any other held modifiers.
    const generic = entry.name.replace(/^r/, "");
    const others = mods.filter((m) => m !== generic && m !== entry.name);
    return { name: [...others, entry.name].join("+"), media: false };
  }
  return { name: [...mods, entry.name].join("+"), media: false };
}
