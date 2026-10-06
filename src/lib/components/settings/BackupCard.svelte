<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { t } from '$lib/i18n';
  import type { AppConfig } from '$lib/types';
  import SettingsCard from './SettingsCard.svelte';

  interface Props {
    /** The import path adopts the on-disk document; the parent re-snapshots. */
    onimported?: () => void;
  }

  let { onimported }: Props = $props();

  let backupBusy = $state(false);
  let backupMessage = $state('');

  /** `{ path, config }` — the Rust `ImportOutcome` (commands/config.rs). */
  type ImportOutcome = { path: string; config: AppConfig };

  function backupError(e: unknown): string {
    return String((e as Error)?.message ?? e).slice(0, 180);
  }

  async function exportConfig() {
    if (backupBusy) return;
    backupBusy = true;
    backupMessage = '';
    try {
      const path = await invoke<string | null>('export_config', {
        title: t('settings.backupExportDialogTitle')
      });
      // `null` = the dialog was dismissed: not a failure, and no message.
      if (path) backupMessage = t('settings.backupExported', { path });
    } catch (e) {
      console.error('[SETTINGS] export_config failed:', e);
      backupMessage = t('settings.backupError', { error: backupError(e) });
    } finally {
      backupBusy = false;
    }
  }

  async function importConfig() {
    if (backupBusy) return;
    backupBusy = true;
    backupMessage = '';
    try {
      // Both the picker and the overwrite confirmation live in the
      // `import_config` command (Rust), so the same dialog appears in the main
      // window and in a popped-out Settings pane — the JS dialog plugin is
      // ACL-gated per window, and granting it to detached panes would hand them
      // the file dialogs too. Only the *copy* is localized here: Rust has no
      // dictionary, so the title, the body and both button labels arrive as
      // arguments and every one of them goes through `t()`.
      const outcome = await invoke<ImportOutcome | null>('import_config', {
        title: t('settings.backupImportDialogTitle'),
        confirmBody: t('settings.backupConfirmOverwrite'),
        confirmOk: t('common.yes'),
        confirmCancel: t('common.no')
      });
      if (!outcome) return;
      onimported?.();
      backupMessage = t('settings.backupImported', { path: outcome.path });
    } catch (e) {
      console.error('[SETTINGS] import_config failed:', e);
      // Rust refuses an import that carries a plaintext client_secret and
      // names the offending path; surface that text rather than a generic
      // failure, since it is the only way the user learns why.
      backupMessage = t('settings.backupError', { error: backupError(e) });
    } finally {
      backupBusy = false;
    }
  }
</script>

<!-- 4.7.0 (S5): backup. Both actions run in Rust, which owns the file
     dialogs and the resolved path; the export never carries the Spotify
     client secret (keychain-only) and the import refuses a document that
     does. -->
<SettingsCard title={t('settings.sectionBackup')}>
  <p class="hint">{t('settings.backupHint')}</p>
  <div class="row-2">
    <button class="btn-secondary btn-full" onclick={exportConfig} disabled={backupBusy}>
      {t('settings.backupExport')}
    </button>
    <button class="btn-secondary btn-full" onclick={importConfig} disabled={backupBusy}>
      {t('settings.backupImport')}
    </button>
  </div>
  {#if backupMessage}
    <p class="hint" role="status">{backupMessage}</p>
  {/if}
</SettingsCard>

<style>
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
  .btn-full.btn-secondary {
    background: var(--bg-elevated);
  }
</style>
