<script lang="ts">
  import { t } from '$lib/i18n';
  import type { AppConfig } from '$lib/types';
  import SettingsCard from './SettingsCard.svelte';

  interface Props {
    /** Bindable slice: the card edits the presence-gate fields on `teams.*`. */
    teams: AppConfig['teams'];
    /** Reset callback: restore shipped defaults. */
    onreset: () => void;
  }

  let { teams = $bindable(), onreset }: Props = $props();
</script>

<SettingsCard title={t('settings.sectionPresence')} resetLabel={t('common.resetToDefault')} {onreset}>
  <div class="toggle-row">
    <label for="availability-sync">{t('settings.availabilitySyncLabel')}</label>
    <input
      id="availability-sync"
      type="checkbox"
      bind:checked={teams.availability_sync}
    />
  </div>
  <p class="hint">
    {t('settings.availabilitySyncHint')}
  </p>
  <div class="toggle-row">
    <label for="presence-gate">{t('settings.presenceGateLabel')}</label>
    <input
      id="presence-gate"
      type="checkbox"
      bind:checked={teams.presence_gate}
    />
  </div>
  <p class="hint">
    {t('settings.presenceGateHint')}
  </p>
  <!-- Findings #635/#637: the manual-status policy (ON by default) and the
       opt-in out-of-office gate, in the card the meeting/call gate lives in. -->
  <div class="toggle-row">
    <label for="respect-manual-status">{t('settings.respectManualStatusLabel')}</label>
    <input
      id="respect-manual-status"
      type="checkbox"
      bind:checked={teams.respect_manual_status}
    />
  </div>
  <p class="hint">
    {t('settings.respectManualStatusHint')}
  </p>
  <div class="toggle-row">
    <label for="gate-out-of-office">{t('settings.gateOutOfOfficeLabel')}</label>
    <input
      id="gate-out-of-office"
      type="checkbox"
      bind:checked={teams.gate_when_out_of_office}
    />
  </div>
  <p class="hint">
    {t('settings.gateOutOfOfficeHint')}
  </p>
  <!-- Issue #872: OS-level presentation gate (full-screen app, slide
       deck, Windows Focus Assist Quiet Time). OFF by default; the
       toggle is a no-op on Linux/macOS where the probe always
       returns `Unknown`. -->
  <div class="toggle-row">
    <label for="gate-when-presenting">{t('settings.gateWhenPresentingLabel')}</label>
    <input
      id="gate-when-presenting"
      type="checkbox"
      bind:checked={teams.gate_when_presenting}
    />
  </div>
  <p class="hint">
    {t('settings.gateWhenPresentingHint')}
  </p>
  <!-- Issue #873: desktop-idle gate. `0` (the default) keeps 4.7
       behaviour; non-zero values are clamped to 60–3600 by the
       Rust loader. The number field sits next to the toggle so the
       reason the gate fires is clear from the form. -->
  <div class="toggle-row">
    <label for="idle-away-after-seconds">{t('settings.idleAwayLabel')}</label>
    <input
      id="idle-away-after-seconds"
      type="number"
      min="0"
      max="3600"
      step="60"
      bind:value={teams.idle_away_after_seconds}
    />
  </div>
  <p class="hint">
    {t('settings.idleAwayHint')}
  </p>
</SettingsCard>

<style>
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
</style>
