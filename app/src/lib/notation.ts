// Binding notation <-> structured form, for the binding editor. The Rust
// side (engine/src/notation.rs) is the authority and validates everything;
// this only splits strings into editable parts.

import { MEDIA_KEYS } from "./keys";

export type Kind =
  | "inherit"
  | "none"
  | "key"
  | "media"
  | "mouse"
  | "scroll"
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
  /** Mouse button: "left", "right", "middle", "back", "forward", "6".."16". */
  mouse: string;
  scrollDir: ScrollDir;
  /** Notches per press (1..255). */
  scrollSteps: number;
}

export type ScrollDir = "up" | "down" | "left" | "right";

export const KINDS: { kind: Kind; label: string }[] = [
  { kind: "key", label: "Key" },
  { kind: "media", label: "Media" },
  { kind: "mouse", label: "Mouse button" },
  { kind: "scroll", label: "Scroll" },
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

const NAMED_MOUSE = ["left", "right", "middle", "back", "forward"];
export const MOUSE_BUTTONS = [
  { value: "left", label: "Left click" },
  { value: "right", label: "Right click" },
  { value: "middle", label: "Middle click" },
  { value: "back", label: "Back (button 4)" },
  { value: "forward", label: "Forward (button 5)" },
  ...Array.from({ length: 11 }, (_, i) => ({ value: String(i + 6), label: `Button ${i + 6}` })),
];
/** Buttons past 5 work on Linux; other systems may ignore them. */
export const isExtraMouseButton = (b: string) => !NAMED_MOUSE.includes(b);

export const SCROLL_DIRS: { value: ScrollDir; label: string }[] = [
  { value: "up", label: "Up" },
  { value: "down", label: "Down" },
  { value: "left", label: "Left" },
  { value: "right", label: "Right" },
];

/** Canonical mouse button name ("1" → "left"), or null if not a button. */
function mouseName(b: string): string | null {
  if (NAMED_MOUSE.includes(b)) return b;
  const n = /^\d+$/.test(b) ? parseInt(b, 10) : NaN;
  if (n >= 1 && n <= 5) return NAMED_MOUSE[n - 1];
  if (n >= 6 && n <= 16) return String(n);
  return null;
}

/** Parses "mouse:…" / "scroll:…" action text (lowercase), or null. */
export function parseMouse(l: string): Pick<Binding, "kind" | "mouse" | "scrollDir" | "scrollSteps"> | null {
  const m = /^mouse:\s*(\w+)$/.exec(l);
  const name = m && mouseName(m[1]);
  if (name) return { kind: "mouse", mouse: name, scrollDir: "down", scrollSteps: 1 };
  const s = /^scroll:\s*(up|down|left|right)(?::\s*(\d+))?$/.exec(l);
  if (s) {
    const steps = s[2] ? parseInt(s[2], 10) : 1;
    if (steps >= 1 && steps <= 255) return { kind: "scroll", mouse: "left", scrollDir: s[1] as ScrollDir, scrollSteps: steps };
  }
  return null;
}

export function formatMouse(b: Pick<Binding, "kind" | "mouse" | "scrollDir" | "scrollSteps">): string {
  if (b.kind === "mouse") return `mouse:${b.mouse}`;
  return b.scrollSteps > 1 ? `scroll:${b.scrollDir}:${b.scrollSteps}` : `scroll:${b.scrollDir}`;
}

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
  return { kind, key: "", mods: [], layer: "", macro: "", media: "playpause", system: "settings", profile: 1, interval: DEFAULT_REPEAT_MS, mouse: "left", scrollDir: "down", scrollSteps: 1 };
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
  const mouse = parseMouse(l);
  if (mouse) return { ...empty(mouse.kind), ...mouse };
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
    case "mouse":
    case "scroll": return formatMouse(b);
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
