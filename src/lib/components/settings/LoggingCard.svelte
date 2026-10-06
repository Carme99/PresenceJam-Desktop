<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { onDestroy } from 'svelte';
  import { t } from '$lib/i18n';
  import type { AppConfig } from '$lib/types';
  import SettingsCard from './SettingsCard.svelte';

  interface Props {
    /** Bindable slice: the card edits `logging.*` in place. */
    logging: AppConfig['logging'];
  }

  let { logging = $bindable() }: Props = $props();

  /** Rust mirrors of `config.rs::clamp_logging` (1..=500 MB, 1..=20 files). */
  const LOG_MAX_FILE_SIZE_MB = { min: 1, max: 500 } as const;
  const LOG_KEEP_FILES = { min: 1, max: 20 } as const;
  /**
   * The wire values `config.rs::apply_log_level` matches (case-insensitively).
   * Shown verbatim: identifiers a log reader is looking for, not prose to translate.
   */
  const LOG_LEVELS = ['Off', 'Error', 'Warn', 'Info', 'Debug', 'Trace'] as const;

  // Issue #979: surface in-pane via the existing `saveMessage` channel
  // (same toast the rest of this view uses) instead of a console-only
  // warning. The backend now returns a non-empty error string for a
  // missing target instead of silently dispatching a no-op spawn.
  let saveMessage = $state('');
  let saveTimeout: ReturnType<typeof setTimeout> | null = null;

  async function openLogs() {
    saveMessage = '';
    try {
      await invoke('open_logs_folder');
    } catch (e) {
      console.warn('[SETTINGS] open_logs_folder failed:', e);
      const msg = String((e as Error)?.message ?? e).slice(0, 180);
      saveMessage = msg || t('logs.openFolderError');
      saveTimeout = setTimeout(() => (saveMessage = ''), 3000);
    }
  }

  onDestroy(() => {
    if (saveTimeout !== null) clearTimeout(saveTimeout);
  });
</script>

<!-- 4.7.0 (S5): log rotation. Rust owns the file target; this card is the
     only place `logging.*` is edited. `enabled`/`log_level` take effect
     immediately (config::apply_log_level, CfgDiag#4); size and retention
     are read once when the log plugin is built, i.e. at the next launch —
     the hint says so rather than implying an immediate effect. -->
<SettingsCard title={t('settings.sectionLogging')}>
  <div class="toggle-row">
    <label for="logging-enabled">{t('settings.loggingEnabledLabel')}</label>
    <input id="logging-enabled" type="checkbox" bind:checked={logging.enabled} />
  </div>
  <div class="form-group">
    <label for="log-level">{t('settings.logLevelLabel')}</label>
    <select id="log-level" bind:value={logging.log_level}>
      {#each LOG_LEVELS as level (level)}
        <option value={level}>{level}</option>
      {/each}
    </select>
  </div>
  <div class="row-2">
    <div class="form-group">
      <label for="log-max-size">{t('settings.logMaxSizeLabel')}</label>
      <input
        id="log-max-size"
        type="number"
        min={LOG_MAX_FILE_SIZE_MB.min}
        max={LOG_MAX_FILE_SIZE_MB.max}
        bind:value={logging.max_file_size_mb}
      />
    </div>
    <div class="form-group">
      <label for="log-keep-files">{t('settings.logKeepFilesLabel')}</label>
      <input
        id="log-keep-files"
        type="number"
        min={LOG_KEEP_FILES.min}
        max={LOG_KEEP_FILES.max}
        bind:value={logging.keep_files}
      />
    </div>
  </div>
  <p class="hint">{t('settings.logRotationHint')}</p>
  <button class="btn-secondary btn-full" onclick={openLogs}>
    {t('settings.openLogsFolder')}
  </button>
  {#if saveMessage}
    <p class="hint" role="status">{saveMessage}</p>
  {/if}
</SettingsCard>

<style>
  .form-group {
    display: flex;
    flex-direction: column;
    gap: var(--sp-2);
  }
  .form-group label {
    font-size: var(--fs-sm);
    font-weight: 600;
    color: var(--fg);
  }
  .row-2 {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: var(--sp-3);
  }
  @media (max-width: 480px) {
    .row-2 {
      grid-template-columns: 1fr;
    }
  }
  .toggle-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--sp-3);
    padding: var(--sp-2) 0;
  }
  .toggle-row label {
    font-size: var(--fs-base);
    color: var(--fg);
  }
  .btn-full.btn-secondary {
    background: var(--bg-elevated);
  }
</style>
