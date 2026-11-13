<script lang="ts">
  import { KEY_GROUPS, captureKey, type Capture } from "./keys";

  let {
    allowMedia = true,
    onpick,
    onclose,
  }: {
    /** Offer media/launch keys (fine for macro steps, not for the key field). */
    allowMedia?: boolean;
    onpick: (name: string) => void;
    onclose: () => void;
  } = $props();

  let query = $state("");
  let captured = $state<Capture | null>(null);
  let captureEl = $state<HTMLButtonElement>();

  const groups = $derived.by(() => {
    const q = query.trim().toLowerCase();
    return KEY_GROUPS.filter((g) => allowMedia || !g.media)
      .map((g) => ({
        ...g,
        keys: q ? g.keys.filter((k) => k.name.includes(q) || k.label.toLowerCase().includes(q)) : g.keys,
      }))
      .filter((g) => g.keys.length);
  });

  const usable = $derived(captured && "name" in captured && (allowMedia || !captured.media) ? captured.name : null);

  function onCaptureKey(e: KeyboardEvent) {
    e.preventDefault();
    e.stopPropagation();
    captured = captureKey(e);
  }

  function pick(name: string) {
    onpick(name);
    onclose();
  }

  $effect(() => {
    captureEl?.focus();
  });
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop" onclick={onclose}>
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="dialog" onclick={(e) => e.stopPropagation()}>
    <div class="head">
      <h2>Pick a key</h2>
      <button onclick={onclose} title="Close">✕</button>
    </div>

    <div class="capture-row">
      <button bind:this={captureEl} class="capture" onkeydown={onCaptureKey} title="Click here, then press a key or combination">
        {#if !captured}
          <span class="muted">Click here and press a key or combination…</span>
        {:else if "unknown" in captured}
          <span class="warn">That key isn't supported ({captured.unknown})</span>
        {:else}
          <span class="mono big">{captured.name}</span>
          {#if captured.media && !allowMedia}<span class="warn"> — media keys go in the Media type</span>{/if}
        {/if}
      </button>
      <button class="primary" disabled={!usable} onclick={() => usable && pick(usable)}>Use</button>
    </div>
    <p class="muted small">
      Keys your desktop grabs (e.g. Super, Alt+Tab, media keys) may not reach the app; pick those from the list below.
    </p>

    <input class="search" placeholder="Search keys (e.g. numpad, page, f13, copy)" bind:value={query} />

    <div class="groups">
      {#each groups as g}
        <section>
          <h3>{g.title}</h3>
          {#if g.note}<p class="muted small">{g.note}</p>{/if}
          <div class="keys">
            {#each g.keys as k}
              <button class="key" onclick={() => pick(k.name)} title={k.name}>
                <span>{k.label}</span>
                {#if k.label.toLowerCase() !== k.name}<span class="mono name">{k.name}</span>{/if}
              </button>
            {/each}
          </div>
        </section>
      {/each}
      {#if !groups.length}<p class="muted">No keys match "{query}".</p>{/if}
    </div>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgb(0 0 0 / 0.5);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 100;
  }
  .dialog {
    width: min(760px, calc(100vw - 32px));
    max-height: calc(100vh - 64px);
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 16px;
    background: var(--panel);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    box-shadow: 0 10px 40px rgb(0 0 0 / 0.4);
  }
  .head {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  .head h2,
  h3 {
    margin: 0;
  }
  h3 {
    font-size: 13px;
    margin-bottom: 4px;
  }
  .capture-row {
    display: flex;
    gap: 8px;
  }
  .capture {
    flex: 1;
    min-height: 48px;
    text-align: left;
    border-style: dashed;
  }
  .capture:focus {
    border-color: var(--accent);
    border-style: solid;
    outline: none;
  }
  .big {
    font-size: 18px;
  }
  .search {
    width: 100%;
  }
  .groups {
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding-right: 4px;
  }
  .keys {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }
  .key {
    display: flex;
    flex-direction: column;
    align-items: center;
    padding: 4px 9px;
    min-width: 40px;
    font-size: 13px;
  }
  .name {
    font-size: 10px;
    color: var(--muted);
  }
  .muted {
    color: var(--muted);
  }
  .small {
    margin: 0 0 4px;
    font-size: 12px;
  }
  .warn {
    color: var(--warn);
  }
</style>
