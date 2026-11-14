<script lang="ts">
  import { open, save } from "@tauri-apps/plugin-dialog";
  import { tick } from "svelte";
  import {
    api,
    makeId,
    message,
    CATEGORY_LABELS,
    ENCODER_PRESS,
    type ColorSlot,
    type LayerView,
    type MacroEdit,
    type MacroView,
    type View,
  } from "$lib/api";
  import BindingEditor from "$lib/BindingEditor.svelte";
  import ColorField from "$lib/ColorField.svelte";
  import ProfileSettings from "$lib/ProfileSettings.svelte";
  import DevicePanel from "$lib/DevicePanel.svelte";
  import KeyGrid from "$lib/KeyGrid.svelte";
  import MacroEditor from "$lib/MacroEditor.svelte";
  import Simulator from "$lib/Simulator.svelte";

  let view = $state<View>({ path: null, dirty: false, can_undo: false, can_redo: false, doc: null });
  let layerId = $state("base");
  let target = $state<number | "cw" | "ccw" | null>(0);
  let toast = $state<string | null>(null);
  let tab = $state<"sim" | "device">("sim");
  /** Bumped on every document change so the simulator refreshes. */
  let revision = $state(0);
  let newLayerName = $state("");
  let newMacroName = $state("");
  /** The macro being edited; null shows the selected layer. */
  let macroId = $state<string | null>(null);
  /** Show the profile settings in the center instead of a layer/macro. */
  let settingsOpen = $state(false);
  let simulator = $state<ReturnType<typeof Simulator>>();

  const doc = $derived(view.doc);
  const layer = $derived<LayerView | undefined>(doc?.layers.find((l) => l.id === layerId) ?? doc?.layers[0]);
  const macro = $derived<MacroView | undefined>(macroId ? doc?.macros.find((m) => m.id === macroId) : undefined);
  const fileName = $derived(view.path ? view.path.split("/").pop() : doc ? "unsaved" : "");

  // Layers as a tree by fallthrough parent, for the sidebar.
  const tree = $derived.by(() => {
    const layers = doc?.layers ?? [];
    const out: { layer: LayerView; depth: number }[] = [];
    const visit = (parent: string | null, depth: number, seen: Set<string>) => {
      for (const l of layers.filter((x) => x.parent === parent && !seen.has(x.id))) {
        seen.add(l.id);
        out.push({ layer: l, depth });
        visit(l.id, depth + 1, seen);
      }
    };
    const seen = new Set<string>();
    visit(null, 0, seen);
    for (const l of layers) if (!seen.has(l.id)) out.push({ layer: l, depth: 0 }); // cycles / "none"
    return out;
  });

  function setView(v: View) {
    view = v;
    revision++;
    if (v.doc && !v.doc.layers.some((l) => l.id === layerId)) layerId = v.doc.layers[0]?.id ?? "base";
    if (macroId && !v.doc?.macros.some((m) => m.id === macroId)) macroId = null;
  }

  function flash(text: string) {
    toast = text;
    setTimeout(() => (toast === text ? (toast = null) : null), 3000);
  }

  async function call(p: Promise<View>): Promise<string | null> {
    try {
      setView(await p);
      return null;
    } catch (e) {
      return message(e);
    }
  }

  async function report(p: Promise<View>) {
    const e = await call(p);
    if (e) flash(e);
  }

  async function openFile() {
    const path = await open({ filters: [{ name: "InputDeck config", extensions: ["toml"] }] });
    if (typeof path === "string") await report(api.openFile(path));
  }

  async function saveAs() {
    const path = await save({ filters: [{ name: "InputDeck config", extensions: ["toml"] }], defaultPath: "profile.toml" });
    if (path) {
      await report(api.saveFile(path));
      flash("Saved");
    }
  }

  async function saveFile() {
    if (!doc) return;
    if (!view.path) return saveAs();
    await report(api.saveFile(null));
    flash("Saved");
  }

  async function addLayer() {
    const name = newLayerName.trim();
    if (!name || !doc) return;
    const id = makeId(name, "layer", doc.layers.map((l) => l.id));
    const e = await call(api.addLayer(id, name));
    if (e) return flash(e);
    layerId = id;
    macroId = null;
    newLayerName = "";
  }

  async function addMacro() {
    const name = newMacroName.trim();
    if (!name || !doc) return;
    const id = makeId(name, "macro", doc.macros.map((m) => m.id));
    const e = await call(api.addMacro(id, name));
    if (e) return flash(e);
    macroId = id;
    newMacroName = "";
  }

  const heldStyle = $derived(doc?.settings.held_style ?? doc?.settings.defaults.held_style ?? "brighten");
  const COLOR_SLOTS: { slot: ColorSlot; label: string; unset: string; help: string }[] = [
    { slot: "idle", label: "Normal", unset: "Layer color", help: "The key's color. Unset: the color for its key type on this layer." },
    { slot: "held", label: "While pressed", unset: "Auto", help: "Unset: the normal color, brightened / white / inverted (see Profile settings)." },
    { slot: "active", label: "While on", unset: "Auto", help: "While a toggle, latch, repeat or toggle macro on this key is on." },
  ];

  function setKeyColor(slot: ColorSlot, color: string | null) {
    if (layer && typeof target === "number" && target < 12) report(api.setKeyColor(layer.id, target, slot, color));
  }

  /** Show the file id next to a name only when it isn't just the name in lowercase. */
  function showId(name: string, id: string): boolean {
    return !!name && id.replace(/_\d+$/, "") !== makeId(name, "", []);
  }

  function selectLayer(id: string) {
    layerId = id;
    macroId = null;
    settingsOpen = false;
  }

  function applyMacro(settings: MacroEdit): Promise<string | null> {
    return macroId ? call(api.setMacro(macroId, settings)) : Promise.resolve(null);
  }

  async function runMacro() {
    if (!macroId) return;
    tab = "sim";
    await tick();
    await simulator?.runMacro(macroId);
  }

  /** Text fields keep their own undo (Ctrl+Z) behavior. */
  function inTextField(e: KeyboardEvent): boolean {
    const t = e.target as HTMLElement | null;
    return !!t && (t.tagName === "TEXTAREA" || t.tagName === "SELECT" || (t.tagName === "INPUT" && !["checkbox", "range", "color"].includes((t as HTMLInputElement).type)));
  }

  function onKeydown(e: KeyboardEvent) {
    const mod = e.ctrlKey || e.metaKey;
    if (mod && !inTextField(e) && doc) {
      const k = e.key.toLowerCase();
      if (k === "z" && !e.shiftKey) {
        e.preventDefault();
        if (view.can_undo) report(api.undo());
        return;
      }
      if ((k === "z" && e.shiftKey) || k === "y") {
        e.preventDefault();
        if (view.can_redo) report(api.redo());
        return;
      }
    }
    if ((e.ctrlKey || e.metaKey) && e.key === "s") {
      e.preventDefault();
      saveFile();
    }
    if ((e.ctrlKey || e.metaKey) && e.key === "o") {
      e.preventDefault();
      openFile();
    }
  }

  const editorTitle = $derived(
    target === "cw" ? "Turn right" : target === "ccw" ? "Turn left" : target === ENCODER_PRESS ? "Knob press" : `Key ${(target as number) + 1}`,
  );
  const editorRaw = $derived(
    !layer || target === null ? "" : target === "cw" ? layer.encoder_cw : target === "ccw" ? layer.encoder_ccw : layer.keys[target].raw,
  );

  function applyBinding(binding: string): Promise<string | null> {
    if (!layer || target === null) return Promise.resolve(null);
    if (target === "cw" || target === "ccw") return call(api.setEncoder(layer.id, target, binding));
    return call(api.setKey(layer.id, target, binding));
  }

  $effect(() => {
    api.getView().then(setView);
  });
</script>

<svelte:window onkeydown={onKeydown} />

<div class="app">
  <header>
    <div class="brand">InputDeck</div>
    <button onclick={() => report(api.newFile())}>New</button>
    <button onclick={openFile}>Open…</button>
    <button onclick={saveFile} disabled={!doc}>Save</button>
    <button onclick={saveAs} disabled={!doc}>Save as…</button>
    <span class="sep"></span>
    <button onclick={() => report(api.undo())} disabled={!view.can_undo} title="Undo (Ctrl+Z)">↶ Undo</button>
    <button onclick={() => report(api.redo())} disabled={!view.can_redo} title="Redo (Ctrl+Shift+Z)">↷ Redo</button>
    {#if doc}
      <div class="file">
        <span class="mono">{fileName}</span>
        {#if view.dirty}<span class="dirty" title="Unsaved changes">●</span>{/if}
        {#if doc.name}<span class="muted">· {doc.name}</span>{/if}
      </div>
    {/if}
    {#if toast}<div class="toast">{toast}</div>{/if}
  </header>

  {#if doc?.error}
    <div class="banner mono">{doc.error}</div>
  {/if}

  {#if !doc}
    <div class="empty">
      <p>Open a profile (TOML) to start, or create a new one.</p>
      <div>
        <button class="primary" onclick={openFile}>Open…</button>
        <button onclick={() => report(api.newFile())}>New profile</button>
      </div>
    </div>
  {:else}
    <main>
      <aside class="layers">
        <button class="layer settings-btn" class:on={settingsOpen} onclick={() => (settingsOpen = true)}>⚙ Profile settings</button>
        <h2 class="section">Layers</h2>
        <ul>
          {#each tree as { layer: l, depth }}
            <li>
              <button
                class="layer"
                title="id: {l.id}"
                class:on={!macro && !settingsOpen && l.id === layer?.id}
                style:padding-left="{10 + depth * 16}px"
                onclick={() => selectLayer(l.id)}
              >
                {l.name || l.id}{#if showId(l.name, l.id)}<span class="id mono">{l.id}</span>{/if}
              </button>
            </li>
          {/each}
        </ul>
        <div class="add">
          <input placeholder="New layer name" bind:value={newLayerName} onkeydown={(e) => e.key === "Enter" && addLayer()} />
          <button onclick={addLayer} disabled={!newLayerName.trim()}>Add</button>
        </div>

        <h2 class="section">Macros</h2>
        <ul>
          {#each doc.macros as m}
            <li>
              <button
                class="layer"
                title="id: {m.id}"
                class:on={!settingsOpen && m.id === macroId}
                onclick={() => {
                  macroId = m.id;
                  settingsOpen = false;
                }}
              >
                {m.name || m.id}{#if showId(m.name, m.id)}<span class="id mono">{m.id}</span>{/if}
                {#if !m.used_by.length}<span class="muted" title="Not bound to any key"> ·</span>{/if}
              </button>
            </li>
          {/each}
          {#if !doc.macros.length}<li class="muted small">None yet.</li>{/if}
        </ul>
        <div class="add">
          <input placeholder="New macro name" bind:value={newMacroName} onkeydown={(e) => e.key === "Enter" && addMacro()} />
          <button onclick={addMacro} disabled={!newMacroName.trim()}>Add</button>
        </div>
      </aside>

      <section class="center">
        {#if settingsOpen}
          <div class="panel">
            <ProfileSettings settings={doc.settings} {report} />
          </div>
        {:else if macro}
          <div class="panel">
            <MacroEditor
              {macro}
              onapply={applyMacro}
              onrun={runMacro}
              ondelete={() => report(api.removeMacro(macro.id))}
            />
          </div>
        {:else if layer}
          <div class="props">
            <label>
              <span>Name</span>
              <input
                value={layer.name}
                placeholder={layer.id}
                onchange={(e) => report(api.setLayerName(layer.id, e.currentTarget.value.trim()))}
              />
            </label>
            {#if layer.id !== "base"}
              <label>
                <span>Falls back to</span>
                <select value={layer.parent ?? "base"} onchange={(e) => report(api.setLayerParent(layer.id, e.currentTarget.value))}>
                  {#each doc.layers.filter((l) => l.id !== layer.id) as l}<option value={l.id}>{l.name || l.id}</option>{/each}
                  <option value="none">nothing</option>
                </select>
              </label>
              <button class="danger" onclick={() => report(api.removeLayer(layer.id))}>Delete layer</button>
            {/if}
          </div>

          <KeyGrid {layer} selected={target} onselect={(t) => (target = t)} />

          {#if target !== null}
            <div class="panel">
              <BindingEditor
                title={editorTitle}
                raw={editorRaw}
                layers={doc.layers}
                macros={doc.macros}
                actionsOnly={target === "cw" || target === "ccw"}
                onapply={applyBinding}
              />
              {#if typeof target === "number" && target < 12}
                {@const key = layer.keys[target]}
                <h3 class="sub">Key color</h3>
                <div class="color-list">
                  {#each COLOR_SLOTS as s}
                    <ColorField
                      label={s.label}
                      value={key.colors[s.slot]}
                      inherited={s.slot === "idle" ? key.color : ""}
                      unsetLabel={s.unset}
                      help={s.help}
                      onchange={(c) => setKeyColor(s.slot, c)}
                    />
                  {/each}
                </div>
                <p class="help">
                  Pressed keys {heldStyle === "white" ? "turn white" : heldStyle === "invert" ? "invert their color" : "brighten"} unless "While
                  pressed" is set. "While on" applies to toggles, latches, repeats and toggle macros.
                </p>
              {/if}
            </div>
          {/if}

          <details class="panel">
            <summary>Layer colors <span class="muted">— colors by key type on {layer.name || layer.id}</span></summary>
            <p class="help">
              Unset colors come from {layer.parent && layer.parent !== "none" ? "the layer this one falls back to" : "the profile settings"}.
            </p>
            <div class="color-grid">
              {#each layer.palette as entry}
                <ColorField
                  label={CATEGORY_LABELS[entry.category]}
                  value={entry.raw}
                  inherited={entry.inherited}
                  onchange={(c) => report(api.setPalette(layer.id, entry.category, c))}
                />
              {/each}
            </div>
          </details>
        {/if}
      </section>

      <aside class="side">
        <div class="tabs">
          <button class:on={tab === "sim"} onclick={() => (tab = "sim")}>Simulator</button>
          <button class:on={tab === "device"} onclick={() => (tab = "device")}>Device</button>
        </div>
        <div class="panel">
          {#if tab === "sim"}
            <Simulator {revision} bind:this={simulator} />
          {:else}
            <DevicePanel canUpload={!doc.error} onpulled={(v) => { setView(v); flash("Opened from the device — save to keep it"); }} />
          {/if}
        </div>
      </aside>
    </main>
  {/if}
</div>

<style>
  .app {
    height: 100vh;
    display: flex;
    flex-direction: column;
  }
  header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 16px;
    background: var(--panel);
    border-bottom: 1px solid var(--border);
  }
  .sep {
    width: 1px;
    align-self: stretch;
    margin: 0 4px;
    background: var(--border);
  }
  .brand {
    font-weight: 700;
    margin-right: 12px;
  }
  .file {
    margin-left: 12px;
    display: flex;
    gap: 6px;
    align-items: center;
  }
  .dirty {
    color: var(--warn);
  }
  .toast {
    margin-left: auto;
    padding: 4px 10px;
    background: var(--panel-2);
    border: 1px solid var(--border);
    border-radius: 6px;
  }
  .banner {
    padding: 8px 16px;
    background: color-mix(in srgb, var(--danger) 18%, var(--panel));
    color: var(--danger);
    white-space: pre-wrap;
    font-size: 12px;
  }
  .empty {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 12px;
    color: var(--muted);
  }
  main {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: 230px 1fr 340px;
  }
  aside,
  .center {
    overflow-y: auto;
    padding: 16px;
  }
  .layers {
    border-right: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .layers ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .layer {
    width: 100%;
    text-align: left;
    background: none;
    border-color: transparent;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .layer.on {
    background: var(--panel-2);
    border-color: var(--accent);
  }
  .add {
    margin-top: 4px;
    display: flex;
    gap: 6px;
  }
  .add input {
    flex: 1;
    min-width: 0;
  }
  .section {
    margin-top: 18px;
  }
  .id {
    margin-left: 8px;
    font-size: 11px;
    color: var(--muted);
  }
  .settings-btn {
    margin-bottom: 4px;
  }
  .sub {
    margin: 18px 0 8px;
    font-size: 14px;
  }
  .color-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .color-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(min(100%, 280px), 1fr));
    gap: 10px 18px;
    margin-top: 10px;
  }
  .help {
    margin: 8px 0 0;
    font-size: 12px;
    color: var(--muted);
  }
  summary {
    cursor: pointer;
    font-weight: 600;
  }
  summary .muted {
    font-weight: 400;
  }
  .small {
    font-size: 12px;
    padding: 4px 10px;
  }
  .center {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 18px;
  }
  .props {
    width: 100%;
    max-width: 520px;
    display: flex;
    gap: 10px;
    align-items: flex-end;
    flex-wrap: wrap;
  }
  .props label {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 12px;
    color: var(--muted);
  }
  .panel {
    width: 100%;
    max-width: 520px;
    background: var(--panel);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 14px;
  }
  .side {
    border-left: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .side .panel {
    max-width: none;
  }
  .tabs {
    display: flex;
    gap: 6px;
  }
  .tabs button.on {
    background: var(--accent);
    border-color: var(--accent);
    color: #fff;
  }
  .muted {
    color: var(--muted);
  }
</style>
