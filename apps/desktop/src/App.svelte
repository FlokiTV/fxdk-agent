<script lang="ts">
  const shellStatus: 'Ready' = 'Ready';
</script>

<main class="shell">
  <header>
    <p class="eyebrow">WIP · MVP v0.1</p>
    <h1>FXDK Agent</h1>
    <p class="subtitle">Local control plane for FiveM/FxDK development.</p>
  </header>

  <section class="status-card" aria-labelledby="shell-status-title">
    <div>
      <p class="label" id="shell-status-title">Desktop shell</p>
      <p class="value">{shellStatus}</p>
    </div>
    <span class="status-dot" aria-hidden="true"></span>
  </section>

  <p class="hint">
    Control API and runtime lifecycle will be wired in the next MVP milestones.
  </p>
</main>

<style>
  :global(:root) {
    font-family:
      Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI",
      sans-serif;
    color: #f4f4f5;
    background: #09090b;
    font-synthesis: none;
    text-rendering: optimizeLegibility;
  }

  :global(*) {
    box-sizing: border-box;
  }

  :global(body) {
    margin: 0;
    min-width: 320px;
    min-height: 100vh;
  }

  .shell {
    width: min(720px, calc(100% - 48px));
    margin: 0 auto;
    padding: 64px 0;
  }

  .eyebrow,
  .label,
  .hint {
    color: #a1a1aa;
  }

  .eyebrow,
  .label {
    margin: 0;
    font-size: 0.75rem;
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  h1 {
    margin: 8px 0 0;
    font-size: clamp(2rem, 6vw, 3.5rem);
    line-height: 1;
  }

  .subtitle {
    margin: 16px 0 0;
    color: #d4d4d8;
  }

  .status-card {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-top: 48px;
    padding: 20px;
    border: 1px solid #27272a;
    border-radius: 12px;
    background: #18181b;
  }

  .value {
    margin: 6px 0 0;
    font-size: 1.125rem;
    font-weight: 650;
  }

  .status-dot {
    width: 10px;
    height: 10px;
    border-radius: 999px;
    background: #22c55e;
    box-shadow: 0 0 20px rgb(34 197 94 / 55%);
  }

  .hint {
    margin-top: 18px;
    font-size: 0.875rem;
  }
</style>
