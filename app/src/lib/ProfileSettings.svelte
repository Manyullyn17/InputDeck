<script lang="ts">
  import { CATEGORY_LABELS, type Category, type SettingValue, type SettingsView } from "./api";
  import ColorField from "./ColorField.svelte";

  let {
    settings,
    onset,
    onpalette,
  }: {
    settings: SettingsView;
    onset: (path: string, value: SettingValue) => void;
    onpalette: (category: Category, color: string | null) => void;
  } = $props();

  const d = $derived(settings.defaults);

  /** A number field's value: empty = default (null). */
  function num(e: Event & { currentTarget: HTMLInputElement }): number | null {
    const v = e.currentTarget.value.trim();
    return v === "" ? null : Math.max(0, Math.round(Number(v)));
  }

  function duration(s: number): string {
    if (s === 0) return "never";
    if (s % 60 === 0) return `${s / 60} min`;
    return `${s} s`;
  }
</script>

<div class="settings">
  <h2>Profile settings</h2>

  <section>
    <label class="row">
      <span>Name</span>
      <input
        value={settings.name}
        placeholder="Shown on the device when switching profiles"
        maxlength="20"
        onchange={(e) => onset("name", e.currentTarget.value)}
      />
    </label>
  </section>

  <section>
    <h3>LEDs</h3>
    <label class="row">
      <span>Enabled</span>
      <input
        type="checkbox"
        checked={settings.leds_enabled ?? true}
        onchange={(e) => onset("leds.enabled", e.currentTarget.checked ? null : false)}
      />
    </label>
    <label class="row">
      <span>Brightness</span>
      <input
        type="range"
        min="0"
        max="100"
        value={settings.brightness ?? d.brightness}
        onchange={(e) => onset("leds.brightness", Number(e.currentTarget.value))}
      />
      <span class="val">{settings.brightness ?? d.brightness}%</span>
      <button class="small" disabled={settings.brightness === null} onclick={() => onset("leds.brightness", null)}>Default</button>
    </label>
    <label class="row">
      <span>Idle level</span>
      <input
        type="range"
        min="0"
        max="100"
        value={settings.idle_level ?? d.idle_level}
        onchange={(e) => onset("leds.idle_level", Number(e.currentTarget.value))}
      />
      <span class="val">{settings.idle_level ?? d.idle_level}%</span>
      <button class="small" disabled={settings.idle_level === null} onclick={() => onset("leds.idle_level", null)}>Default</button>
    </label>
    <p class="help">Idle keys glow at this share of the brightness; pressed keys light up fully.</p>
    <label class="row">
      <span>Pressed keys</span>
      <select value={settings.held_style ?? ""} onchange={(e) => onset("leds.held_style", e.currentTarget.value || null)}>
        <option value="">Default ({d.held_style})</option>
        <option value="brighten">Brighten</option>
        <option value="white">Turn white</option>
        <option value="invert">Invert color</option>
      </select>
    </label>
    <p class="help">Brightness and LEDs on/off can also be changed in the device's settings menu.</p>
  </section>

  <section>
    <h3>Colors by key type</h3>
    <p class="help">Used for keys without their own color. Layers can override these.</p>
    <div class="colors">
      {#each settings.palette as entry}
        <ColorField
          label={CATEGORY_LABELS[entry.category]}
          value={entry.raw}
          inherited={entry.inherited}
          unsetLabel="Default"
          onchange={(c) => onpalette(entry.category, c)}
        />
      {/each}
    </div>
  </section>

  <section>
    <h3>Idle</h3>
    <label class="row">
      <span>Dim after</span>
      <input type="number" min="0" placeholder={String(d.dim_after)} value={settings.dim_after ?? ""} onchange={(e) => onset("idle.dim_after", num(e))} />
      <span class="unit">seconds · {duration(settings.dim_after ?? d.dim_after)}</span>
    </label>
    <label class="row">
      <span>Off after</span>
      <input type="number" min="0" placeholder={String(d.off_after)} value={settings.off_after ?? ""} onchange={(e) => onset("idle.off_after", num(e))} />
      <span class="unit">seconds · {duration(settings.off_after ?? d.off_after)}</span>
    </label>
    <p class="help">0 = never. Empty fields use the default.</p>
  </section>

  <section>
    <h3>Macro timing</h3>
    <label class="row">
      <span>Tap length</span>
      <input type="number" min="1" placeholder={String(d.tap)} value={settings.tap ?? ""} onchange={(e) => onset("timing.tap", num(e))} />
      <span class="unit">ms a tapped or typed key is held</span>
    </label>
    <label class="row">
      <span>Typing delay</span>
      <input type="number" min="0" placeholder={String(d.char_delay)} value={settings.char_delay ?? ""} onchange={(e) => onset("timing.char_delay", num(e))} />
      <span class="unit">ms between typed characters</span>
    </label>
    <p class="help">Raise these if an app misses characters from Type text steps.</p>
  </section>
</div>

<style>
  .settings {
    display: flex;
    flex-direction: column;
    gap: 18px;
  }
  h2,
  h3 {
    margin: 0;
  }
  h3 {
    font-size: 14px;
    margin-bottom: 8px;
  }
  section {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .row > span:first-child {
    width: 100px;
    flex-shrink: 0;
    color: var(--muted);
  }
  .row input:not([type="checkbox"]):not([type="number"]),
  .row select {
    flex: 1;
    min-width: 0;
  }
  input[type="number"] {
    width: 90px;
  }
  .val {
    width: 42px;
    text-align: right;
  }
  .unit,
  .help {
    color: var(--muted);
    font-size: 12px;
  }
  .help {
    margin: 0;
  }
  .colors {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px 18px;
  }
  .small {
    padding: 2px 8px;
    font-size: 12px;
  }
</style>
