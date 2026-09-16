<script lang="ts">
  import { onMount } from 'svelte';
  import { check, type Update } from '@tauri-apps/plugin-updater';
  import { invoke } from '@tauri-apps/api/core';
  import { getVersion } from '@tauri-apps/api/app';
  import { t } from '$lib/i18n';

  // Always-mounted update banner (3.0-P5). On mount it asks the updater
  // plugin whether a newer release exists; if it does it shows a small
  // dismissible banner with a "Download & Install" button (immediate
  // relaunch) or an "Install on quit" button (deferred — see C3 in
  // docs/scope-3.3.md). Stays silent when the app is already current.
  //
  // Issue #431: the deferred path goes through a quit-time confirmation
  // surface first — the banner shows the candidate-vs-current versions
  // with an explicit install/skip choice, and the backend refuses stale
  // stages (older than or equal to the running build) unless the user
  // forces them after seeing both versions.
  let update = $state<Update | null>(null);
  let dismissed = $state(false);
  let downloading = $state(false);
  let downloadedBytes = $state(0);
  let totalBytes = $state(0);
  let error = $state('');
  // C3(a): silent re-check every ~24h while the app keeps running (the
  // startup check alone would pin long-lived sessions to a stale
  // release). setInterval pauses naturally when the OS suspends the app.
  const CHECK_INTERVAL_MS = 24 * 60 * 60 * 1000;
  // C3(c): set once the deferred update has been downloaded Rust-side;
  // applied on process exit by `updater_bg::install_pending_on_exit`.
  let stagedVersion = $state('');
  let staging = $state(false);
  // #431: `confirming` shows the install/skip choice surface;
  // `currentVersion` is the running build (best-effort — the banner
  // falls back to version-agnostic strings when it stays empty);
  // `staleSkippedVersion` remembers a candidate declined as stale so the
  // same version is not re-offered (the backend skip marker persists
  // this across launches).
  let confirming = $state(false);
  let currentVersion = $state('');
  let staleSkippedVersion = $state('');

  // Mirrors the backend `StageDeferredOutcome` shape (kept local so no
  // generated types need to change for this slice).
  interface StageOutcome {
    staged: string | null;
    current: string;
  }

  let isStaleSkipped = $derived(
    update !== null && staleSkippedVersion !== '' && staleSkippedVersion === update.version
  );

  function checkForUpdate() {
    check()
      .then((u) => {
        if (u) {
          // A new candidate deserves its own verdict — forget a stale
          // skip recorded for a previous version.
          if (!update || update.version !== u.version) {
            staleSkippedVersion = '';
            confirming = false;
            // #597: a dismissal is scoped to the version it dismissed.
            // `dismissed` gates rendering, so leaving it set would make the
            // 24h re-check unable to ever surface a newer release in a
            // long-running session. The same version stays dismissed.
            dismissed = false;
          }
          update = u;
        }
      })
      .catch((e) => {
        // Offline / unreachable endpoint / mismatched pubkey etc. — never
        // surface a failed update check to the user, whether at startup
        // or from a background timer.
        console.error('[UPDATER] check failed:', e);
      });
  }

  onMount(() => {
    checkForUpdate();
    const interval = setInterval(checkForUpdate, CHECK_INTERVAL_MS);
    return () => clearInterval(interval);
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

  // #431: open the quit-time confirmation surface (candidate-vs-current
  // versions with an install/skip choice) instead of staging blindly.
  // The JS plugin API can't defer — `downloadAndInstall()` applies the
  // payload immediately on Windows — so the actual staging still runs
  // Rust-side (`updater_bg::stage_deferred_update` on confirmation).
  async function openQuitConfirm() {
    if (!update || staging || confirming || downloading) return;
    error = '';
    try {
      currentVersion = await getVersion();
    } catch (e) {
      console.error('[UPDATER] getVersion failed:', e);
      currentVersion = '';
    }
    confirming = true;
  }

  // Skip choice before anything is staged: nothing pending, nothing to
  // undo — just leave the banner on its plain update offer.
  function cancelQuitConfirm() {
    confirming = false;
  }

  // Stages the deferred update Rust-side and holds it until the process
  // exits (`updater_bg::install_pending_on_exit` on RunEvent::Exit).
  // `force` is true only from the stale-skipped surface, where the user
  // confirmed the install knowing both versions.
  async function stageForQuit(force: boolean) {
    if (!update || staging || downloading) return;
    staging = true;
    error = '';
    try {
      const outcome = await invoke<StageOutcome>('stage_deferred_update', { force });
      // Backend truth for the running version — always populated, so
      // the staged state can show both versions unconditionally.
      currentVersion = outcome.current;
      if (outcome.staged) {
        stagedVersion = outcome.staged;
        confirming = false;
        staleSkippedVersion = '';
      } else {
        // Declined as stale (backend recorded a skip marker, so a retry
        // short-circuits without re-downloading): show the skipped
        // state with an explicit install-anyway choice instead of
        // re-offering the plain install prompt.
        staleSkippedVersion = update.version;
        confirming = false;
      }
    } catch (e) {
      console.error('[UPDATER] stage_deferred_update failed:', e);
      error = String(e);
    } finally {
      staging = false;
    }
  }

  async function confirmQuitInstall() {
    await stageForQuit(false);
  }

  async function installStaleAnyway() {
    await stageForQuit(true);
  }
</script>

{#if update && !dismissed}
  <div class="update-banner" role="region" aria-label={t('update.available', { version: update.version })}>
    <div class="update-info" role="status">
      <span class="update-title">{t('update.available', { version: update.version })}</span>
      {#if stagedVersion}
        <span class="update-staged">
          {currentVersion
            ? t('update.stagedVsCurrent', { staged: stagedVersion, current: currentVersion })
            : t('update.stagedQuit', { version: stagedVersion })}
        </span>
      {:else if confirming}
        <span class="update-confirm">
          {currentVersion
            ? t('update.confirmQuitInstall', { staged: update.version, current: currentVersion })
            : t('update.confirmQuitInstallUnknown', { staged: update.version })}
        </span>
      {:else if isStaleSkipped}
        <span class="update-stale">
          {currentVersion
            ? t('update.staleSkipped', { staged: staleSkippedVersion, current: currentVersion })
            : t('update.staleSkippedUnknown', { staged: staleSkippedVersion })}
        </span>
      {:else if downloading}
        <span class="update-progress">
          {Math.round(progress * 100)}%{totalBytes > 0
            ? ` (${Math.round(downloadedBytes / 1024 / 1024)}/${Math.round(totalBytes / 1024 / 1024)} MB)`
            : ''}
        </span>
      {:else if error}
        <span class="update-error">{t('update.downloadFailed', { error })}</span>
      {/if}
    </div>
    <div class="update-actions">
      <button
        type="button"
        class="download-btn"
        onclick={downloadAndInstall}
        disabled={downloading || staging}
      >
        {downloading ? t('update.downloading') : t('update.downloadAndInstall')}
      </button>
      {#if !stagedVersion}
        {#if confirming}
          <button
            type="button"
            class="quit-btn"
            onclick={confirmQuitInstall}
            disabled={downloading || staging}
          >
            {staging ? t('update.preparing') : t('update.installOnQuit')}
          </button>
          <button
            type="button"
            class="quit-btn"
            onclick={cancelQuitConfirm}
            disabled={downloading || staging}
          >
            {t('common.dismiss')}
          </button>
        {:else if isStaleSkipped}
          <button
            type="button"
            class="quit-btn"
            onclick={installStaleAnyway}
            disabled={downloading || staging}
          >
            {staging ? t('update.preparing') : t('update.installAnyway')}
          </button>
        {:else}
          <button
            type="button"
            class="quit-btn"
            onclick={openQuitConfirm}
            disabled={downloading || staging}
          >
            {t('update.installOnQuit')}
          </button>
        {/if}
      {/if}
      <button
        type="button"
        class="icon-btn dismiss-btn"
        onclick={() => (dismissed = true)}
        aria-label={t('update.dismissAria')}
        title={t('common.dismiss')}
        disabled={downloading || staging}
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
  .update-confirm {
    font-size: var(--fs-xs);
    color: var(--fg-muted);
  }
  .update-stale {
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
  .quit-btn {
    padding: var(--sp-2) var(--sp-4);
    font-size: var(--fs-sm);
    background: transparent;
    color: var(--fg-muted);
    border: 1px solid var(--fg-muted);
  }
  .quit-btn:hover:not(:disabled) {
    color: var(--fg);
    border-color: var(--fg);
  }
  .update-staged {
    font-size: var(--fs-xs);
    color: var(--success);
  }
  .dismiss-btn {
    width: 28px;
    height: 28px;
    font-size: var(--fs-lg);
    line-height: 1;
  }
</style>
