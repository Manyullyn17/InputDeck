<script lang="ts">
  import { api, message, type DeviceInfo, type View } from "./api";

  let {
    canUpload,
    onpulled,
  }: {
    /** Whether the open document compiles. */
    canUpload: boolean;
    onpulled: (view: View) => void;
  } = $props();

  let info = $state<DeviceInfo | null>(null);
  let error = $state<string | null>(null);
  let busy = $state(false);
  let status = $state<string | null>(null);

  async function run<T>(p: () => Promise<T>, done?: string): Promise<T | null> {
    busy = true;
    status = null;
    try {
      const result = await p();
      error = null;
      if (done) status = done;
      return result;
    } catch (e) {
      error = message(e);
      return null;
    } finally {
      busy = false;
    }
  }

  async function refresh() {
    info = await run(api.deviceInfo);
  }

  async function upload(slot: number | null) {
    const result = await run(() => api.deviceUpload(slot), slot === null ? "Added as a new profile" : "Uploaded");
    if (result) info = result;
  }

  async function act(f: () => Promise<DeviceInfo>, done: string) {
    const result = await run(f, done);
    if (result) info = result;
  }

  async function pull(i: number) {
    const view = await run(() => api.devicePull(i), "Opened from the device (unsaved)");
    if (view) onpulled(view);
  }

  $effect(() => {
    refresh();
  });
</script>

<section class="device">
  <div class="head">
    <h2>Device</h2>
    <button onclick={refresh} disabled={busy}>Refresh</button>
  </div>

  {#if error}
    <p class="error">{error}</p>
  {:else if info && !info.compatible}
    <p class="error">The device uses config format v{info.format_version}; update the firmware or app.</p>
  {:else if info}
    <ul>
      {#each info.profiles as p, i}
        <li class:active={i === info.active}>
          <div class="name">
            <span class="dot">{i === info.active ? "●" : "○"}</span>
            <span class="muted">{i + 1}.</span> {p.name} <span class="muted">· {p.layers} layers</span>
          </div>
          <div class="btns">
            <button onclick={() => act(() => api.deviceActivate(i), "Activated")} disabled={busy || i === info.active}>Activate</button>
            <button onclick={() => upload(i)} disabled={busy || !canUpload} title="Replace this profile with the open file">Replace</button>
            <button onclick={() => pull(i)} disabled={busy} title="Open this profile for editing">Open</button>
            <button class="danger" onclick={() => act(() => api.deviceRemove(i), "Removed")} disabled={busy || info.profiles.length < 2}>✕</button>
          </div>
        </li>
      {/each}
      {#if !info.profiles.length}<li class="muted">No config stored (built-in demo running).</li>{/if}
    </ul>
    <button class="primary" onclick={() => upload(null)} disabled={busy || !canUpload}>Upload open file as new profile</button>
  {/if}
  {#if status}<p class="ok">{status}</p>{/if}
</section>

<style>
  .device {
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
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  li {
    padding: 8px;
    background: var(--panel-2);
    border: 1px solid var(--border);
    border-radius: 6px;
  }
  li.active {
    border-color: var(--accent);
  }
  .name {
    margin-bottom: 6px;
  }
  .dot {
    color: var(--accent);
  }
  .btns {
    display: flex;
    gap: 4px;
    flex-wrap: wrap;
  }
  .btns button {
    padding: 3px 8px;
    font-size: 12px;
  }
  .muted {
    color: var(--muted);
  }
  .error {
    color: var(--danger);
    margin: 0;
  }
  .ok {
    color: var(--ok);
    margin: 0;
  }
</style>
