<script lang="ts">
  import { onMount } from 'svelte';
  import type {
    AppConfig,
    ConfigPatch,
    ConfigValidationIssue,
  } from '@fxdk-agent/protocol';
  import {
    CONTROL_API_BASE_URL,
    ControlApiError,
    getConfig,
    patchConfig,
  } from './lib/control-api';

  type ApiState = 'connecting' | 'online' | 'offline';

  interface ConfigForm {
    serverProject: string;
    fxserverPath: string;
    fivemPath: string;
    syntheticIdentityEnabled: boolean;
  }

  let apiState: ApiState = 'connecting';
  let loading = true;
  let saving = false;
  let notice = '';
  let issues: ConfigValidationIssue[] = [];
  let form: ConfigForm = emptyForm();

  onMount(() => {
    void loadConfiguration();
  });

  function emptyForm(): ConfigForm {
    return {
      serverProject: '',
      fxserverPath: '',
      fivemPath: '',
      syntheticIdentityEnabled: false,
    };
  }

  function applyConfig(config: AppConfig): void {
    form = {
      serverProject: config.serverProject ?? '',
      fxserverPath: config.fxserverPath ?? '',
      fivemPath: config.fivemPath ?? '',
      syntheticIdentityEnabled: config.syntheticIdentity.enabled,
    };
  }

  function normalizePath(value: string): string | null {
    const trimmed = value.trim();
    return trimmed.length > 0 ? trimmed : null;
  }

  function validationIssues(error: unknown): ConfigValidationIssue[] {
    if (!(error instanceof ControlApiError)) {
      return [];
    }

    const detail = error.payload?.error.detail;
    const rawIssues = detail?.['issues'];

    return Array.isArray(rawIssues) ? (rawIssues as ConfigValidationIssue[]) : [];
  }

  function errorMessage(error: unknown): string {
    if (error instanceof ControlApiError) {
      return error.message;
    }
    if (error instanceof Error) {
      return error.message;
    }
    return 'Unexpected Control API error';
  }

  async function loadConfiguration(): Promise<void> {
    loading = true;
    notice = '';
    issues = [];

    try {
      const config = await getConfig();
      applyConfig(config);
      apiState = 'online';
    } catch (error) {
      apiState = 'offline';
      notice = errorMessage(error);
    } finally {
      loading = false;
    }
  }

  async function saveConfiguration(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    saving = true;
    notice = '';
    issues = [];

    const patch: ConfigPatch = {
      serverProject: normalizePath(form.serverProject),
      fxserverPath: normalizePath(form.fxserverPath),
      fivemPath: normalizePath(form.fivemPath),
      syntheticIdentity: {
        enabled: form.syntheticIdentityEnabled,
      },
    };

    try {
      const config = await patchConfig(patch);
      applyConfig(config);
      apiState = 'online';
      notice = 'Configuration saved.';
    } catch (error) {
      apiState = error instanceof ControlApiError ? 'online' : 'offline';
      issues = validationIssues(error);
      notice = errorMessage(error);
    } finally {
      saving = false;
    }
  }
</script>

<main class="shell">
  <header class="hero">
    <div>
      <p class="eyebrow">WIP · MVP v0.1</p>
      <h1>FXDK Agent</h1>
      <p class="subtitle">Local control plane for FiveM/FxDK development.</p>
    </div>

    <div class="api-state" data-state={apiState}>
      <span class="status-dot" aria-hidden="true"></span>
      <div>
        <p class="label">Control API</p>
        <p class="api-value">{apiState}</p>
      </div>
    </div>
  </header>

  <section class="panel" aria-labelledby="environment-title">
    <div class="panel-heading">
      <div>
        <p class="label">Environment</p>
        <h2 id="environment-title">Runtime configuration</h2>
      </div>

      <button
        class="secondary"
        type="button"
        disabled={loading || saving}
        onclick={() => void loadConfiguration()}
      >
        {loading ? 'Loading…' : 'Reload'}
      </button>
    </div>

    <p class="endpoint">{CONTROL_API_BASE_URL}</p>

    <form onsubmit={saveConfiguration}>
      <label>
        <span>Server project</span>
        <input
          type="text"
          bind:value={form.serverProject}
          placeholder="D:\DEV\fivem\my-server"
          disabled={loading || saving}
          autocomplete="off"
        />
      </label>

      <label>
        <span>FXServer executable</span>
        <input
          type="text"
          bind:value={form.fxserverPath}
          placeholder="D:\cfx\FXServer.exe"
          disabled={loading || saving}
          autocomplete="off"
        />
      </label>

      <label>
        <span>FiveM executable</span>
        <input
          type="text"
          bind:value={form.fivemPath}
          placeholder="C:\Users\you\AppData\Local\FiveM\FiveM.exe"
          disabled={loading || saving}
          autocomplete="off"
        />
      </label>

      <label class="toggle-row">
        <span>
          <strong>Synthetic DEV identity</strong>
          <small>Development-only license/license2 identity for local FXDK sessions.</small>
        </span>
        <input
          type="checkbox"
          bind:checked={form.syntheticIdentityEnabled}
          disabled={loading || saving}
        />
      </label>

      {#if issues.length > 0}
        <div class="validation" role="alert">
          <p class="label">Validation</p>
          <ul>
            {#each issues as issue}
              <li>
                <strong>{issue.field}</strong>
                <span>{issue.message}</span>
              </li>
            {/each}
          </ul>
        </div>
      {/if}

      {#if notice}
        <p class:success={notice === 'Configuration saved.'} class="notice">
          {notice}
        </p>
      {/if}

      <div class="actions">
        <button
          class="primary"
          type="submit"
          disabled={loading || saving || apiState === 'offline'}
        >
          {saving ? 'Saving…' : 'Save configuration'}
        </button>
      </div>
    </form>
  </section>
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

  button,
  input {
    font: inherit;
  }

  .shell {
    width: min(860px, calc(100% - 48px));
    margin: 0 auto;
    padding: 48px 0 64px;
  }

  .hero,
  .panel-heading,
  .toggle-row,
  .actions,
  .api-state {
    display: flex;
    align-items: center;
  }

  .hero,
  .panel-heading,
  .toggle-row {
    justify-content: space-between;
    gap: 24px;
  }

  .eyebrow,
  .label,
  .endpoint,
  small {
    color: #a1a1aa;
  }

  .eyebrow,
  .label {
    margin: 0;
    font-size: 0.72rem;
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  h1 {
    margin: 8px 0 0;
    font-size: clamp(2rem, 6vw, 3.4rem);
    line-height: 1;
  }

  h2 {
    margin: 6px 0 0;
    font-size: 1.2rem;
  }

  .subtitle {
    margin: 14px 0 0;
    color: #d4d4d8;
  }

  .api-state {
    min-width: 148px;
    gap: 12px;
    padding: 12px 14px;
    border: 1px solid #27272a;
    border-radius: 10px;
    background: #18181b;
  }

  .api-value {
    margin: 3px 0 0;
    text-transform: capitalize;
  }

  .status-dot {
    width: 10px;
    height: 10px;
    border-radius: 999px;
    background: #eab308;
  }

  .api-state[data-state='online'] .status-dot {
    background: #22c55e;
    box-shadow: 0 0 18px rgb(34 197 94 / 45%);
  }

  .api-state[data-state='offline'] .status-dot {
    background: #ef4444;
  }

  .panel {
    margin-top: 36px;
    padding: 24px;
    border: 1px solid #27272a;
    border-radius: 14px;
    background: #18181b;
  }

  .endpoint {
    margin: 12px 0 24px;
    font-family: "Cascadia Code", "SFMono-Regular", Consolas, monospace;
    font-size: 0.78rem;
  }

  form {
    display: grid;
    gap: 18px;
  }

  label:not(.toggle-row) {
    display: grid;
    gap: 8px;
    font-size: 0.86rem;
    font-weight: 650;
  }

  input[type='text'] {
    width: 100%;
    padding: 11px 12px;
    border: 1px solid #3f3f46;
    border-radius: 8px;
    outline: none;
    color: #f4f4f5;
    background: #0f0f12;
  }

  input[type='text']:focus {
    border-color: #71717a;
  }

  input:disabled,
  button:disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }

  .toggle-row {
    padding: 14px 0;
    border-top: 1px solid #27272a;
    border-bottom: 1px solid #27272a;
  }

  .toggle-row span {
    display: grid;
    gap: 4px;
  }

  .toggle-row small {
    font-weight: 400;
  }

  .toggle-row input {
    width: 18px;
    height: 18px;
  }

  button {
    border: 1px solid transparent;
    border-radius: 8px;
    padding: 9px 13px;
    cursor: pointer;
  }

  .primary {
    color: #09090b;
    background: #f4f4f5;
    font-weight: 700;
  }

  .secondary {
    color: #e4e4e7;
    border-color: #3f3f46;
    background: #27272a;
  }

  .actions {
    justify-content: flex-end;
  }

  .validation {
    padding: 14px;
    border: 1px solid #7f1d1d;
    border-radius: 8px;
    background: rgb(127 29 29 / 18%);
  }

  .validation ul {
    margin: 10px 0 0;
    padding-left: 20px;
  }

  .validation li {
    margin-top: 6px;
  }

  .validation li span {
    margin-left: 8px;
    color: #fecaca;
  }

  .notice {
    margin: 0;
    color: #fca5a5;
    font-size: 0.86rem;
  }

  .notice.success {
    color: #86efac;
  }

  @media (max-width: 680px) {
    .hero,
    .panel-heading {
      align-items: flex-start;
      flex-direction: column;
    }

    .api-state {
      width: 100%;
    }
  }
</style>
