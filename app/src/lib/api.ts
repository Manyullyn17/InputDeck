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

export interface MacroView {
  id: string;
  name: string;
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

  simState: () => invoke<SimState>("sim_state"),
  simReset: () => invoke<SimState>("sim_reset"),
  simPress: (index: number) => invoke<SimState>("sim_press", { index }),
  simRelease: (index: number) => invoke<SimState>("sim_release", { index }),
  simRotate: (clockwise: boolean) => invoke<SimState>("sim_rotate", { clockwise }),
  simTick: (ms: number) => invoke<SimState>("sim_tick", { ms }),

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
