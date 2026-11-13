<script lang="ts">
  import { api, message, type SimState } from "./api";

  let { revision }: { revision: number } = $props();

  let sim = $state<SimState | null>(null);
  let error = $state<string | null>(null);
  /** Keys held with right-click (stay down until right-clicked again). */
  let sticky = $state<Set<number>>(new Set());

  async function run(p: Promise<SimState>) {
    try {
      sim = await p;
      error = null;
    } catch (e) {
      sim = null;
      error = message(e);
    }
  }

  // Refresh after edits (the backend already swapped in the new config).
  $effect(() => {
    revision;
    sticky = new Set();
    run(api.simState());
  });

  // Keep simulated time running while macros/repeats are active.
  $effect(() => {
    if (!sim?.busy) return;
    const timer = setInterval(() => run(api.simTick(20)), 20);
    return () => clearInterval(timer);
  });

  function down(i: number, e: MouseEvent) {
    if (e.button !== 0 || sticky.has(i)) return;
    run(api.simPress(i));
  }
  function up(i: number) {
    if (sticky.has(i) || !sim?.keys[i].held) return;
    run(api.simRelease(i));
  }
  function toggleSticky(i: number, e: MouseEvent) {
    e.preventDefault();
    const next = new Set(sticky);
    if (next.has(i)) {
      next.delete(i);
      run(api.simRelease(i));
    } else {
      next.add(i);
      run(api.simPress(i));
    }
    sticky = next;
  }
  function reset() {
    sticky = new Set();
    run(api.simReset());
  }
</script>

<section class="sim">
  <div class="head">
    <h2>Simulator</h2>
    <button onclick={reset}>Reset</button>
  </div>
  {#if error}
    <p class="error">{error}</p>
  {:else if sim}
    <div class="oled mono">
      <div class="path">{sim.path.join(">")}</div>
      <div class="cells">
        {#each sim.keys.slice(0, 12) as k}
          <span class:inv={k.highlighted}>{k.label}</span>
        {/each}
      </div>
    </div>

    <div class="knob-row">
      <button onclick={() => run(api.simRotate(false))} title="Turn left">⟲</button>
      <button
        class="knob"
        class:held={sim.keys[12].held}
        onmousedown={(e) => down(12, e)}
        onmouseup={() => up(12)}
        onmouseleave={() => up(12)}
        oncontextmenu={(e) => toggleSticky(12, e)}
        title="Knob press"
      >{sim.keys[12].label || "●"}</button>
      <button onclick={() => run(api.simRotate(true))} title="Turn right">⟳</button>
    </div>

    <div class="pad">
      {#each sim.keys.slice(0, 12) as k, i}
        <button
          class="pkey"
          class:held={k.held}
          class:sticky={sticky.has(i)}
          style:--led={k.color}
          onmousedown={(e) => down(i, e)}
          onmouseup={() => up(i)}
          onmouseleave={() => up(i)}
          oncontextmenu={(e) => toggleSticky(i, e)}
        >{k.label}</button>
      {/each}
    </div>
    <p class="hint">Click to press, right-click to hold (for chords).</p>

    <div class="log mono">
      {#each sim.log.toReversed() as line}<div>{line}</div>{/each}
      {#if !sim.log.length}<div class="muted">Output appears here.</div>{/if}
    </div>
  {/if}
</section>

<style>
  .sim {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .head {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  .head h2 {
    margin: 0;
  }
  .oled {
    background: #000;
    color: #cfe8ff;
    border-radius: 6px;
    padding: 6px 8px 8px;
    font-size: 11px;
    line-height: 1.25;
  }
  .path {
    border-bottom: 1px solid #cfe8ff;
    padding-bottom: 3px;
    margin-bottom: 4px;
    white-space: nowrap;
    overflow: hidden;
  }
  .cells {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 1px;
  }
  .cells span {
    padding: 1px 3px;
    white-space: nowrap;
    overflow: hidden;
    min-height: 15px;
  }
  .cells .inv {
    background: #cfe8ff;
    color: #000;
  }
  .knob-row {
    display: grid;
    grid-template-columns: 1fr 1.4fr 1fr;
    gap: 6px;
  }
  .knob {
    border-radius: 999px;
  }
  .pad {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 6px;
  }
  .pkey {
    height: 46px;
    font-weight: 600;
    background: color-mix(in srgb, var(--led) 30%, var(--panel-2));
    border-color: color-mix(in srgb, var(--led) 60%, var(--border));
    overflow: hidden;
    white-space: nowrap;
    user-select: none;
  }
  .held {
    background: color-mix(in srgb, var(--led) 75%, var(--panel-2));
    transform: translateY(1px);
  }
  .sticky {
    outline: 2px dashed var(--accent);
  }
  .hint,
  .muted {
    margin: 0;
    color: var(--muted);
    font-size: 12px;
  }
  .log {
    height: 150px;
    overflow-y: auto;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 6px 8px;
    font-size: 12px;
  }
  .error {
    color: var(--warn);
    margin: 0;
  }
</style>
