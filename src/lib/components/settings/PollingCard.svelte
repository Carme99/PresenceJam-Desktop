<script lang="ts">
  import { t } from '$lib/i18n';
  import type { AppConfig } from '$lib/types';
  import SettingsCard from './SettingsCard.svelte';

  interface Props {
    /** Bindable slice: the card edits `polling.*` in place. */
    polling: AppConfig['polling'];
    /** Reset callback: restore shipped defaults. */
    onreset: () => void;
  }

  let { polling = $bindable(), onreset }: Props = $props();

  // C9: effective polling bounds, mirroring Rust `clamp_polling`
  // (`config::clamp_polling`): minimum clamps to [5, 30] first, then
  // maximum clamps to [effectiveMinimum, 300]. An entered max below min
  // is silently raised on save; the hint surfaces that effective value.
  let pollingClamp = $derived.by(() => {
    const rawMin = Number(polling.minimum_interval_seconds);
    const rawMax = Number(polling.max_interval_seconds);
    const effMin = Math.min(30, Math.max(5, rawMin));
    const effMax = Math.min(300, Math.max(effMin, rawMax));
    return { active: rawMin > rawMax, effMin, effMax };
  });

  // Frontend mirror of `clamp_polling` for the pause-backoff ceiling
  // (60..=3600): a typed value shows the value the backend will store.
  const PAUSE_BACKOFF_MIN_SECONDS = 60;
  const PAUSE_BACKOFF_MAX_SECONDS = 3600;
  let pauseBackoffClamp = $derived.by(() => {
    const raw = Number(polling.pause_backoff_max_seconds);
    const effective = Math.min(
      PAUSE_BACKOFF_MAX_SECONDS,
      Math.max(PAUSE_BACKOFF_MIN_SECONDS, Number.isFinite(raw) ? raw : 300)
    );
    return { active: effective !== raw, effective };
  });
</script>

<SettingsCard title={t('settings.sectionPolling')} resetLabel={t('common.resetToDefault')} {onreset}>
  <div class="form-group">
    <label for="default-interval">{t('settings.defaultIntervalLabel', { seconds: Number(polling.default_interval_seconds) })}</label>
    <input
      id="default-interval"
      type="range"
      min="10"
      max="60"
      step="5"
      bind:value={polling.default_interval_seconds}
    />
  </div>
  <div class="row-2">
    <div class="form-group">
      <label for="min-interval">{t('settings.minIntervalLabel')}</label>
      <input
        id="min-interval"
        type="number"
        min="5"
        max="30"
        bind:value={polling.minimum_interval_seconds}
      />
    </div>
    <div class="form-group">
      <label for="max-interval">{t('settings.maxIntervalLabel')}</label>
      <!-- `min` tracks clamp_polling's effective minimum; `max` is the
           backend's fixed upper bound (config.rs) — `pollingClamp.effMax`
           depends on the entered max, so using it here would be
           self-referential. -->
      <input
        id="max-interval"
        type="number"
        min={pollingClamp.effMin}
        max="300"
        bind:value={polling.max_interval_seconds}
      />
    </div>
  </div>
  <!-- Issue #538: `polling.pause_backoff_max_seconds` was config-only
       until 4.6. `min`/`max` mirror Rust's `clamp_polling` (60..=3600) and
       the hint reports the effective value a typed value would land on. -->
  <div class="form-group">
    <label for="pause-backoff-max">
      {t('settings.pauseBackoffMaxLabel')}
    </label>
    <input
      id="pause-backoff-max"
      type="number"
      min={PAUSE_BACKOFF_MIN_SECONDS}
      max={PAUSE_BACKOFF_MAX_SECONDS}
      bind:value={polling.pause_backoff_max_seconds}
    />
    {#if pauseBackoffClamp.active}
      <p class="clamp-hint" role="status">
        {t('settings.pauseBackoffClampHint', {
          min: PAUSE_BACKOFF_MIN_SECONDS,
          max: PAUSE_BACKOFF_MAX_SECONDS,
          effective: pauseBackoffClamp.effective
        })}
      </p>
    {/if}
  </div>
  {#if pollingClamp.active}
    <p class="clamp-hint" role="status">
      {t('settings.clampHint', { max: pollingClamp.effMax })}
    </p>
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
</style>
