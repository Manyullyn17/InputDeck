// Typed wrappers around the Rust backend (src-tauri/src/lib.rs).
import { invoke } from "@tauri-apps/api/core";

export interface KeyView {
  raw: string;
  label: string;
  inherited: boolean;
  category: "layer" | "key" | "modified" | "media" | "macro" | "unbound";
  color: string;
}

export interface LayerView {
  id: string;
  name: string;
  parent: string | null;
  /** 12 keys, then the encoder press (index 12). */
  keys: KeyView[];
  encoder_cw: string;
  encoder_ccw: string;
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
  error: string | null;
}

export interface View {
  path: string | null;
  dirty: boolean;
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
