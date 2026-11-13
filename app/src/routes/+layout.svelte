<script lang="ts">
  let { children } = $props();
</script>

{@render children()}

<style>
  :global(:root) {
    --bg: #15171c;
    --panel: #1d2027;
    --panel-2: #252932;
    --border: #323744;
    --text: #e6e8ee;
    --muted: #8b92a3;
    --accent: #6ea8fe;
    --danger: #ff6b6b;
    --ok: #51cf66;
    --warn: #fcc419;
    --radius: 8px;
    font-family: Inter, "Segoe UI", system-ui, sans-serif;
    font-size: 14px;
    color: var(--text);
    background: var(--bg);
    color-scheme: dark;
  }
  @media (prefers-color-scheme: light) {
    :global(:root) {
      --bg: #f3f4f7;
      --panel: #ffffff;
      --panel-2: #eef0f4;
      --border: #d6d9e0;
      --text: #1d2027;
      --muted: #6b7280;
      --accent: #2563eb;
      color-scheme: light;
    }
  }
  :global(body) {
    margin: 0;
    background: var(--bg);
  }
  :global(*) {
    box-sizing: border-box;
  }
  :global(button) {
    font: inherit;
    color: var(--text);
    background: var(--panel-2);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 5px 10px;
    cursor: pointer;
  }
  :global(button:hover:not(:disabled)) {
    border-color: var(--accent);
  }
  :global(button:disabled) {
    opacity: 0.45;
    cursor: default;
  }
  :global(button.primary) {
    background: var(--accent);
    border-color: var(--accent);
    color: #fff;
  }
  :global(button.danger) {
    color: var(--danger);
  }
  :global(input, select) {
    font: inherit;
    color: var(--text);
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 5px 8px;
  }
  :global(input:focus, select:focus) {
    outline: none;
    border-color: var(--accent);
  }
  :global(h2) {
    font-size: 12px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--muted);
    margin: 0 0 8px;
  }
  :global(code, .mono) {
    font-family: "JetBrains Mono", ui-monospace, monospace;
  }
</style>
