<script lang="ts">
  import type { MacroEdit, MacroKind, MacroView, StopMode } from "./api";
  import KeyPicker from "./KeyPicker.svelte";
  import { KEY_NAMES } from "./keys";

  let {
    macro,
    onapply,
    onrun,
    ondelete,
  }: {
    macro: MacroView;
    onapply: (settings: MacroEdit) => Promise<string | null>;
    onrun: () => void;
    ondelete: () => void;
  } = $props();

  type StepType = "tap" | "down" | "up" | "delay" | "text";
  interface StepRow {
    type: StepType;
    value: string;
  }

  const STEP_TYPES: { type: StepType; label: string }[] = [
    { type: "tap", label: "Tap" },
    { type: "down", label: "Hold" },
    { type: "up", label: "Release" },
    { type: "text", label: "Type text" },
    { type: "delay", label: "Wait" },
  ];

  const KIND_INFO: { kind: MacroKind; label: string; help: string }[] = [
    { kind: "once", label: "Once", help: "Runs once per press. Pressing again while it runs is ignored." },
    { kind: "while_held", label: "While held", help: "Repeats while the key is held, then finishes the current run." },
    { kind: "toggle", label: "Toggle", help: "Press to start repeating, press again to stop." },
  ];

  function parseStep(s: string): StepRow {
    if (s.startsWith("text:")) return { type: "text", value: s.slice(5) };
    const t = s.trim();
    for (const type of ["down", "up", "tap", "delay"] as const) {
      if (t.toLowerCase().startsWith(type + ":")) return { type, value: t.slice(type.length + 1).trim() };
    }
    return { type: "tap", value: t };
  }

  function formatStep(r: StepRow): string {
    // Number inputs bind numbers, so normalize to a string first.
    const v = String(r.value ?? "");
    switch (r.type) {
      case "text": return `text:${v}`;
      case "tap": return v.trim();
      default: return `${r.type}:${v.trim()}`;
    }
  }

  let name = $state("");
  let kind = $state<MacroKind>("once");
  let stop = $state<StopMode>("finish");
  let charDelay = $state<number | null>(null);
  let rows = $state<StepRow[]>([]);
  let error = $state<string | null>(null);
  /** Step row whose key picker is open. */
  let picking = $state<number | null>(null);

  function load() {
    name = macro.name;
    kind = macro.kind;
    stop = macro.stop;
    charDelay = macro.char_delay;
    rows = macro.steps.map(parseStep);
    error = null;
  }

  // Reset when another macro is selected or the file changes underneath.
  $effect(() => {
    macro;
    load();
  });

  const edited = $derived<MacroEdit>({ name: name.trim(), kind, stop, char_delay: charDelay, steps: rows.map(formatStep) });
  const original = $derived<MacroEdit>({
    name: macro.name,
    kind: macro.kind,
    stop: macro.stop,
    char_delay: macro.char_delay,
    // Normalized like the edited steps, so equivalent spellings
    // ("tap:enter" vs "enter") don't count as changes.
    steps: macro.steps.map((s) => formatStep(parseStep(s))),
  });
  const changed = $derived(JSON.stringify(edited) !== JSON.stringify(original));
  const hasText = $derived(rows.some((r) => r.type === "text"));

  /** Keys held by "Hold" steps and never released (the device releases them when the macro ends). */
  const stillHeld = $derived.by(() => {
    const held = new Set<string>();
    for (const r of rows) {
      const k = String(r.value).trim().toLowerCase();
      if (r.type === "down") held.add(k);
      if (r.type === "up") held.delete(k);
    }
    return [...held].filter(Boolean);
  });

  function add(type: StepType) {
    rows = [...rows, { type, value: type === "delay" ? "100" : "" }];
  }
  function move(i: number, by: number) {
    const j = i + by;
    if (j < 0 || j >= rows.length) return;
    const next = [...rows];
    [next[i], next[j]] = [next[j], next[i]];
    rows = next;
  }
  function remove(i: number) {
    rows = rows.filter((_, k) => k !== i);
  }

  async function apply(): Promise<boolean> {
    error = await onapply(edited);
    return error === null;
  }

  /** Runs the macro as shown, applying pending edits first. */
  async function run() {
    if (changed && !(await apply())) return;
    onrun();
  }

  function confirmDelete() {
    const where = macro.used_by.length ? `\n\nIt's bound to: ${macro.used_by.join(", ")}. Those keys will need a new binding.` : "";
    if (confirm(`Delete macro "${macro.name || macro.id}"?${where}`)) ondelete();
  }
</script>

<div class="editor">
  <div class="head">
    <h2>Macro <span class="muted mono">{macro.id}</span></h2>
    <div class="head-btns">
      <button onclick={run} title="Play one run in the simulator">{changed ? "▶ Apply & run" : "▶ Run"}</button>
      <button class="danger" onclick={confirmDelete}>Delete</button>
    </div>
  </div>
  <p class="muted small">
    {#if macro.used_by.length}Bound to {macro.used_by.join(", ")}.{:else}Not bound to any key yet — pick a key and choose Macro.{/if}
  </p>

  <label class="row">
    <span>Name</span>
    <input bind:value={name} placeholder={macro.id} maxlength="20" />
  </label>

  <div class="row">
    <span>Type</span>
    <div class="seg">
      {#each KIND_INFO as k}
        <button class:on={kind === k.kind} onclick={() => (kind = k.kind)}>{k.label}</button>
      {/each}
    </div>
  </div>
  <p class="muted small indent">{KIND_INFO.find((k) => k.kind === kind)?.help}</p>

  {#if kind === "toggle"}
    <div class="row">
      <span>When stopped</span>
      <div class="seg">
        <button class:on={stop === "finish"} onclick={() => (stop = "finish")}>Finish the current run</button>
        <button class:on={stop === "immediate"} onclick={() => (stop = "immediate")}>Stop immediately</button>
      </div>
    </div>
  {/if}

  <h3>Steps</h3>
  <ol class="steps">
    {#each rows as row, i}
      <li>
        <span class="n">{i + 1}</span>
        <select bind:value={row.type}>
          {#each STEP_TYPES as t}<option value={t.type}>{t.label}</option>{/each}
        </select>
        {#if row.type === "delay"}
          <input class="ms" type="number" min="0" max="65535" step="10" bind:value={row.value} />
          <span class="muted">ms</span>
        {:else if row.type === "text"}
          <input class="grow" bind:value={row.value} placeholder="text to type" />
        {:else}
          <input class="grow mono" list="macro-key-names" bind:value={row.value} placeholder="key, e.g. ctrl+c, shift, f5, vol+" />
          <button class="pick" onclick={() => (picking = i)} title="Pick or press a key">⌨</button>
        {/if}
        <div class="row-btns">
          <button onclick={() => move(i, -1)} disabled={i === 0} title="Move up">↑</button>
          <button onclick={() => move(i, 1)} disabled={i === rows.length - 1} title="Move down">↓</button>
          <button onclick={() => remove(i)} title="Remove">✕</button>
        </div>
      </li>
    {/each}
    {#if !rows.length}<li class="muted empty">No steps yet.</li>{/if}
  </ol>

  <div class="adds">
    <button onclick={() => add("tap")}>+ Tap key</button>
    <button onclick={() => add("text")}>+ Type text</button>
    <button onclick={() => add("delay")}>+ Wait</button>
    <button onclick={() => add("down")}>+ Hold key</button>
    <button onclick={() => add("up")}>+ Release key</button>
  </div>

  {#if stillHeld.length}
    <p class="warn small">Still held at the end: {stillHeld.join(", ")} (released automatically when the macro finishes).</p>
  {/if}

  {#if hasText}
    <label class="row">
      <span>Typing speed</span>
      <input
        class="ms"
        type="number"
        min="0"
        max="1000"
        placeholder="default"
        value={charDelay ?? ""}
        oninput={(e) => {
          const v = parseInt(e.currentTarget.value, 10);
          charDelay = isNaN(v) ? null : Math.max(0, Math.min(1000, v));
        }}
      />
      <span class="muted">ms between characters</span>
    </label>
  {/if}

  {#if error}<p class="error">{error}</p>{/if}

  <div class="actions">
    {#if changed}<span class="warn small pending">Unapplied changes</span>{/if}
    <button onclick={load} disabled={!changed}>Revert</button>
    <button class="primary" onclick={apply} disabled={!changed}>Apply</button>
  </div>
</div>

{#if picking !== null}
  <KeyPicker
    onpick={(name) => {
      if (picking !== null && rows[picking]) rows[picking].value = name;
    }}
    onclose={() => (picking = null)}
  />
{/if}

<datalist id="macro-key-names">
  {#each KEY_NAMES as k}<option value={k}></option>{/each}
</datalist>

<style>
  .editor {
    display: flex;
    flex-direction: column;
    gap: 10px;
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
    font-size: 14px;
    margin-top: 6px;
  }
  .head-btns,
  .row-btns,
  .adds,
  .seg {
    display: flex;
    gap: 4px;
    flex-wrap: wrap;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .row > span:first-child {
    width: 96px;
    flex-shrink: 0;
    color: var(--muted);
  }
  .row > input:not(.ms) {
    flex: 1;
  }
  .indent {
    margin-left: 106px !important;
  }
  .seg button.on {
    background: var(--accent);
    border-color: var(--accent);
    color: #fff;
  }
  .steps {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .steps li {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .n {
    width: 18px;
    text-align: right;
    color: var(--muted);
    font-size: 12px;
  }
  .grow {
    flex: 1;
    min-width: 0;
  }
  .ms {
    width: 80px;
  }
  .pick {
    padding: 3px 8px;
  }
  .row-btns button,
  .adds button {
    padding: 3px 8px;
    font-size: 12px;
  }
  .empty {
    padding: 6px 24px;
  }
  .muted {
    color: var(--muted);
  }
  .small,
  .error,
  .warn {
    margin: 0;
    font-size: 12px;
  }
  .error {
    color: var(--danger);
    font-size: 13px;
  }
  .warn {
    color: var(--warn);
  }
  .pending {
    margin-right: auto;
    align-self: center;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
</style>
