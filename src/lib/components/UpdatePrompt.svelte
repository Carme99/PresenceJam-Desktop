<script lang="ts">
  import { onMount } from 'svelte';
  import { check, type Update } from '@tauri-apps/plugin-updater';
  import { invoke } from '@tauri-apps/api/core';

  // Always-mounted update banner (3.0-P5). On mount it asks the updater
  // plugin whether a newer release exists; if it does it shows a small
  // dismissible banner with a "Download & Install" button. Stays silent
  // when the app is already current.
  let update = $state<Update | null>(null);
  let dismissed = $state(false);
  let downloading = $state(false);
  let downloadedBytes = $state(0);
  let totalBytes = $state(0);
  let error = $state('');

  onMount(() => {
    let cancelled = false;
    check()
      .then((u) => {
        if (!cancelled && u) update = u;
      })
      .catch((e) => {
        // Offline / unreachable endpoint / mismatched pubkey etc. — never
        // block the UI over a failed update check.
        console.error('[UPDATER] check failed:', e);
      });
    return () => {
      cancelled = true;
    };
  });

  const progress = $derived(
    totalBytes > 0 ? Math.min(downloadedBytes / totalBytes, 1) : 0
  );

  async function downloadAndInstall() {
    if (!update || downloading) return;
    downloading = true;
    error = '';
    downloadedBytes = 0;
    totalBytes = 0;
    try {
      await update.downloadAndInstall((event) => {
        if (event.event === 'Started' && event.data.contentLength) {
          totalBytes = event.data.contentLength;
        } else if (event.event === 'Progress') {
          downloadedBytes += event.data.chunkLength;
        }
      });
      // Update is staged; relaunch so the new version takes effect.
      await invoke('relaunch_app');
    } catch (e) {
      console.error('[UPDATER] downloadAndInstall failed:', e);
      error = String(e);
      downloading = false;
    }
  }
</script>

{#if update && !dismissed}
  <div class="update-banner" role="status">
    <div class="update-info">
      <span class="update-title">Update v{update.version} available</span>
      {#if downloading}
        <span class="update-progress">
          {Math.round(progress * 100)}%{totalBytes > 0
            ? ` (${Math.round(downloadedBytes / 1024 / 1024)}/${Math.round(totalBytes / 1024 / 1024)} MB)`
            : ''}
        </span>
      {:else if error}
        <span class="update-error">Download failed — {error}</span>
      {/if}
    </div>
    <div class="update-actions">
      <button
        type="button"
        class="download-btn"
        onclick={downloadAndInstall}
        disabled={downloading}
      >
        {downloading ? 'Downloading…' : 'Download & Install'}
      </button>
      <button
        type="button"
        class="icon-btn dismiss-btn"
        onclick={() => (dismissed = true)}
        aria-label="Dismiss update banner"
        title="Dismiss"
      >
        ×
      </button>
    </div>
  </div>
{/if}

<style>
  .update-banner {
    position: fixed;
    top: var(--sp-3);
    left: 50%;
    transform: translateX(-50%);
    z-index: 1000;
    display: flex;
    align-items: center;
    gap: var(--sp-4);
    max-width: calc(100% - var(--sp-6));
    padding: var(--sp-2) var(--sp-3);
    background: var(--bg-elevated);
    border: 1px solid var(--accent);
    border-radius: var(--r-md);
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.25);
  }
  .update-info {
    display: flex;
    flex-direction: column;
    gap: var(--sp-1);
    min-width: 0;
  }
  .update-title {
    font-size: var(--fs-sm);
    font-weight: 700;
    color: var(--fg);
    white-space: nowrap;
  }
  .update-progress {
    font-size: var(--fs-xs);
    color: var(--fg-muted);
  }
  .update-error {
    font-size: var(--fs-xs);
    color: var(--danger);
  }
  .update-actions {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    flex-shrink: 0;
  }
  .download-btn {
    padding: var(--sp-2) var(--sp-4);
    font-size: var(--fs-sm);
  }
  .dismiss-btn {
    width: 28px;
    height: 28px;
    font-size: var(--fs-lg);
    line-height: 1;
  }
</style>
