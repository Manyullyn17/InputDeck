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
  note?: string;
}

export const KEY_GROUPS: KeyGroup[] = catalog;

const MODIFIER_NAMES = ["ctrl", "shift", "alt", "gui", "rctrl", "rshift", "ralt", "rgui"];

/** Plain key names (no media, no modifiers), for autocomplete. */
export const KEY_NAMES: string[] = KEY_GROUPS.filter((g) => !g.media)
  .flatMap((g) => g.keys.map((k) => k.name))
  .filter((n) => !MODIFIER_NAMES.includes(n));

/** Media and launch keys, for the media picker. */
export const MEDIA_KEYS: KeyEntry[] = KEY_GROUPS.filter((g) => g.media).flatMap((g) => g.keys);

const BY_CODE = new Map(KEY_GROUPS.flatMap((g) => g.keys.filter((k) => k.code).map((k) => [k.code!, k] as const)));

export type Capture = { name: string; media: boolean } | { unknown: string };

/** Notation for a key event, e.g. "ctrl+shift+t", "rshift", "vol+". */
export function captureKey(e: KeyboardEvent): Capture {
  const entry = BY_CODE.get(e.code);
  if (!entry) return { unknown: e.code || e.key };
  const media = KEY_GROUPS.some((g) => g.media && g.keys.includes(entry));
  if (media) return { name: entry.name, media: true };
  const altGr = e.getModifierState("AltGraph");
  const held = [
    e.ctrlKey && !altGr ? "ctrl" : null,
    e.shiftKey ? "shift" : null,
    e.altKey && !altGr ? "alt" : null,
    e.metaKey ? "gui" : null,
    altGr ? "ralt" : null,
  ].filter((m): m is string => m !== null);
  if (MODIFIER_NAMES.includes(entry.name)) {
    // A modifier on its own: keep its side, plus any other held modifiers.
    const generic = entry.name.replace(/^r/, "");
    const others = held.filter((m) => m !== generic && m !== entry.name);
    return { name: [...others, entry.name].join("+"), media: false };
  }
  return { name: [...held, entry.name].join("+"), media: false };
}
