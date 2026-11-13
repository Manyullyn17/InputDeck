<script lang="ts">
  import { api, CATEGORY_LABELS, type IndicatorLed, type SettingValue, type SettingsView, type View } from "./api";
  import ColorField from "./ColorField.svelte";
  import KeyPicker from "./KeyPicker.svelte";

  let {
    settings,
    report,
  }: {
    settings: SettingsView;
    /** Runs an edit; errors are shown by the page. */
    report: (edit: Promise<View>) => void;
  } = $props();

  const d = $derived(settings.defaults);
  const onset = (path: string, value: SettingValue) => report(api.setSetting(path, value));

  // ---- Layout ----
  const PRESETS = ["us", "de", "system"];
  const layoutChoice = $derived(
    settings.layout.base === null ? "" : PRESETS.includes(settings.layout.base) ? settings.layout.base : "custom",
  );
  let customLayout = $state(false);
  const detected = $derived("Ok" in settings.layout.detected ? settings.layout.detected.Ok : null);
  let newChar = $state("");
  let newKeys = $state("");

  function chooseLayout(value: string) {
    customLayout = value === "custom";
    if (!customLayout) report(api.setLayout(value || null));
  }

  function addChar() {
    const keys = newKeys.trim().split(/\s+/).filter(Boolean);
    if (!newChar || !keys.length) return;
    report(api.setLayoutChar(newChar, keys));
    newChar = "";
    newKeys = "";
  }

  // ---- Indicators ----
  const LED_LABELS: Record<IndicatorLed, string> = { caps_lock: "Caps Lock", num_lock: "Num Lock", scroll_lock: "Scroll Lock" };

  // ---- Global keys ----
  let pickingGlobal = $state<number | null>(null);
  const globalLabel = (i: number) => (i === 12 ? "Knob" : `Key ${i + 1}`);

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

{#if pickingGlobal !== null}
  <KeyPicker
    onpick={(name) => pickingGlobal !== null && report(api.setGlobal(pickingGlobal, name))}
    onclose={() => (pickingGlobal = null)}
  />
{/if}

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
          onchange={(c) => report(api.setPalette(null, entry.category, c))}
        />
      {/each}
    </div>
  </section>

  <section>
    <h3>Lock indicators</h3>
    <p class="help">Light a key while the computer's lock is on. Overrides that key's color.</p>
    {#each settings.indicators as ind}
      <div class="row">
        <span>{LED_LABELS[ind.led]}</span>
        <select
          value={ind.key ?? ""}
          onchange={(e) => {
            const v = e.currentTarget.value;
            report(api.setIndicator(ind.led, v ? Number(v) : null, ind.color ?? "#ffffff"));
          }}
        >
          <option value="">Not shown</option>
          {#each Array.from({ length: 12 }, (_, i) => i + 1) as k}<option value={k}>Key {k}</option>{/each}
        </select>
        {#if ind.key !== null}
          <input
            type="color"
            class="color"
            value={ind.color ?? "#ffffff"}
            onchange={(e) => report(api.setIndicator(ind.led, ind.key, e.currentTarget.value))}
            title="Indicator color"
          />
        {/if}
      </div>
    {/each}
  </section>

  <section>
    <h3>Global keys</h3>
    <p class="help">Used on every layer that leaves the key on Inherit. Keys and media keys only.</p>
    <div class="globals">
      {#each settings.global as action, i}
        <label class="global">
          <span>{globalLabel(i)}</span>
          <input
            class="mono"
            value={action}
            placeholder="—"
            onchange={(e) => report(api.setGlobal(i, e.currentTarget.value || null))}
          />
          <button class="small" type="button" onclick={() => (pickingGlobal = i)} title="Pick or press a key">⌨</button>
        </label>
      {/each}
    </div>
  </section>

  <section>
    <h3>Typing layout</h3>
    <p class="help">
      Type text steps become key presses for this keyboard layout when you upload. Choose the layout of the computer you type
      into.
    </p>
    <label class="row">
      <span>Layout</span>
      <select value={customLayout ? "custom" : layoutChoice} onchange={(e) => chooseLayout(e.currentTarget.value)}>
        <option value="">Default ({d.layout.toUpperCase()})</option>
        <option value="us">US</option>
        <option value="de">German</option>
        <option value="system">This computer's layout{detected ? ` (${detected})` : " (not detected)"}</option>
        <option value="custom">Other XKB layout…</option>
      </select>
    </label>
    {#if customLayout || layoutChoice === "custom"}
      <label class="row">
        <span>XKB name</span>
        <input
          class="mono"
          value={layoutChoice === "custom" ? settings.layout.base : "xkb:"}
          placeholder="xkb:fr, xkb:de(nodeadkeys)…"
          onchange={(e) => report(api.setLayout(e.currentTarget.value))}
        />
      </label>
      <p class="help">A file in /usr/share/X11/xkb/symbols, optionally with a variant in parentheses.</p>
    {/if}
    {#if layoutChoice === "system"}
      <p class="help">
        {#if detected}Resolved when uploading, on this computer: {detected}.{:else}{"Err" in settings.layout.detected ? settings.layout.detected.Err : ""}{/if}
      </p>
    {/if}

    <h4>Character overrides</h4>
    <p class="help">
      For characters the layout can't type, or to type them differently. Several keys are pressed one after another (e.g. a dead
      key, then a letter).
    </p>
    {#if settings.layout.chars.length}
      <ul class="chars">
        {#each settings.layout.chars as c}
          <li>
            <span class="ch">{c.ch}</span>
            <span class="mono keys">{c.keys.join(" → ")}</span>
            <button class="small" onclick={() => report(api.setLayoutChar(c.ch, null))} title="Remove">✕</button>
          </li>
        {/each}
      </ul>
    {/if}
    <div class="row add-char">
      <input class="ch-input" bind:value={newChar} placeholder="é" maxlength="2" />
      <input class="mono grow" bind:value={newKeys} placeholder="keys, e.g. ralt+e  or  ' e" onkeydown={(e) => e.key === "Enter" && addChar()} />
      <button onclick={addChar} disabled={!newChar || !newKeys.trim()}>Add</button>
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
    grid-template-columns: repeat(auto-fill, minmax(min(100%, 280px), 1fr));
    gap: 10px 18px;
  }
  .small {
    padding: 2px 8px;
    font-size: 12px;
  }
  h4 {
    margin: 8px 0 0;
    font-size: 13px;
  }
  .color {
    width: 44px;
    height: 30px;
    padding: 2px;
  }
  .globals {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(min(100%, 150px), 1fr));
    gap: 6px 10px;
  }
  .global {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .global span {
    width: 44px;
    flex-shrink: 0;
    font-size: 12px;
    color: var(--muted);
  }
  .global input {
    flex: 1;
    min-width: 0;
  }
  .chars {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .chars li {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .ch {
    width: 32px;
    text-align: center;
    font-size: 18px;
  }
  .keys {
    flex: 1;
    font-size: 12px;
  }
  .ch-input {
    width: 48px;
    text-align: center;
  }
  .grow {
    flex: 1;
    min-width: 0;
  }
</style>
