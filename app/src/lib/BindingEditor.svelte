<script lang="ts">
  import type { LayerView, MacroView } from "./api";
  import KeyPicker from "./KeyPicker.svelte";
  import { KEY_NAMES, KINDS, MEDIA, MODS, SYSTEM, format, parse, type Binding, type Kind } from "./notation";

  let {
    title,
    raw,
    layers,
    macros,
    actionsOnly = false,
    onapply,
  }: {
    title: string;
    raw: string;
    layers: LayerView[];
    macros: MacroView[];
    /** Encoder rotation: only keys/media (no layers, toggles, macros). */
    actionsOnly?: boolean;
    onapply: (binding: string) => Promise<string | null>;
  } = $props();

  let form = $state<Binding>(parse(""));
  let text = $state("");
  let error = $state<string | null>(null);
  let picking = $state(false);

  // Reset when another key is selected (or the file changes underneath).
  $effect(() => {
    form = parse(raw);
    text = raw;
    error = null;
  });

  const kinds = $derived(
    KINDS.filter((k) => !actionsOnly || ["key", "media", "inherit", "none"].includes(k.kind)),
  );
  const hasKey = $derived(["key", "latch", "repeat"].includes(form.kind));
  const changed = $derived(text.trim() !== raw.trim());

  function update(next: Partial<Binding>) {
    form = { ...form, ...next };
    text = format(form);
    error = null;
  }

  function setKind(kind: Kind) {
    const next: Partial<Binding> = { kind };
    if ((kind === "layer" || kind === "toggle") && !form.layer) next.layer = layers[0]?.id ?? "";
    if (kind === "macro" && !form.macro) next.macro = macros[0]?.id ?? "";
    update(next);
  }

  function toggleMod(m: string) {
    update({ mods: form.mods.includes(m) ? form.mods.filter((x) => x !== m) : [...form.mods, m] });
  }

  function onText() {
    form = parse(text);
    error = null;
  }

  async function apply() {
    error = await onapply(text.trim());
  }

  function layerLabel(l: LayerView) {
    return l.name ? `${l.name} (${l.id})` : l.id;
  }
</script>

<div class="editor">
  <h2>{title}</h2>

  <div class="kinds">
    {#each kinds as k}
      <button class:on={form.kind === k.kind} onclick={() => setKind(k.kind)}>{k.label}</button>
    {/each}
  </div>

  <div class="fields">
    {#if hasKey}
      <label class="row">
        <span>Key</span>
        <input
          list="key-names"
          value={form.key}
          placeholder="a, f5, enter… (empty: modifiers only)"
          oninput={(e) => update({ key: e.currentTarget.value.trim().toLowerCase() })}
        />
        <button class="pick" onclick={() => (picking = true)} title="Pick or press a key">⌨</button>
      </label>
      <div class="row">
        <span>Modifiers</span>
        <div class="mods">
          {#each MODS as m}
            <button class="chip" class:on={form.mods.includes(m)} onclick={() => toggleMod(m)}>{m}</button>
          {/each}
        </div>
      </div>
      {#if form.kind === "repeat"}
        <label class="row">
          <span>Every</span>
          <input
            type="number"
            min="2"
            step="10"
            value={form.interval}
            oninput={(e) => update({ interval: Math.max(2, parseInt(e.currentTarget.value, 10) || 2) })}
          />
          <span class="unit">ms</span>
        </label>
      {/if}
    {:else if form.kind === "media"}
      <label class="row">
        <span>Media key</span>
        <select value={form.media} onchange={(e) => update({ media: e.currentTarget.value })}>
          {#each MEDIA as m}<option value={m.value}>{m.label}</option>{/each}
        </select>
      </label>
    {:else if form.kind === "layer" || form.kind === "toggle"}
      <label class="row">
        <span>Layer</span>
        <select value={form.layer} onchange={(e) => update({ layer: e.currentTarget.value })}>
          {#each layers as l}<option value={l.id}>{layerLabel(l)}</option>{/each}
        </select>
      </label>
    {:else if form.kind === "macro"}
      <label class="row">
        <span>Macro</span>
        {#if macros.length}
          <select value={form.macro} onchange={(e) => update({ macro: e.currentTarget.value })}>
            {#each macros as m}<option value={m.id}>{m.name ? `${m.name} (${m.id})` : m.id}</option>{/each}
          </select>
        {:else}
          <span class="muted">No macros in this file yet.</span>
        {/if}
      </label>
    {:else if form.kind === "system"}
      <label class="row">
        <span>Function</span>
        <select value={form.system} onchange={(e) => update({ system: e.currentTarget.value })}>
          {#each SYSTEM as s}<option value={s.value}>{s.label}</option>{/each}
        </select>
      </label>
      {#if form.system === "profile"}
        <label class="row">
          <span>Profile</span>
          <input
            type="number"
            min="1"
            max="255"
            value={form.profile}
            oninput={(e) => update({ profile: Math.min(255, Math.max(1, parseInt(e.currentTarget.value, 10) || 1)) })}
          />
          <span class="unit">position in the device's profile list</span>
        </label>
      {/if}
    {:else if form.kind === "inherit"}
      <p class="muted">Uses the parent layer's binding (actions only; layer keys aren't inherited).</p>
    {:else}
      <p class="muted">The key does nothing on this layer.</p>
    {/if}
  </div>

  <label class="row text">
    <span>Notation</span>
    <input class="mono" bind:value={text} oninput={onText} onkeydown={(e) => e.key === "Enter" && apply()} />
  </label>

  {#if error}<p class="error">{error}</p>{/if}

  <div class="actions">
    <button onclick={() => { text = raw; form = parse(raw); error = null; }} disabled={!changed}>Revert</button>
    <button class="primary" onclick={apply} disabled={!changed}>Apply</button>
  </div>
</div>

{#if picking}
  <KeyPicker
    allowMedia={false}
    onpick={(name) => {
      const picked = parse(name);
      update({ key: picked.key, mods: picked.mods });
    }}
    onclose={() => (picking = false)}
  />
{/if}

<datalist id="key-names">
  {#each KEY_NAMES as k}<option value={k}></option>{/each}
</datalist>

<style>
  .editor {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .kinds {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .kinds button.on,
  .chip.on {
    background: var(--accent);
    border-color: var(--accent);
    color: #fff;
  }
  .fields {
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-height: 70px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .row > span:first-child {
    width: 78px;
    flex-shrink: 0;
    color: var(--muted);
  }
  .row input:not([type="number"]),
  .row select {
    flex: 1;
    min-width: 0;
  }
  .pick {
    padding: 3px 8px;
  }
  .row input[type="number"] {
    width: 90px;
  }
  .mods {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }
  .chip {
    padding: 2px 8px;
    font-size: 12px;
  }
  .unit,
  .muted {
    color: var(--muted);
  }
  .muted {
    margin: 0;
  }
  .error {
    margin: 0;
    color: var(--danger);
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
</style>
