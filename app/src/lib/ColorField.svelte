<script lang="ts">
  let {
    label,
    value,
    inherited = "",
    unsetLabel = "Inherit",
    help = "",
    onchange,
  }: {
    label: string;
    /** As written in the file ("#rrggbb", "off"); null = not set. */
    value: string | null;
    /** What applies when unset ("#rrggbb", "off", or "" for automatic). */
    inherited?: string;
    /** Button text for clearing the value ("Inherit", "Default", "Auto"). */
    unsetLabel?: string;
    help?: string;
    onchange: (color: string | null) => void;
  } = $props();

  const shown = $derived(value ?? inherited);
  const isOff = $derived(shown.toLowerCase() === "off");
  const isHex = $derived(/^#[0-9a-f]{6}$/i.test(shown));
  let picker = $state<HTMLInputElement>();
</script>

<div class="field" title={help}>
  <button
    class="swatch"
    class:unset={value === null}
    class:off={isOff}
    class:auto={!isOff && !isHex}
    style:--c={isHex ? shown : "transparent"}
    onclick={() => picker?.click()}
    title="Choose a color"
    aria-label="Choose a color for {label}"
  ></button>
  <input
    bind:this={picker}
    class="native"
    type="color"
    value={isHex ? shown : "#ffffff"}
    onchange={(e) => onchange(e.currentTarget.value)}
    tabindex="-1"
  />
  <div class="text">
    <span class="label">{label}</span>
    <span class="value mono">
      {#if value !== null}{value}{:else if inherited}<span class="muted">{unsetLabel.toLowerCase()}: {inherited}</span>{:else}<span class="muted">{unsetLabel.toLowerCase()}</span>{/if}
    </span>
  </div>
  <div class="btns">
    <button class="small" class:on={value?.toLowerCase() === "off"} onclick={() => onchange("off")} title="LED off">Off</button>
    <button class="small" disabled={value === null} onclick={() => onchange(null)}>{unsetLabel}</button>
  </div>
</div>

<style>
  .field {
    display: flex;
    align-items: center;
    gap: 10px;
    position: relative;
  }
  .swatch {
    width: 34px;
    height: 34px;
    flex-shrink: 0;
    padding: 0;
    border-radius: 8px;
    border: 2px solid var(--border);
    background: var(--c);
  }
  .swatch.unset {
    border-style: dashed;
    opacity: 0.75;
  }
  .swatch.off {
    background: linear-gradient(135deg, transparent 45%, var(--muted) 45% 55%, transparent 55%), var(--bg);
  }
  .swatch.auto {
    background: repeating-linear-gradient(45deg, var(--panel-2) 0 5px, var(--bg) 5px 10px);
  }
  .native {
    position: absolute;
    left: 0;
    top: 0;
    width: 34px;
    height: 34px;
    opacity: 0;
    pointer-events: none;
  }
  .text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .label {
    font-size: 13px;
  }
  .value {
    font-size: 11px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .muted {
    color: var(--muted);
  }
  .btns {
    display: flex;
    gap: 4px;
  }
  .small {
    padding: 2px 8px;
    font-size: 12px;
  }
  .small.on {
    border-color: var(--accent);
  }
</style>
