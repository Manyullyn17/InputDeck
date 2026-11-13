// Binding notation <-> structured form, for the binding editor. The Rust
// side (engine/src/notation.rs) is the authority and validates everything;
// this only splits strings into editable parts.

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

export const MEDIA = ["vol+", "vol-", "mute", "play", "next", "prev", "stop", "bri+", "bri-"];
export const SYSTEM = [
  { value: "settings", label: "Settings menu" },
  { value: "profile:next", label: "Next profile" },
  { value: "profile:prev", label: "Previous profile" },
  { value: "bootloader", label: "Bootloader" },
];

/** Key names offered as suggestions. */
export const KEY_NAMES = [
  ..."abcdefghijklmnopqrstuvwxyz0123456789".split(""),
  ...Array.from({ length: 24 }, (_, i) => `f${i + 1}`),
  "enter", "esc", "backspace", "tab", "space", "minus", "equal", "lbracket", "rbracket", "backslash",
  "semicolon", "quote", "grave", "comma", "period", "slash", "capslock", "printscreen", "scrolllock",
  "pause", "insert", "home", "pageup", "delete", "end", "pagedown", "right", "left", "down", "up",
  "numlock", "menu",
];

export const DEFAULT_REPEAT_MS = 100;

function empty(kind: Kind): Binding {
  return { kind, key: "", mods: [], layer: "", macro: "", media: "vol+", system: "settings", interval: DEFAULT_REPEAT_MS };
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
  if (SYSTEM.some((s) => s.value === l)) return { ...empty("system"), system: l };
  if (MEDIA.includes(l)) return { ...empty("media"), media: l };
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
    case "system": return b.system;
  }
}

/** Switch kind, keeping compatible fields. */
export function withKind(b: Binding, kind: Kind): Binding {
  return { ...b, kind };
}
