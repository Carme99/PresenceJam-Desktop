<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { onMount } from 'svelte';
  import PageHeader from './PageHeader.svelte';
  import { currentView } from '$lib/stores/app';
  import type { DiagnosticsSnapshot } from '$lib/types';

  /**
   * Telemetry-free local diagnostics page (scope-3.3 candidate C5).
   *
   * Collects a support snapshot via the `get_diagnostics_snapshot`
   * command — app/OS versions, sanitized config summary, token metadata
   * only (expiry timestamps, presence flags; never token values),
   * keychain presence flags, and the redacted tail of the on-disk log.
   * "Copy diagnostics" puts the JSON on the clipboard and "Save to
   * file" downloads it, so the user can paste it into a GitHub issue.
   * No network calls anywhere — matches SECURITY.md "No Telemetry".
   */

  let snapshot = $state<DiagnosticsSnapshot | null>(null);
  let loadError = $state('');
  let feedback = $state('');

  let loading = $derived(snapshot === null && loadError === '');

  function goBack() {
    currentView.set('dashboard');
  }

  function formatText(snap: DiagnosticsSnapshot): string {
    return JSON.stringify(snap, null, 2);
  }

  async function copyDiagnostics() {
    if (!snapshot) return;
    try {
      await navigator.clipboard.writeText(formatText(snapshot));
      feedback = 'Diagnostics copied to clipboard.';
    } catch (e) {
      console.warn('[DIAGNOSTICS] clipboard write failed:', e);
      feedback = 'Copy failed — use "Save to file" instead.';
    }
  }

  function saveToFile() {
    if (!snapshot) return;
    try {
      const blob = new Blob([formatText(snapshot)], { type: 'application/json' });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = `presencejam-diagnostics-${new Date().toISOString().replace(/[:.]/g, '-')}.json`;
      document.body.appendChild(a);
      a.click();
      a.remove();
      URL.revokeObjectURL(url);
      feedback = 'Diagnostics saved to your downloads folder.';
    } catch (e) {
      console.warn('[DIAGNOSTICS] save failed:', e);
      feedback = 'Save failed — use "Copy diagnostics" instead.';
    }
  }

  function boolLabel(v: boolean | undefined | null): string {
    return v ? 'Yes' : 'No';
  }

  onMount(async () => {
    try {
      snapshot = await invoke<DiagnosticsSnapshot>('get_diagnostics_snapshot');
    } catch (e) {
      console.warn('[DIAGNOSTICS] get_diagnostics_snapshot failed:', e);
      loadError = String(e);
    }
  });
</script>

<div class="diagnostics">
  <PageHeader title="Diagnostics" onBack={goBack} showLogo={false} showThemeToggle={false} />

  <div class="toolbar">
    <span class="hint">Local-only snapshot — safe to attach to a bug report.</span>
    <button class="btn-secondary" onclick={copyDiagnostics} disabled={!snapshot}>Copy diagnostics</button>
    <button class="btn-secondary" onclick={saveToFile} disabled={!snapshot}>Save to file</button>
  </div>

  <p class="feedback" role="status">{feedback}</p>

  <div class="content">
    {#if loading}
      <div class="empty-state">
        <p>Collecting diagnostics…</p>
      </div>
    {:else if loadError}
      <div class="empty-state" role="alert">
        <p>Failed to collect diagnostics</p>
        <p class="hint">{loadError}</p>
      </div>
    {:else if snapshot}
      <section aria-label="Versions">
        <h2>Versions</h2>
        <dl>
          <dt>PresenceJam</dt><dd>{snapshot.app_version}</dd>
          <dt>Tauri</dt><dd>{snapshot.tauri_version}</dd>
          <dt>OS</dt><dd>{snapshot.os.platform} ({snapshot.os.arch}, {snapshot.os.family})</dd>
        </dl>
      </section>

      <section aria-label="Configuration">
        <h2>Configuration</h2>
        <dl>
          <dt>Spotify client ID</dt><dd class="mono">{snapshot.config.spotify_client_id || '(not set)'}</dd>
          <dt>Redirect URI</dt><dd class="mono">{snapshot.config.redirect_uri}</dd>
          <dt>Client secret in keychain</dt><dd>{boolLabel(snapshot.config.client_secret_set)}</dd>
          <dt>Clear status on pause</dt><dd>{boolLabel(snapshot.config.clear_on_pause)}</dd>
          <dt>Profanity filter</dt><dd>{boolLabel(snapshot.config.profanity_filter)}</dd>
          <dt>Start minimized</dt><dd>{boolLabel(snapshot.config.start_minimized)}</dd>
          <dt>Teams availability sync</dt><dd>{boolLabel(snapshot.config.availability_sync)}</dd>
          <dt>Presence gate</dt><dd>{boolLabel(snapshot.config.presence_gate)}</dd>
          <dt>Poll interval (default/min/max)</dt>
          <dd>{snapshot.config.default_interval_seconds}s / {snapshot.config.minimum_interval_seconds}s / {snapshot.config.maximum_interval_seconds}s</dd>
          <dt>Logging</dt>
          <dd>{snapshot.config.logging_enabled ? `enabled (${snapshot.config.log_level})` : 'disabled'}</dd>
          <dt>Launch at login</dt><dd>{boolLabel(snapshot.config.autostart)}</dd>
        </dl>
      </section>

      <section aria-label="Connection status">
        <h2>Connections</h2>
        <dl>
          <dt>Spotify connected</dt><dd>{boolLabel(snapshot.tokens.spotify_connected)}</dd>
          <dt>Spotify token expires</dt><dd>{snapshot.tokens.spotify_expires_at ?? '—'}{snapshot.tokens.spotify_expired ? ' (expired)' : ''}</dd>
          <dt>Teams connected</dt><dd>{boolLabel(snapshot.tokens.teams_connected)}</dd>
          <dt>Teams token expires</dt><dd>{snapshot.tokens.teams_expires_at ?? '—'}{snapshot.tokens.teams_expired ? ' (expired)' : ''}</dd>
          <dt>Keychain: Spotify secret present</dt><dd>{boolLabel(snapshot.keychain.spotify_client_secret_present)}</dd>
          <dt>Keychain: token encryption key present</dt><dd>{boolLabel(snapshot.keychain.tokens_encryption_key_present)}</dd>
        </dl>
        <p class="hint">Token values are never included — expiry timestamps and presence flags only.</p>
      </section>

      <section aria-label="Recent logs">
        <h2>Recent log lines</h2>
        <p class="hint">{snapshot.log_source_status}</p>
        {#if snapshot.recent_logs.length === 0}
          <div class="empty-state small">
            <p>No log lines available yet.</p>
          </div>
        {:else}
          <div class="log-list">
            {#each snapshot.recent_logs as line}
              <div class="log-entry">{line}</div>
            {/each}
          </div>
        {/if}
      </section>
    {/if}
  </div>
</div>

<style>
  .diagnostics {
    display: flex;
    flex-direction: column;
    height: 100vh;
    padding: var(--sp-5);
    max-width: 980px;
    margin: 0 auto;
    gap: var(--sp-4);
  }

  .toolbar {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
  }
  .toolbar .btn-secondary {
    width: auto;
    padding: var(--sp-2) var(--sp-4);
    font-size: var(--fs-sm);
  }
  .hint {
    margin-right: auto;
    font-size: var(--fs-xs);
    color: var(--fg-subtle);
  }

  .feedback {
    min-height: 1.2em;
    margin: 0;
    font-size: var(--fs-sm);
    color: var(--info);
  }

  .content {
    flex: 1;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: var(--sp-4);
  }

  section {
    background: var(--bg-surface);
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    padding: var(--sp-3);
  }
  h2 {
    margin: 0 0 var(--sp-2);
    font-size: var(--fs-base);
    font-weight: 700;
    color: var(--fg);
  }

  dl {
    display: grid;
    grid-template-columns: minmax(160px, max-content) 1fr;
    gap: var(--sp-1) var(--sp-3);
    margin: 0;
  }
  dt {
    color: var(--fg-muted);
    font-size: var(--fs-sm);
  }
  dd {
    margin: 0;
    color: var(--fg);
    font-size: var(--fs-sm);
    word-break: break-all;
  }
  dd.mono {
    font-family: var(--font-mono);
  }

  .log-list {
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    padding: var(--sp-2);
    font-family: var(--font-mono);
    font-size: var(--fs-xs);
    max-height: 240px;
    overflow-y: auto;
  }
  .log-entry {
    padding: 2px var(--sp-2);
    color: var(--fg);
    white-space: pre-wrap;
    word-break: break-all;
    line-height: 1.5;
  }

  .empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: var(--sp-1);
    min-height: 120px;
    color: var(--fg-subtle);
  }
  .empty-state.small { min-height: 60px; }
  .empty-state p { color: var(--fg-muted); }
</style>
