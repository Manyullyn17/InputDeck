//! Macropad OS desktop app backend: Tauri commands around the shared
//! config tooling (`mpcfg`): editing TOML documents, simulating the engine,
//! and talking to the device.

use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use macropad_engine::Config;
use macropad_engine::codec::{self, Bundle, FORMAT_VERSION};
use mpcfg::device::Device;
use mpcfg::document::{ConfigDocument, DocumentView};
use mpcfg::sim::{SimState, Simulator};
use mpcfg::to_toml;
use serde::Serialize;
use tauri::State;

#[derive(Default)]
struct AppState {
    doc: Option<ConfigDocument>,
    path: Option<PathBuf>,
    dirty: bool,
    sim: Option<Simulator>,
}

type Shared<'a> = State<'a, Mutex<AppState>>;
type CmdResult<T> = Result<T, String>;

#[derive(Serialize)]
struct View {
    path: Option<String>,
    dirty: bool,
    doc: Option<DocumentView>,
}

fn err(e: impl std::fmt::Display) -> String {
    format!("{e:#}")
}

fn view(s: &AppState) -> View {
    View { path: s.path.as_ref().map(|p| p.display().to_string()), dirty: s.dirty, doc: s.doc.as_ref().map(|d| d.view()) }
}

/// Keeps the simulator in step with the document after it changes.
fn refresh_sim(s: &mut AppState) {
    let config = s.doc.as_ref().and_then(|d| d.compiled().ok().cloned());
    match (&mut s.sim, config) {
        (Some(sim), Some(config)) => sim.set_config(config),
        (None, Some(config)) => s.sim = Some(Simulator::new(config)),
        _ => {}
    }
}

fn edit(state: &Shared, f: impl FnOnce(&mut ConfigDocument) -> anyhow::Result<()>) -> CmdResult<View> {
    let mut s = state.lock().unwrap();
    let doc = s.doc.as_mut().ok_or("no file open")?;
    f(doc).map_err(err)?;
    s.dirty = true;
    refresh_sim(&mut s);
    Ok(view(&s))
}

// ---- Files ----

#[tauri::command]
fn get_view(state: Shared) -> View {
    view(&state.lock().unwrap())
}

#[tauri::command]
fn open_file(state: Shared, path: String) -> CmdResult<View> {
    let text = fs::read_to_string(&path).map_err(|e| format!("reading {path}: {e}"))?;
    let doc = ConfigDocument::parse(&text).map_err(err)?;
    let mut s = state.lock().unwrap();
    s.doc = Some(doc);
    s.path = Some(PathBuf::from(path));
    s.dirty = false;
    s.sim = None;
    refresh_sim(&mut s);
    Ok(view(&s))
}

#[tauri::command]
fn new_file(state: Shared) -> CmdResult<View> {
    let text = "name = \"New profile\"\n\n[layers.base]\nname = \"Base\"\n";
    let mut s = state.lock().unwrap();
    s.doc = Some(ConfigDocument::parse(text).map_err(err)?);
    s.path = None;
    s.dirty = true;
    s.sim = None;
    refresh_sim(&mut s);
    Ok(view(&s))
}

/// Saves to `path`, or to the current file if `path` is `None`.
#[tauri::command]
fn save_file(state: Shared, path: Option<String>) -> CmdResult<View> {
    let mut s = state.lock().unwrap();
    let target = path.map(PathBuf::from).or_else(|| s.path.clone()).ok_or("no file name")?;
    let text = s.doc.as_ref().ok_or("no file open")?.text();
    fs::write(&target, text).map_err(|e| format!("writing {}: {e}", target.display()))?;
    s.path = Some(target);
    s.dirty = false;
    Ok(view(&s))
}

#[tauri::command]
fn get_text(state: Shared) -> CmdResult<String> {
    Ok(state.lock().unwrap().doc.as_ref().ok_or("no file open")?.text())
}

// ---- Editing ----

#[tauri::command]
fn set_key(state: Shared, layer: String, index: usize, binding: String) -> CmdResult<View> {
    edit(&state, |d| d.set_key(&layer, index, &binding))
}

#[tauri::command]
fn set_encoder(state: Shared, layer: String, direction: String, binding: String) -> CmdResult<View> {
    edit(&state, |d| d.set_encoder(&layer, &direction, &binding))
}

#[tauri::command]
fn set_layer_name(state: Shared, layer: String, name: String) -> CmdResult<View> {
    edit(&state, |d| d.set_layer_name(&layer, &name))
}

#[tauri::command]
fn set_layer_parent(state: Shared, layer: String, parent: Option<String>) -> CmdResult<View> {
    edit(&state, |d| d.set_layer_parent(&layer, parent.as_deref()))
}

#[tauri::command]
fn add_layer(state: Shared, id: String, name: String) -> CmdResult<View> {
    edit(&state, |d| d.add_layer(&id, &name))
}

#[tauri::command]
fn remove_layer(state: Shared, id: String) -> CmdResult<View> {
    edit(&state, |d| d.remove_layer(&id))
}

// ---- Simulator ----

fn with_sim(state: &Shared, f: impl FnOnce(&mut Simulator)) -> CmdResult<SimState> {
    let mut s = state.lock().unwrap();
    let sim = s.sim.as_mut().ok_or("the config has errors; fix them to simulate")?;
    f(sim);
    Ok(sim.state())
}

#[tauri::command]
fn sim_state(state: Shared) -> CmdResult<SimState> {
    with_sim(&state, |_| {})
}

#[tauri::command]
fn sim_reset(state: Shared) -> CmdResult<SimState> {
    let mut s = state.lock().unwrap();
    s.sim = None;
    refresh_sim(&mut s);
    s.sim.as_ref().map(Simulator::state).ok_or_else(|| "the config has errors; fix them to simulate".into())
}

#[tauri::command]
fn sim_press(state: Shared, index: usize) -> CmdResult<SimState> {
    with_sim(&state, |sim| sim.press(index))
}

#[tauri::command]
fn sim_release(state: Shared, index: usize) -> CmdResult<SimState> {
    with_sim(&state, |sim| sim.release(index))
}

#[tauri::command]
fn sim_rotate(state: Shared, clockwise: bool) -> CmdResult<SimState> {
    with_sim(&state, |sim| sim.rotate(clockwise))
}

#[tauri::command]
fn sim_tick(state: Shared, ms: u64) -> CmdResult<SimState> {
    with_sim(&state, |sim| sim.tick(ms))
}

// ---- Device ----

#[derive(Serialize)]
struct DeviceProfile {
    name: String,
    layers: usize,
}

#[derive(Serialize)]
struct DeviceInfo {
    format_version: u16,
    compatible: bool,
    active: usize,
    profiles: Vec<DeviceProfile>,
}

fn read_bundle(dev: &mut Device) -> anyhow::Result<Option<Bundle>> {
    match dev.get_config() {
        Ok(bytes) => Ok(Some(codec::decode(&bytes).map_err(|e| anyhow::anyhow!("stored config: {e:?}"))?)),
        Err(e) if e.to_string().contains("no config stored") => Ok(None),
        Err(e) => Err(e),
    }
}

fn label(config: &Config, i: usize) -> String {
    if config.name.is_empty() { format!("Profile {}", i + 1) } else { config.name.clone() }
}

#[tauri::command]
async fn device_info() -> CmdResult<DeviceInfo> {
    let mut dev = Device::open(None).map_err(err)?;
    let info = dev.info().map_err(err)?;
    let compatible = info.format_version == FORMAT_VERSION;
    let bundle = if compatible { read_bundle(&mut dev).map_err(err)? } else { None };
    let (active, profiles) = match bundle {
        Some(b) => (
            b.active as usize,
            b.profiles
                .iter()
                .enumerate()
                .map(|(i, c)| DeviceProfile { name: label(c, i), layers: c.layers.len() })
                .collect(),
        ),
        None => (0, Vec::new()),
    };
    Ok(DeviceInfo { format_version: info.format_version, compatible, active, profiles })
}

fn put_bundle(dev: &mut Device, bundle: &Bundle) -> anyhow::Result<()> {
    dev.put_config(&codec::encode(bundle))
}

/// Uploads the open document into profile `slot` (replacing it), or as a
/// new profile if `slot` is `None`. The uploaded profile becomes active.
#[tauri::command]
async fn device_upload(state: Shared<'_>, slot: Option<usize>) -> CmdResult<DeviceInfo> {
    let config = {
        let s = state.lock().unwrap();
        let doc = s.doc.as_ref().ok_or("no file open")?;
        doc.compiled().map_err(|e| format!("the config has errors: {e}"))?.clone()
    };
    let mut dev = Device::open(None).map_err(err)?;
    let mut bundle = read_bundle(&mut dev).map_err(err)?.unwrap_or(Bundle { active: 0, profiles: Vec::new() });
    let index = match slot {
        Some(i) if i < bundle.profiles.len() => {
            bundle.profiles[i] = config;
            i
        }
        Some(i) => return Err(format!("no profile {i} on the device")),
        None => {
            bundle.profiles.push(config);
            bundle.profiles.len() - 1
        }
    };
    bundle.active = index as u8;
    put_bundle(&mut dev, &bundle).map_err(err)?;
    drop(dev);
    device_info().await
}

#[tauri::command]
async fn device_remove(index: usize) -> CmdResult<DeviceInfo> {
    let mut dev = Device::open(None).map_err(err)?;
    let mut bundle = read_bundle(&mut dev).map_err(err)?.ok_or("no config stored on the device")?;
    if bundle.profiles.len() < 2 {
        return Err("the last profile can't be removed".into());
    }
    if index >= bundle.profiles.len() {
        return Err(format!("no profile {index}"));
    }
    bundle.profiles.remove(index);
    let active = bundle.active as usize;
    bundle.active = if active == index { 0 } else if active > index { active as u8 - 1 } else { active as u8 };
    put_bundle(&mut dev, &bundle).map_err(err)?;
    drop(dev);
    device_info().await
}

#[tauri::command]
async fn device_activate(index: usize) -> CmdResult<DeviceInfo> {
    let mut dev = Device::open(None).map_err(err)?;
    let mut bundle = read_bundle(&mut dev).map_err(err)?.ok_or("no config stored on the device")?;
    if index >= bundle.profiles.len() {
        return Err(format!("no profile {index}"));
    }
    bundle.active = index as u8;
    put_bundle(&mut dev, &bundle).map_err(err)?;
    drop(dev);
    device_info().await
}

/// Opens a device profile as a new, unsaved document.
#[tauri::command]
async fn device_pull(state: Shared<'_>, index: usize) -> CmdResult<View> {
    let mut dev = Device::open(None).map_err(err)?;
    let bundle = read_bundle(&mut dev).map_err(err)?.ok_or("no config stored on the device")?;
    let config = bundle.profiles.get(index).ok_or_else(|| format!("no profile {index}"))?;
    let doc = ConfigDocument::parse(&to_toml::write(config)).map_err(err)?;
    let mut s = state.lock().unwrap();
    s.doc = Some(doc);
    s.path = None;
    s.dirty = true;
    s.sim = None;
    refresh_sim(&mut s);
    Ok(view(&s))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(Mutex::new(AppState::default()))
        .invoke_handler(tauri::generate_handler![
            get_view,
            open_file,
            new_file,
            save_file,
            get_text,
            set_key,
            set_encoder,
            set_layer_name,
            set_layer_parent,
            add_layer,
            remove_layer,
            sim_state,
            sim_reset,
            sim_press,
            sim_release,
            sim_rotate,
            sim_tick,
            device_info,
            device_upload,
            device_remove,
            device_activate,
            device_pull,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
