// Typed wrappers around the Rust backend (src-tauri/src/lib.rs).
import { invoke } from "@tauri-apps/api/core";

export interface KeyColors {
  idle: string | null;
  held: string | null;
  active: string | null;
}
export type ColorSlot = keyof KeyColors;

export interface KeyView {
  raw: string;
  label: string;
  inherited: boolean;
  category: "layer" | "key" | "modified" | "media" | "macro" | "unbound";
  /** Effective idle color, "#rrggbb". */
  color: string;
  /** Per-key colors as written in the file. */
  colors: KeyColors;
}

export type Category = "layer_key" | "key" | "modified" | "media" | "macro" | "unbound";

export const CATEGORY_LABELS: Record<Category, string> = {
  layer_key: "Layer keys",
  key: "Keys",
  modified: "Keys with modifiers",
  media: "Media keys",
  macro: "Macros & system",
  unbound: "Unbound keys",
};

export interface PaletteEntry {
  category: Category;
  /** Set here (null = not set). */
  raw: string | null;
  /** What applies when not set: "#rrggbb" or "off". */
  inherited: string;
}

export interface SettingsView {
  name: string;
  leds_enabled: boolean | null;
  brightness: number | null;
  idle_level: number | null;
  held_style: string | null;
  dim_after: number | null;
  off_after: number | null;
  tap: number | null;
  char_delay: number | null;
  palette: PaletteEntry[];
  layout: {
    /** As written; null = default ("us"). */
    base: string | null;
    /** What "system" means on this computer. */
    detected: { Ok: string } | { Err: string };
    chars: CharOverride[];
  };
  indicators: { led: IndicatorLed; key: number | null; color: string | null }[];
  /** Global actions: keys 1-12, then the knob ("" = none). */
  global: string[];
  defaults: {
    brightness: number;
    idle_level: number;
    held_style: string;
    dim_after: number;
    off_after: number;
    tap: number;
    char_delay: number;
    layout: string;
  };
}

export interface CharOverride {
  ch: string;
  keys: string[];
}
export type IndicatorLed = "caps_lock" | "num_lock" | "scroll_lock";

export type SettingValue = boolean | number | string | null;

export interface LayerView {
  id: string;
  name: string;
  parent: string | null;
  /** 12 keys, then the encoder press (index 12). */
  keys: KeyView[];
  encoder_cw: string;
  encoder_ccw: string;
  palette: PaletteEntry[];
}

export type MacroKind = "once" | "while_held" | "toggle";
export type StopMode = "finish" | "immediate";

/** A macro's editable fields. */
export interface MacroEdit {
  name: string;
  kind: MacroKind;
  stop: StopMode;
  /** ms between typed characters; null = global default. */
  char_delay: number | null;
  /** Steps in notation: "ctrl+c", "down:shift", "delay:50", "text:hi". */
  steps: string[];
}

export interface MacroView extends MacroEdit {
  id: string;
  /** Where it's bound, e.g. "Tools · key 4". */
  used_by: string[];
}

export interface DocumentView {
  name: string;
  layers: LayerView[];
  macros: MacroView[];
  settings: SettingsView;
  error: string | null;
}

export interface View {
  path: string | null;
  dirty: boolean;
  can_undo: boolean;
  can_redo: boolean;
  doc: DocumentView | null;
}

export interface SimKey {
  label: string;
  held: boolean;
  highlighted: boolean;
  color: string;
}

export interface SimState {
  path: string[];
  keys: SimKey[];
  log: string[];
  busy: boolean;
}

export interface DeviceInfo {
  format_version: number;
  compatible: boolean;
  active: number;
  profiles: { name: string; layers: number }[];
}

export const ENCODER_PRESS = 12;

export const api = {
  getView: () => invoke<View>("get_view"),
  openFile: (path: string) => invoke<View>("open_file", { path }),
  newFile: () => invoke<View>("new_file"),
  saveFile: (path: string | null) => invoke<View>("save_file", { path }),
  setKey: (layer: string, index: number, binding: string) => invoke<View>("set_key", { layer, index, binding }),
  setEncoder: (layer: string, direction: "cw" | "ccw", binding: string) =>
    invoke<View>("set_encoder", { layer, direction, binding }),
  setLayerName: (layer: string, name: string) => invoke<View>("set_layer_name", { layer, name }),
  setLayerParent: (layer: string, parent: string | null) => invoke<View>("set_layer_parent", { layer, parent }),
  addLayer: (id: string, name: string) => invoke<View>("add_layer", { id, name }),
  removeLayer: (id: string) => invoke<View>("remove_layer", { id }),
  addMacro: (id: string, name: string) => invoke<View>("add_macro", { id, name }),
  removeMacro: (id: string) => invoke<View>("remove_macro", { id }),
  setMacro: (id: string, settings: MacroEdit) => invoke<View>("set_macro", { id, settings }),

  setKeyColor: (layer: string, index: number, slot: ColorSlot, color: string | null) =>
    invoke<View>("set_key_color", { layer, index, slot, color }),
  setPalette: (layer: string | null, category: Category, color: string | null) =>
    invoke<View>("set_palette", { layer, category, color }),
  setSetting: (path: string, value: SettingValue) => invoke<View>("set_setting", { path, value }),
  setLayout: (name: string | null) => invoke<View>("set_layout", { name }),
  setLayoutChar: (ch: string, keys: string[] | null) => invoke<View>("set_layout_char", { ch, keys }),
  setIndicator: (led: IndicatorLed, key: number | null, color: string) => invoke<View>("set_indicator", { led, key, color }),
  setGlobal: (index: number, action: string | null) => invoke<View>("set_global", { index, action }),
  undo: () => invoke<View>("undo"),
  redo: () => invoke<View>("redo"),

  simState: () => invoke<SimState>("sim_state"),
  simReset: () => invoke<SimState>("sim_reset"),
  simPress: (index: number) => invoke<SimState>("sim_press", { index }),
  simRelease: (index: number) => invoke<SimState>("sim_release", { index }),
  simRotate: (clockwise: boolean) => invoke<SimState>("sim_rotate", { clockwise }),
  simTick: (ms: number) => invoke<SimState>("sim_tick", { ms }),
  simRunMacro: (id: string) => invoke<SimState>("sim_run_macro", { id }),

  deviceInfo: () => invoke<DeviceInfo>("device_info"),
  deviceUpload: (slot: number | null) => invoke<DeviceInfo>("device_upload", { slot }),
  deviceRemove: (index: number) => invoke<DeviceInfo>("device_remove", { index }),
  deviceActivate: (index: number) => invoke<DeviceInfo>("device_activate", { index }),
  devicePull: (index: number) => invoke<View>("device_pull", { index }),
};

/** Backend errors arrive as strings. */
export function message(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : JSON.stringify(e);
}

/** A readable id from a display name, unique among `taken`. */
export function makeId(name: string, fallback: string, taken: string[]): string {
  const base =
    name
      .toLowerCase()
      .normalize("NFKD")
      .replace(/[^a-z0-9]+/g, "_")
      .replace(/^_+|_+$/g, "") || fallback;
  let id = base;
  for (let n = 2; taken.includes(id) || id === "none"; n++) id = `${base}_${n}`;
  return id;
}
