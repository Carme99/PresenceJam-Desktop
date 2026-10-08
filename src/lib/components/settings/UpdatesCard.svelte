<script lang="ts">
  import { t } from '$lib/i18n';
  import type { AppConfig } from '$lib/types';
  import SettingsCard from './SettingsCard.svelte';

  interface Props {
    /** Bindable slice: the card edits `updates.channel` in place. */
    updates: AppConfig['updates'];
  }

  let { updates = $bindable() }: Props = $props();
</script>

<!-- 4.7.0 (issue #678): release channel the updater reads. The saved
     value is the backend's single source of truth — the banner and the
     deferred staging path both resolve it on every check. -->
<SettingsCard title={t('settings.sectionUpdates')}>
  <div class="form-group">
    <label for="update-channel">{t('settings.updateChannelLabel')}</label>
    <select
      id="update-channel"
      value={updates.channel}
      onchange={(e) => {
        const value = (e.currentTarget as HTMLSelectElement).value;
        updates.channel = value === 'beta' ? 'beta' : 'stable';
      }}
    >
      <option value="stable">{t('settings.updateChannelStable')}</option>
      <option value="beta">{t('settings.updateChannelBeta')}</option>
    </select>
  </div>
  <p class="hint">{t('settings.updateChannelHint')}</p>
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
</style>
