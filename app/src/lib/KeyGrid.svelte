<script lang="ts">
  import { ENCODER_PRESS, type LayerView } from "./api";

  let {
    layer,
    selected,
    onselect,
  }: {
    layer: LayerView;
    /** Key index 0-12, or "cw"/"ccw" for encoder rotation. */
    selected: number | "cw" | "ccw" | null;
    onselect: (target: number | "cw" | "ccw") => void;
  } = $props();

  const keys = $derived(layer.keys.slice(0, 12));
  const press = $derived(layer.keys[ENCODER_PRESS]);
</script>

<div class="device">
  <div class="encoder">
    <button class="enc" class:sel={selected === "ccw"} onclick={() => onselect("ccw")} title="Turn left">
      <span class="hint">⟲</span>
      <span class="raw mono">{layer.encoder_ccw}</span>
    </button>
    <button
      class="enc knob"
      class:sel={selected === ENCODER_PRESS}
      class:inherited={press.inherited}
      onclick={() => onselect(ENCODER_PRESS)}
      title="Knob press"
    >
      <span class="label">{press.label || "—"}</span>
      <span class="raw mono">{press.raw}</span>
    </button>
    <button class="enc" class:sel={selected === "cw"} onclick={() => onselect("cw")} title="Turn right">
      <span class="hint">⟳</span>
      <span class="raw mono">{layer.encoder_cw}</span>
    </button>
  </div>

  <div class="grid">
    {#each keys as key, i}
      <button
        class="key cat-{key.category}"
        class:sel={selected === i}
        class:inherited={key.inherited}
        onclick={() => onselect(i)}
      >
        <span class="led" style:background={key.color}></span>
        <span class="num">{i + 1}</span>
        <span class="label">{key.label || "—"}</span>
        <span class="raw mono">{key.raw}</span>
      </button>
    {/each}
  </div>
</div>

<style>
  .device {
    display: flex;
    flex-direction: column;
    gap: 14px;
    width: 100%;
    max-width: 520px;
  }
  .encoder {
    display: grid;
    grid-template-columns: 1fr 1.3fr 1fr;
    gap: 10px;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 10px;
  }
  .key,
  .enc {
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 4px;
    min-height: 82px;
    padding: 14px 8px 10px;
    background: var(--panel);
    border: 2px solid var(--border);
    border-radius: var(--radius);
    overflow: hidden;
  }
  .enc {
    min-height: 58px;
  }
  .knob {
    border-radius: 999px;
  }
  .sel {
    border-color: var(--accent) !important;
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 25%, transparent);
  }
  .led {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    height: 5px;
  }
  .num {
    position: absolute;
    top: 8px;
    left: 9px;
    font-size: 11px;
    color: var(--muted);
  }
  .label {
    font-size: 17px;
    font-weight: 600;
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .hint {
    font-size: 18px;
    color: var(--muted);
  }
  .raw {
    font-size: 11px;
    color: var(--muted);
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .inherited .label {
    opacity: 0.45;
    font-weight: 400;
  }
  .cat-layer .label {
    color: #74a9ff;
  }
  .cat-macro .label {
    color: #ffd43b;
  }
  .cat-media .label {
    color: #69db7c;
  }
  .cat-modified .label {
    color: #c084fc;
  }
</style>
