<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { onMount } from 'svelte';
  import PageHeader from './PageHeader.svelte';
  import { currentView } from '$lib/stores/app';
  import { t } from '$lib/i18n';
  import type { DiagnosticsSnapshot } from '$lib/types';
  import { devLog } from '$lib/utils/dev';

  /**
   * Telemetry-free local diagnostics page (scope-3.3 candidate C5).
   *
   * Collects a support snapshot via the `get_diagnostics_snapshot`
   * command — app/OS versions, sanitized config summary, token metadata
   * only (expiry timestamps, presence flags; never token values),
   * keychain presence flags, and the redacted tail of the on-disk log.
   * "Copy diagnostics" puts the JSON on the clipboard and "Save to file"
   * asks the backend to write it into the downloads folder, so the user
   * can attach it to a GitHub issue.
   * No network calls anywhere — matches SECURITY.md "No Telemetry".
   */

  let snapshot = $state<DiagnosticsSnapshot | null>(null);
  let loadError = $state('');
  let feedback = $state('');
  /** Guard so a double click cannot start two saves (issue #598). */
  let saving = $state(false);
  /**
   * #537: Dismiss only hides the banner for this session. Unlike the
   * #244 failed-install record — which `dismissFailedInstall` deletes,
   * because the payload it describes is disposable — the quarantine is the
   * user's only pointer to the settings they lost, so nothing on disk is
   * touched and the snapshot fields stay truthful in "Copy diagnostics".
   */
  let quarantineDismissed = $state(false);
  let loading = $derived(snapshot === null && loadError === '');

  // #537: the config file was unreadable, so every value in the summary
  // below is a factory default rather than the user's. Two independent
  // facts drive this: `config_quarantined` (this process did the
  // quarantine) and `config_quarantine_backup` (the `.bak` is still there,
  // possibly from an earlier launch — the flag is per-process, so without
  // the second the banner would vanish on the first restart, which is
  // exactly when the user notices their settings are gone).
  const quarantineBackup = $derived(
    snapshot?.config.config_quarantine_backup ?? null
  );
  const quarantineNotice = $derived(
    snapshot !== null &&
      !quarantineDismissed &&
      (snapshot.config.config_quarantined || quarantineBackup !== null)
  );
  const quarantineBody = $derived(
    snapshot?.config.config_quarantined
      ? t('diagnostics.quarantineBodyNow')
      : t('diagnostics.quarantineBodyEarlier')
  );
  const quarantineBackupBody = $derived(
    quarantineBackup
      ? t('diagnostics.quarantineBackupPresent', { name: quarantineBackup })
      : t('diagnostics.quarantineBackupMissing')
  );

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
      feedback = t('diagnostics.copied');
    } catch (e) {
      console.warn('[DIAGNOSTICS] clipboard write failed:', e);
      feedback = t('diagnostics.copyFailed');
    }
  }

  /**
   * Issue #598: the old implementation clicked a synthetic anchor on a
   * `blob:` URL and then reported success unconditionally. No download
   * handler is registered anywhere in the app, so on engines that ignore
   * an unhandled download (WebKitGTK) nothing was written while the user
   * was told the file had been saved. The backend now performs the write
   * and returns the path it created (dev-logged below), so success is
   * reported only for a file that actually exists.
   */
  async function saveToFile() {
    if (!snapshot || saving) return;
    saving = true;
    try {
      const path = await invoke<string>('save_diagnostics_snapshot', {
        json: formatText(snapshot)
      });
      devLog('[DIAGNOSTICS] snapshot saved to', path);
      feedback = t('diagnostics.savedToDownloads');
    } catch (e) {
      console.warn('[DIAGNOSTICS] save failed:', e);
      feedback = t('diagnostics.saveFailed');
    } finally {
      saving = false;
    }
  }
  /**
   * Issue #766: corrupt-key recovery affordance, next to the connections
   * card that shows the stored key/secret presence. The two-step shape
   * (arm, then confirm — never a single destructive click) names what is
   * deleted and the re-sign-in that follows; `resetting` serialises the
   * invoke, and after a success the snapshot is reloaded so the rows below
   * read the emptied state instead of the stale one.
   */
  let resetArmed = $state(false);
  let resetting = $state(false);

  function armTokenReset() {
    resetArmed = true;
  }

  function cancelTokenReset() {
    resetArmed = false;
  }

  async function confirmTokenReset() {
    if (resetting) return;
    resetting = true;
    try {
      await invoke('reset_local_token_storage');
      devLog('[DIAGNOSTICS] token storage reset');
      feedback = t('diagnostics.resetTokenDone');
      resetArmed = false;
      await loadSnapshot();
    } catch (e) {
      console.warn('[DIAGNOSTICS] reset_local_token_storage failed:', e);
      feedback = t('diagnostics.resetTokenFailed', { error: String(e) });
    } finally {
      resetting = false;
    }
  }
  function boolLabel(v: boolean | undefined | null): string {
    return v ? t('common.yes') : t('common.no');
  }

  /**
   * Issue #244: exit-time update installs happen after the event loop
   * ends, so a failure cannot be shown in that session. It is recorded
   * on disk and surfaced here (via the snapshot) on the next launch;
   * Dismiss discards the record both on disk and in the snapshot, so
   * "Copy diagnostics"/"Save to file" agree with the UI.
   */
  async function dismissFailedInstall() {
    try {
      await invoke('clear_failed_update_install');
      if (snapshot) snapshot.failed_update_install = null;
    } catch (e) {
      console.warn('[DIAGNOSTICS] clear_failed_update_install failed:', e);
      feedback = t('diagnostics.failedInstallDismissFailed');
    }
  }

  async function loadSnapshot() {
    loadError = '';
    snapshot = null;
    try {
      snapshot = await invoke<DiagnosticsSnapshot>('get_diagnostics_snapshot');
    } catch (e) {
      console.warn('[DIAGNOSTICS] get_diagnostics_snapshot failed:', e);
      loadError = String(e);
    }
  }

  onMount(() => {
    void loadSnapshot();
  });

</script>

<div class="diagnostics">
  <PageHeader title={t('diagnostics.title')} onBack={goBack} showLogo={false} showThemeToggle={false} />

  <div class="toolbar">
    <span class="hint">{t('diagnostics.localOnlyHint')}</span>
    <button class="btn-secondary" onclick={copyDiagnostics} disabled={!snapshot}>{t('diagnostics.copy')}</button>
    <button class="btn-secondary" onclick={saveToFile} disabled={!snapshot || saving}>{t('diagnostics.saveToFile')}</button>
  </div>

  <p class="feedback" role="status">{feedback}</p>

  <div class="content">
    {#if loading}
      <div class="empty-state">
        <p>{t('diagnostics.collecting')}</p>
      </div>
    {:else if loadError}
      <div class="empty-state" role="alert">
        <p>{t('diagnostics.collectFailed')}</p>
        <p class="hint">{loadError}</p>
        <button class="btn-secondary" onclick={loadSnapshot}>{t('common.retry')}</button>
      </div>
    {:else if snapshot}
      {#if quarantineNotice}
        <section class="quarantine" role="alert" aria-label={t('diagnostics.quarantineTitle')}>
          <h2>{t('diagnostics.quarantineTitle')}</h2>
          <p>{quarantineBody}</p>
          <p>{quarantineBackupBody}</p>
          <p class="hint">{t('diagnostics.quarantineWhere')}</p>
          <div class="quarantine-actions">
            <button class="btn-secondary" onclick={() => (quarantineDismissed = true)}>{t('common.dismiss')}</button>
          </div>
        </section>
      {/if}

      <section aria-label={t('diagnostics.versions')}>
        <h2>{t('diagnostics.versions')}</h2>
        <dl>
          <dt>{t('diagnostics.app')}</dt><dd>{snapshot.app_version}</dd>
          <dt>{t('diagnostics.tauri')}</dt><dd>{snapshot.tauri_version}</dd>
          <dt>{t('diagnostics.os')}</dt><dd>{snapshot.os.platform} ({snapshot.os.arch}, {snapshot.os.family})</dd>
        </dl>
      </section>

      {#if snapshot.failed_update_install}
        {@const failedInstall = snapshot.failed_update_install}
        <section class="failed-install" aria-label={t('diagnostics.failedInstallTitle')}>
          <h2>{t('diagnostics.failedInstallTitle')}</h2>
          <dl>
            <dt>{t('diagnostics.failedInstallVersion')}</dt><dd>{failedInstall.version}</dd>
            <dt>{t('diagnostics.failedInstallError')}</dt><dd class="mono">{failedInstall.error}</dd>
            <dt>{t('diagnostics.failedInstallTimestamp')}</dt><dd>{failedInstall.timestamp}</dd>
          </dl>
          <div class="failed-install-actions">
            <button class="btn-secondary" onclick={dismissFailedInstall}>{t('common.dismiss')}</button>
          </div>
        </section>
      {/if}

      <section aria-label={t('diagnostics.configuration')}>
        <h2>{t('diagnostics.configuration')}</h2>
        <dl>
          <dt>{t('diagnostics.spotifyClientId')}</dt><dd class="mono">{snapshot.config.spotify_client_id || t('diagnostics.notSet')}</dd>
          <dt>{t('diagnostics.redirectUri')}</dt><dd class="mono">{snapshot.config.redirect_uri}</dd>
          <dt>{t('diagnostics.clientSecretKeychain')}</dt><dd>{boolLabel(snapshot.config.client_secret_set)}</dd>
          <dt>{t('diagnostics.clearOnPause')}</dt><dd>{boolLabel(snapshot.config.clear_on_pause)}</dd>
          <dt>{t('diagnostics.profanityFilter')}</dt><dd>{boolLabel(snapshot.config.profanity_filter)}</dd>
          <dt>{t('diagnostics.startMinimized')}</dt><dd>{boolLabel(snapshot.config.start_minimized)}</dd>
          <dt>{t('diagnostics.availabilitySync')}</dt><dd>{boolLabel(snapshot.config.availability_sync)}</dd>
          <dt>{t('diagnostics.presenceGate')}</dt><dd>{boolLabel(snapshot.config.presence_gate)}</dd>
          <dt>{t('diagnostics.pollInterval')}</dt>
          <dd>{snapshot.config.default_interval_seconds}s / {snapshot.config.minimum_interval_seconds}s / {snapshot.config.maximum_interval_seconds}s</dd>
          <dt>{t('diagnostics.expiryBuffer')}</dt><dd>{snapshot.config.expiry_buffer_seconds}s</dd>
          <dt>{t('diagnostics.logging')}</dt>
          <dd>{snapshot.config.logging_enabled ? t('diagnostics.loggingEnabled', { level: snapshot.config.log_level }) : t('diagnostics.loggingDisabled')}</dd>
          <dt>{t('diagnostics.launchAtLogin')}</dt><dd>{boolLabel(snapshot.config.autostart)}</dd>
          <dt>{t('diagnostics.statusRules')}</dt><dd>{t('diagnostics.statusRulesValue', { quiet: snapshot.config.quiet_hours_enabled_count, quietTotal: snapshot.config.quiet_hours_count, rules: snapshot.config.track_rules_enabled_count, rulesTotal: snapshot.config.track_rules_count })}</dd>
        </dl>
      </section>

      <section aria-label={t('diagnostics.connections')}>
        <h2>{t('diagnostics.connections')}</h2>
        <dl>
          <dt>{t('diagnostics.spotifyConnected')}</dt><dd>{boolLabel(snapshot.tokens.spotify_connected)}</dd>
          <dt>{t('diagnostics.spotifyTokenExpires')}</dt><dd>{snapshot.tokens.spotify_expires_at ?? '—'}{snapshot.tokens.spotify_expired ? t('diagnostics.expired') : ''}</dd>
          <dt>{t('diagnostics.teamsConnected')}</dt><dd>{boolLabel(snapshot.tokens.teams_connected)}</dd>
          <dt>{t('diagnostics.teamsTokenExpires')}</dt><dd>{snapshot.tokens.teams_expires_at ?? '—'}{snapshot.tokens.teams_expired ? t('diagnostics.expired') : ''}</dd>
          <dt>{t('diagnostics.teamsRefreshTokenPresent')}</dt><dd>{boolLabel(snapshot.tokens.teams_refresh_token_present)}</dd>
          <dt>{t('diagnostics.keychainSpotifySecret')}</dt><dd>{boolLabel(snapshot.keychain.spotify_client_secret_present)}</dd>
          <dt>{t('diagnostics.keychainEncryptionKey')}</dt><dd>{boolLabel(snapshot.keychain.tokens_encryption_key_present)}</dd>
          <dt>{t('diagnostics.syncRunning')}</dt><dd>{boolLabel(snapshot.sync_state.is_syncing)}</dd>
          <dt>{t('diagnostics.syncSnoozed')}</dt><dd>{snapshot.sync_state.snoozed ? t('diagnostics.syncSnoozeMinutes', { minutes: snapshot.sync_state.snooze_minutes_left ?? 0 }) : boolLabel(false)}</dd>
          <dt>{t('diagnostics.syncManualStatusBlocks')}</dt><dd>{boolLabel(snapshot.sync_state.manual_status_blocks)}</dd>
          <dt>{t('diagnostics.syncPresenceGateReason')}</dt><dd>{snapshot.sync_state.presence_gate_reason ?? '—'}</dd>
          <dt>{t('diagnostics.syncTransientFailures')}</dt><dd>{snapshot.sync_state.transient_failure_count}</dd>
          <dt>{t('diagnostics.syncNetworkFailures')}</dt><dd>{snapshot.sync_state.consecutive_network_failures}</dd>
        </dl>
        <p class="hint">{t('diagnostics.tokensNeverIncluded')}</p>
        {#if !resetArmed}
          <div class="reset-actions">
            <button class="btn-secondary" onclick={armTokenReset}>{t('diagnostics.resetTokenStorage')}</button>
          </div>
        {:else}
          <div class="reset-confirm" role="alert">
            <span class="hint">{t('diagnostics.resetTokenConfirm')}</span>
            <div class="reset-actions">
              <button class="btn-secondary" onclick={confirmTokenReset} disabled={resetting}>{t('diagnostics.resetTokenStorage')}</button>
              <button class="btn-secondary" onclick={cancelTokenReset} disabled={resetting}>{t('common.dismiss')}</button>
            </div>
          </div>
        {/if}
      </section>

      <section aria-label={t('diagnostics.recentLogLines')}>
        <h2>{t('diagnostics.recentLogLines')}</h2>
        <p class="hint">{snapshot.log_source_status}</p>
        {#if snapshot.recent_logs.length === 0}
          <div class="empty-state small">
            <p>{t('diagnostics.noLogLinesYet')}</p>
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
  /* #751: `.hint` is global now. Only its toolbar placement is local — the
     hint is the flexible item in that flex row, so it absorbs the slack and
     keeps the two action buttons on the right. */
  .toolbar .hint { margin-right: auto; }
  /* The load-error hint sits in the centred empty-state column, where the
     auto margin is what pinned it left; the other hints are in block
     containers, where it never had an effect. */
  .empty-state .hint { margin-right: auto; }

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

  /* #537: a settings-loss warning, not an error — the app is fine, the
     user's configured values are not. Amber reads as "needs attention"
     without competing with the red failed-install card (#244). */
  .quarantine {
    border-color: var(--warning);
  }
  .quarantine h2 {
    color: var(--warning);
  }
  .quarantine p {
    margin: 0 0 var(--sp-1);
    font-size: var(--fs-sm);
    color: var(--fg);
  }
  .quarantine .hint {
    margin-right: 0;
  }
  .quarantine-actions {
    display: flex;
    justify-content: flex-end;
    margin-top: var(--sp-2);
  }
  .quarantine-actions .btn-secondary {
    width: auto;
    padding: var(--sp-2) var(--sp-4);
    font-size: var(--fs-sm);
  }
  .reset-actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--sp-2);
    margin-top: var(--sp-2);
  }
  .reset-actions .btn-secondary {
    width: auto;
    padding: var(--sp-2) var(--sp-4);
    font-size: var(--fs-sm);
  }
  .reset-confirm {
    margin-top: var(--sp-2);
  }
  .reset-confirm .hint {
    margin-right: 0;
  }

  .failed-install {
    border-color: var(--danger);
  }
  .failed-install h2 {
    color: var(--danger);
  }
  .failed-install-actions {
    display: flex;
    justify-content: flex-end;
    margin-top: var(--sp-2);
  }
  .failed-install-actions .btn-secondary {
    width: auto;
    padding: var(--sp-2) var(--sp-4);
    font-size: var(--fs-sm);
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
