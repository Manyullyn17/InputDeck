// Binding notation <-> structured form, for the binding editor. The Rust
// side (engine/src/notation.rs) is the authority and validates everything;
// this only splits strings into editable parts.

import { MEDIA_KEYS } from "./keys";

export type Kind =
  | "inherit"
  | "none"
  | "key"
  | "media"
  | "layer"
  | "toggle"
  | "latch"
  | "repeat"
  | "macro"
  | "system";

export interface Binding {
  kind: Kind;
  /** Key name for key/latch/repeat ("" for modifier-only). */
  key: string;
  mods: string[];
  layer: string;
  macro: string;
  media: string;
  system: string;
  /** 1-based profile number for the "profile" system function. */
  profile: number;
  interval: number;
}

export const KINDS: { kind: Kind; label: string }[] = [
  { kind: "key", label: "Key" },
  { kind: "media", label: "Media" },
  { kind: "layer", label: "Hold layer" },
  { kind: "toggle", label: "Toggle layer" },
  { kind: "latch", label: "Latch" },
  { kind: "repeat", label: "Repeat" },
  { kind: "macro", label: "Macro" },
  { kind: "system", label: "System" },
  { kind: "inherit", label: "Inherit" },
  { kind: "none", label: "Nothing" },
];

export const MODS = ["ctrl", "shift", "alt", "gui", "rctrl", "rshift", "ralt", "rgui"];
const MOD_ALIASES: Record<string, string> = {
  control: "ctrl", lctrl: "ctrl", lshift: "shift", option: "alt", lalt: "alt",
  super: "gui", win: "gui", cmd: "gui", lgui: "gui", altgr: "ralt",
};

/** Media keys as {value, label}, from the key catalog. */
export const MEDIA = MEDIA_KEYS.map((k) => ({ value: k.name, label: k.label }));
/** Other accepted spellings, mapped to the names above. */
const MEDIA_ALIASES: Record<string, string> = { play: "playpause", volup: "vol+", voldown: "vol-" };

export const SYSTEM = [
  { value: "settings", label: "Open settings menu" },
  { value: "profile", label: "Switch to profile…" },
  { value: "profile:next", label: "Next profile" },
  { value: "profile:prev", label: "Previous profile" },
  { value: "bootloader", label: "Reboot to bootloader" },
];

export { KEY_NAMES } from "./keys";

export const DEFAULT_REPEAT_MS = 100;

function empty(kind: Kind): Binding {
  return { kind, key: "", mods: [], layer: "", macro: "", media: "playpause", system: "settings", profile: 1, interval: DEFAULT_REPEAT_MS };
}

function splitKey(text: string): { key: string; mods: string[] } {
  const parts = text.split("+");
  // A trailing "+" belongs to the key name (e.g. "vol+") — not used for keys.
  const mods: string[] = [];
  let key = text;
  if (parts.length > 1 && parts[parts.length - 1] !== "") {
    key = parts[parts.length - 1];
    for (const m of parts.slice(0, -1)) mods.push(MOD_ALIASES[m] ?? m);
  }
  if (MODS.includes(MOD_ALIASES[key] ?? key)) {
    mods.push(MOD_ALIASES[key] ?? key);
    key = "";
  }
  return { key, mods };
}

export function parse(raw: string): Binding {
  const t = raw.trim();
  const l = t.toLowerCase();
  if (l === "_") return empty("inherit");
  if (l === "" || l === "none") return empty("none");
  if (l !== "profile" && SYSTEM.some((s) => s.value === l)) return { ...empty("system"), system: l };
  const profile = /^profile:\s*(\d+)$/.exec(l);
  if (profile) return { ...empty("system"), system: "profile", profile: parseInt(profile[1], 10) };
  const media = MEDIA_ALIASES[l] ?? l;
  if (MEDIA.some((m) => m.value === media)) return { ...empty("media"), media };
  const prefixed = (p: string) => (t.startsWith(p) ? t.slice(p.length).trim() : null);
  let rest: string | null;
  if ((rest = prefixed("layer:")) !== null) return { ...empty("layer"), layer: rest };
  if ((rest = prefixed("toggle:")) !== null) return { ...empty("toggle"), layer: rest };
  if ((rest = prefixed("macro:")) !== null) return { ...empty("macro"), macro: rest };
  if ((rest = prefixed("latch:")) !== null) return { ...empty("latch"), ...splitKey(rest.toLowerCase()) };
  if ((rest = prefixed("repeat:")) !== null) {
    const [action, ms] = rest.split("@");
    const interval = ms ? parseInt(ms, 10) : DEFAULT_REPEAT_MS;
    return { ...empty("repeat"), ...splitKey(action.toLowerCase()), interval: isNaN(interval) ? DEFAULT_REPEAT_MS : interval };
  }
  return { ...empty("key"), ...splitKey(l) };
}

function keyText(b: Binding): string {
  const mods = MODS.filter((m) => b.mods.includes(m));
  return b.key ? [...mods, b.key].join("+") : mods.join("+");
}

export function format(b: Binding): string {
  switch (b.kind) {
    case "inherit": return "_";
    case "none": return "none";
    case "key": return keyText(b);
    case "media": return b.media;
    case "layer": return `layer:${b.layer}`;
    case "toggle": return `toggle:${b.layer}`;
    case "latch": return `latch:${keyText(b)}`;
    case "repeat": return `repeat:${keyText(b)}@${b.interval}`;
    case "macro": return `macro:${b.macro}`;
    case "system": return b.system === "profile" ? `profile:${b.profile}` : b.system;
  }
}

/** Switch kind, keeping compatible fields. */
export function withKind(b: Binding, kind: Kind): Binding {
  return { ...b, kind };
}
