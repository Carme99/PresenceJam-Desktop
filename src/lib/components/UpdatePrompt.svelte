<script lang="ts">
  import { get } from 'svelte/store';
  import { onMount } from 'svelte';
  import { check } from '@tauri-apps/plugin-updater';
  import { invoke } from '@tauri-apps/api/core';
  import { getVersion } from '@tauri-apps/api/app';
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';
  import { t } from '$lib/i18n';
  import { configHydrated, configStore, loadConfig } from '$lib/stores/config';

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
  let update = $state<UpdateInfo | null>(null);
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
  // #590: live position of the deferred stage, streamed on
  // `update-stage-progress` while `stage_deferred_update` downloads.
  // `staging` alone only drives the "Preparing…" label; `stageAborted`
  // marks a stage the user walked away from mid-download (the Rust side
  // owns the transfer and cannot be interrupted, so the outcome is
  // discarded when it lands — see stageForQuit).
  let stageDownloaded = $state(0);
  let stageTotal = $state<number | null>(null);
  let stageAborted = $state(false);
  let cancelling = $state(false);
  // #431: `confirming` shows the install/skip choice surface;
  // `currentVersion` is the running build (best-effort — the banner
  // falls back to version-agnostic strings when it stays empty);
  // `staleSkippedVersion` remembers a candidate declined as stale so the
  // same version is not re-offered (the backend skip marker persists
  // this across launches).
  let confirming = $state(false);
  let currentVersion = $state('');
  let staleSkippedVersion = $state('');

  // Mirrors the backend `UpdateCheckOutcome` returned by `check_for_update`
  // (kept local, same convention as StageOutcome). `notes` and `pub_date`
  // are the manifest's release notes and publish date.
  interface UpdateInfo {
    version: string;
    notes: string | null;
    pub_date: string | null;
  }

  // Mirrors the backend `StageDeferredOutcome` shape (kept local so no
  // generated types need to change for this slice). `skipped` (#957) says
  // WHICH nothing-to-do a `staged: null` outcome was — the reason names are
  // the backend's `SKIP_REASON_*` constants — and is absent on the staged
  // case (`skip_serializing_if`), hence optional.
  interface StageOutcome {
    staged: string | null;
    current: string;
    skipped?: 'current' | 'stale' | 'already-skipped' | null;
  }

  // Mirrors the backend `StageProgress` shape emitted on
  // `update-stage-progress` (kept local, same convention as StageOutcome).
  // `total` is `null` when the server sent no `Content-Length`.
  interface StageProgress {
    downloaded: number;
    total: number | null;
  }

  // 4.7.0 (issue #678): the candidate comes from the backend, which resolves
  // `config.updates.channel`; the immediate JS `downloadAndInstall()` below
  // downloads from the plugin's static `plugins.updater.endpoints` entry
  // (pinned to the stable manifest), so it is offered on Stable only.
  //
  // `configStore` is hydrated HERE rather than assumed: the layout mounts this
  // banner from the start, while boot loads the config on its own schedule, so
  // nothing guarantees the store holds the user's persisted channel by the time
  // this renders. Reading the mirror's default (`stable`) would offer the
  // stable-only JS path on a Beta install.
  let channelResolved = $state(false);
  const isBeta = $derived($configStore.updates.channel === 'beta');

  // #977: the channel the banner's current candidate was resolved with.
  // `check_for_update` reads `config.updates.channel` from disk on every
  // call, so a switch in Settings has to be followed by a fresh check:
  // otherwise the previous channel's candidate stays on offer for up to a
  // day, and the version staged at quit time can differ from the one the
  // quit-time confirm row showed.
  let checkedChannel = $state('');

  let isStaleSkipped = $derived(
    update !== null && staleSkippedVersion !== '' && staleSkippedVersion === update.version
  );

  // #678: the backend's check also returns the manifest's release notes and
  // publish date. The banner is a one-line strip, so they surface as its
  // tooltip instead of as another row.
  const updateTooltip = $derived(
    [update?.notes, update?.pub_date].filter((part): part is string => Boolean(part)).join('\n\n')
  );

  function checkForUpdate() {
    // #977: remember the channel this check runs against so the effect below
    // can tell a Settings switch apart from the hydration flip. Before
    // hydration the store still holds the mirror's default, so recording
    // that would make the hydrated value look like a user change — and the
    // backend reads the channel from disk, so this check's candidate already
    // belongs to the persisted channel.
    if (channelResolved) checkedChannel = $configStore.updates.channel;
    // The backend resolves the configured channel into the endpoint list —
    // the plugin's JS `check()` cannot take endpoints and is hard-wired to
    // the static stable entry (issue #678).
    invoke<UpdateInfo | null>('check_for_update')
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
        } else {
          // The running build is current: a banner with no candidate behind
          // it must go away rather than keep offering the old version.
          update = null;
        }
      })
      .catch((e) => {
        // Offline / unreachable endpoint / mismatched pubkey etc. — never
        // surface a failed update check to the user, whether at startup
        // or from a background timer.
        console.error('[UPDATER] check failed:', e);
      });
  }

  // #977: a channel switch in Settings republishes the persisted document
  // into `configStore` (`saveConfig`/`updateConfig`), and that is the signal
  // this banner needs — the backend re-reads the channel per command, so no
  // new backend event is required.
  $effect(() => {
    const channel = $configStore.updates.channel;
    // A stage or a download in flight owns the banner: its cancel
    // affordance lives in it, so the switch is picked up as soon as they
    // settle rather than tearing the banner out from under the user.
    const busy = staging || downloading;
    if (!channelResolved) return;
    if (checkedChannel === '') {
      // The first check ran before hydration: adopt the hydrated channel as
      // the baseline instead of re-checking against a value that only just
      // arrived from disk.
      checkedChannel = channel;
      return;
    }
    if (channel === checkedChannel || busy) return;
    // A candidate from the channel the user just left must not survive the
    // switch (nor a dismissal scoped to it).
    update = null;
    staleSkippedVersion = '';
    confirming = false;
    dismissed = false;
    checkForUpdate();
  });

  onMount(() => {
    // Point-of-use hydration (see above), in the two orders this banner can
    // find the store:
    //  - already hydrated (`configHydrated`) — boot went through the store, so
    //    the channel is authoritative now and the gate opens on the first paint
    //    instead of behind a redundant re-read;
    //  - not hydrated — `loadConfig` is the store's own entry point and caches
    //    an in-flight promise, so this joins boot's read rather than racing it,
    //    and never rejects (it falls back to the defaults and logs).
    if (get(configHydrated)) channelResolved = true;
    else loadConfig().finally(() => (channelResolved = true));
    checkForUpdate();
    const interval = setInterval(checkForUpdate, CHECK_INTERVAL_MS);
    // #590: staging progress. `listen()` resolves asynchronously, so an
    // unmount before it does must release the subscription immediately —
    // the `destroyed` guard the other listener sites in this app use.
    let destroyed = false;
    let unlistenStage: UnlistenFn | null = null;
    listen<StageProgress>('update-stage-progress', (event) => {
      stageDownloaded = event.payload.downloaded;
      stageTotal = event.payload.total;
    }).then((fn) => {
      if (destroyed) fn();
      else unlistenStage = fn;
    });
    return () => {
      destroyed = true;
      clearInterval(interval);
      if (unlistenStage) unlistenStage();
    };
  });

  const progress = $derived(
    totalBytes > 0 ? Math.min(downloadedBytes / totalBytes, 1) : 0
  );

  // #590: whole-percent position of the deferred stage, or `null` when the
  // payload size is unknown — the row then renders its indeterminate copy
  // instead of a percentage that would be a guess.
  const stagePercent = $derived(
    stageTotal !== null && stageTotal > 0
      ? Math.min(Math.round((stageDownloaded / stageTotal) * 100), 100)
      : null
  );

  // #737: which byte-level position is on screen, if any. The deferred stage
  // keeps the precedence the banner's single status chain gave it, and the
  // rows those branches used to shadow stay suppressed so the strip never
  // shows two conflicting lines.
  const stageProgress = $derived(staging && !stageAborted);
  const downloadProgress = $derived(
    downloading && !stageProgress && !stagedVersion && !confirming && !isStaleSkipped
  );

  // #737: the progressbar's accessible name — the update it belongs to.
  // Deliberately independent of the percentage so assistive tech reads the
  // position on demand instead of being told on every emitted tick.
  const progressLabel = $derived(
    update ? t('update.available', { version: update.version }) : ''
  );

  async function downloadAndInstall() {
    if (!update || downloading) return;
    downloading = true;
    error = '';
    downloadedBytes = 0;
    totalBytes = 0;
    try {
      // The banner's candidate came from the backend; this path needs the
      // plugin's own update handle, so it re-resolves it here. Offered on the
      // Stable channel only, where the plugin's static endpoint entry and the
      // channel's resolved list are the same manifest (issue #678).
      const candidate = await check();
      if (!candidate) {
        // The release vanished between the banner's check and this click:
        // drop the banner instead of offering an install that cannot run.
        update = null;
        downloading = false;
        return;
      }
      await candidate.downloadAndInstall((event) => {
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
    stageAborted = false;
    stageDownloaded = 0;
    stageTotal = null;
    error = '';
    try {
      const outcome = await invoke<StageOutcome>('stage_deferred_update', { force });
      // Backend truth for the running version — always populated, so
      // the staged state can show both versions unconditionally.
      currentVersion = outcome.current;
      if (stageAborted) {
        // #590: the user cancelled while the payload was downloading. The
        // Rust side owns the transfer and cannot be interrupted, so the
        // bytes landed anyway — discard them instead of advertising a
        // stage the user already walked away from.
        stageAborted = false;
        if (outcome.staged) {
          await invoke('cancel_deferred_update').catch((e) => {
            console.error('[UPDATER] cancel_deferred_update (after cancel) failed:', e);
          });
        }
        return;
      }
      if (outcome.staged) {
        stagedVersion = outcome.staged;
        confirming = false;
        staleSkippedVersion = '';
      } else if (outcome.skipped === 'current') {
        // #957: nothing to do — the manifest no longer offers the version
        // this banner cached (a re-cut or rolled-back release), so there is
        // no update to install and no stale decline to report. Drop the
        // candidate (the banner goes with it) instead of rendering the
        // stale-skip copy with an "Install anyway" button that would only
        // repeat the same no-op.
        staleSkippedVersion = '';
        confirming = false;
        update = null;
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
      stageAborted = false;
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

  // #590: abandons the deferred stage and returns the banner to its plain
  // update offer. A download already in flight cannot be stopped
  // Rust-side, so that case marks the stage abandoned and `stageForQuit`
  // discards the payload when it lands; an already-staged payload is
  // dropped immediately by `cancel_deferred_update`, which also releases
  // the verified bytes instead of holding them for the rest of the session.
  async function cancelStage() {
    if (cancelling) return;
    cancelling = true;
    error = '';
    if (staging) stageAborted = true;
    try {
      await invoke('cancel_deferred_update');
      stagedVersion = '';
      stageDownloaded = 0;
      stageTotal = null;
      confirming = false;
    } catch (e) {
      // The payload is still held Rust-side, so the banner must keep
      // saying so rather than claiming the stage is gone.
      if (staging) stageAborted = false;
      console.error('[UPDATER] cancel_deferred_update failed:', e);
      error = String(e);
    } finally {
      cancelling = false;
    }
  }
</script>

{#if update && !dismissed}
  <div
    class="update-banner"
    role="region"
    aria-label={t('update.available', { version: update.version })}
    title={updateTooltip}
  >
    <div class="update-info" role="status">
      <span class="update-title">{t('update.available', { version: update.version })}</span>
      {#if stageProgress}
        <!-- #737: the deferred stage's byte position is carried by the
             progressbar below, outside this live region, so the polite queue
             is not rewritten on every emitted tick. What stays here is the
             discrete stage transitions — staged, declined as stale, failed. -->
      {:else if stagedVersion}
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
      {:else if downloadProgress}
        <!-- #982: the immediate download's position lives in the progressbar
             below as well. -->
      {:else if error}
        <span class="update-error">{t('update.downloadFailed', { error })}</span>
      {/if}
      {#if isBeta}
        <span class="update-beta">{t('update.betaOnQuitOnly')}</span>
      {/if}
    </div>
    {#if stageProgress}
      <span
        class="update-progress"
        role="progressbar"
        aria-label={progressLabel}
        aria-valuemin="0"
        aria-valuemax="100"
        aria-valuenow={stagePercent ?? undefined}
      >
        {stagePercent === null
          ? t('update.preparing')
          : t('update.stagingProgress', { percent: stagePercent })}
      </span>
    {:else if downloadProgress}
      <!-- #982: without a `Content-Length` there is no position to report, so
           the row says so instead of holding a frozen "0%" for the whole
           download — the rule `stagePercent` already follows for the
           deferred path. -->
      <span
        class="update-progress"
        role="progressbar"
        aria-label={progressLabel}
        aria-valuemin="0"
        aria-valuemax="100"
        aria-valuenow={totalBytes > 0 ? Math.round(progress * 100) : undefined}
      >
        {totalBytes > 0
          ? `${Math.round(progress * 100)}% (${Math.round(downloadedBytes / 1024 / 1024)}/${Math.round(totalBytes / 1024 / 1024)} MB)`
          : t('update.preparing')}
      </span>
    {/if}
    <div class="update-actions">
      {#if channelResolved && !isBeta}
        <button
          type="button"
          class="download-btn"
          onclick={downloadAndInstall}
          disabled={downloading || staging}
        >
          {downloading ? t('update.downloading') : t('update.downloadAndInstall')}
        </button>
      {/if}
      {#if staging || stagedVersion}
        <button
          type="button"
          class="quit-btn"
          onclick={cancelStage}
          disabled={cancelling || stageAborted}
        >
          {t('update.cancelStage')}
        </button>
      {:else if confirming}
        <button
          type="button"
          class="quit-btn"
          onclick={confirmQuitInstall}
          disabled={downloading}
        >
          {t('update.installOnQuit')}
        </button>
        <button
          type="button"
          class="quit-btn"
          onclick={cancelQuitConfirm}
          disabled={downloading}
        >
          {t('common.dismiss')}
        </button>
      {:else if isStaleSkipped}
        <button
          type="button"
          class="quit-btn"
          onclick={installStaleAnyway}
          disabled={downloading}
        >
          {t('update.installAnyway')}
        </button>
      {:else}
        <button
          type="button"
          class="quit-btn"
          onclick={openQuitConfirm}
          disabled={downloading}
        >
          {t('update.installOnQuit')}
        </button>
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
    /* #951: docked to the bottom edge instead of floating over the top
       chrome. Centred at `top: var(--sp-3)` the banner sat on the Dashboard
       header's icon row at the default window, so the theme, logs,
       diagnostics, settings and about buttons were unclickable while an
       update was offered. The bottom placement follows the `.playback-toast`
       convention in `+layout.svelte`: it covers no chrome, and the z-index
       stays below that toast so a playback error still wins. */
    position: fixed;
    bottom: var(--sp-3);
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
    /* #950: the column owns the free space in the strip, and `min-width: 0`
       is what lets the ellipsis rules below cut a long title instead of
       letting it paint underneath the buttons. */
    flex: 1 1 auto;
    min-width: 0;
  }
  .update-title {
    font-size: var(--fs-sm);
    font-weight: 700;
    color: var(--fg);
  }
  /* #950: every info row is one line in the strip — a long release title or
     beta note is cut with an ellipsis rather than wrapping the banner into a
     tall block or spilling past its rounded border. The error row is
     deliberately left wrapping: its message is diagnostic and has to stay
     readable. */
  .update-title,
  .update-progress,
  .update-confirm,
  .update-stale,
  .update-beta,
  .update-staged {
    overflow: hidden;
    text-overflow: ellipsis;
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
  .update-beta {
    font-size: var(--fs-xs);
    color: var(--fg-muted);
  }
  .update-error {
    font-size: var(--fs-xs);
    color: var(--danger);
  }
  .update-actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--sp-2);
    row-gap: var(--sp-1);
    /* #950: the group yields to the strip instead of pushing the flex line
       past its border — its automatic minimum size keeps every button whole
       and wraps them onto a second row when the window is too narrow. */
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
