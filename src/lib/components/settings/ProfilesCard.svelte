<script lang="ts">
  import { t } from '$lib/i18n';
  import type { AppConfig } from '$lib/types';
  import SettingsCard from './SettingsCard.svelte';

  interface Props {
    /** Bindable slice: the card edits `presence_profiles` in place. */
    presenceProfiles: AppConfig['presence_profiles'];
    /** Bindable slice: the active profile id (`null` = base config). */
    activeProfile: AppConfig['active_profile'];
    /** The base `teams.*` values an unset overlay falls back to. */
    teams: AppConfig['teams'];
    /** Shared save-message channel (duplicate/missing-name feedback). */
    saveMessage: string;
    /** Dirty-mark callback: the parent's `markDirty` (button edits bypass oninput). */
    onchange: () => void;
  }

  let {
    presenceProfiles = $bindable(),
    activeProfile = $bindable(),
    teams,
    saveMessage = $bindable(),
    onchange
  }: Props = $props();

  // Issue #869: presence-profile bounds mirror `clamp_presence_profiles`.
  // The Rust side is the source of truth, so these constants exist only to
  // give the input its `maxlength` / `max` attribute.
  const MAX_PROFILE_ID_CHARS = 32;
  const MAX_PROFILE_IDLE_SECONDS = 86400;

  // Issue #869: name for a freshly-added profile. The Rust side dedupes
  // again on load, so a concurrent edit cannot wedge the form — this is
  // only the prefix the new-row picker suggests.
  function defaultProfileName(n: number): string {
    return `Profile ${n}`;
  }
</script>

<!-- Issue #869: presence-profile card. The Settings UI is the canonical
     place to author profiles; the tray / hotkey / CLI only flip the
     active id. Mirrors `clamp_presence_profiles`: names are deduped +
     trimmed to 32 chars and the active pointer clears on a missing id. -->
<SettingsCard title={t('profiles.sectionTitle')}>
  <p class="hint">{t('profiles.sectionHint')}</p>
  <div class="form-group">
    <label for="active-profile">{t('profiles.activeProfileLabel')}</label>
    <select
      id="active-profile"
      value={activeProfile ?? ''}
      onchange={(e) => {
        const value = (e.currentTarget as HTMLSelectElement).value;
        activeProfile = value === '' ? null : value;
      }}
    >
      <option value="">{t('profiles.activeProfileNone')}</option>
      {#each presenceProfiles as profile}
        <option value={profile.name}>{profile.name}</option>
      {/each}
    </select>
    <!-- Issue #869: the picker's options are derived from
         `presence_profiles`, so a name the user just deleted
         cannot appear; the spec's "unknown id clears to base" safety net
         is the Rust-side `clamp_presence_profiles`. -->
  </div>
  {#if presenceProfiles.length === 0}
    <p class="hint">{t('profiles.empty')}</p>
  {/if}
  {#each presenceProfiles as profile, i}
    <div class="rule-row rule-col" role="group" aria-label={`${t('profiles.sectionTitle')} ${i + 1}`}>
      <div class="rule-row">
        <input
          type="text"
          value={profile.name}
          placeholder={t('profiles.profileNamePlaceholder')}
          aria-label={t('profiles.profileNameLabel')}
          oninput={(e) => {
            const next = (e.currentTarget as HTMLInputElement).value;
            const trimmed = next.slice(0, MAX_PROFILE_ID_CHARS);
            // Reject duplicates (case-sensitive, ignores self).
            const clash = presenceProfiles.some(
              (other, idx) => idx !== i && other.name === trimmed
            );
            if (clash) {
              saveMessage = t('profiles.profileNameDuplicate');
              return;
            }
            if (trimmed.length === 0) {
              saveMessage = t('profiles.profileNameMissing');
              return;
            }
            saveMessage = '';
            profile.name = trimmed;
          }}
        />
        <button
          type="button"
          class="btn-link"
          onclick={() => {
            presenceProfiles.splice(i, 1);
            // If the active profile was the deleted one, reset to base.
            if (activeProfile === profile.name) {
              activeProfile = null;
            }
            onchange();
          }}
        >{t('profiles.removeProfile')}</button>
      </div>
      <div class="form-group">
        <label for={`profile-status-${i}`}>{t('profiles.overlayStatusFormatLabel')}</label>
        <input
          id={`profile-status-${i}`}
          type="text"
          value={profile.status_format ?? ''}
          placeholder={teams.status_format}
          oninput={(e) => {
            const v = (e.currentTarget as HTMLInputElement).value;
            profile.status_format = v.length === 0 ? null : v;
          }}
        />
      </div>
      <div class="form-group">
        <label class="rule-check">
          <input
            type="checkbox"
            checked={profile.clear_on_pause ?? teams.clear_on_pause}
            onchange={(e) => {
              profile.clear_on_pause = (e.currentTarget as HTMLInputElement).checked;
            }}
          />
          <span>{t('profiles.overlayClearOnPauseLabel')}</span>
        </label>
      </div>
      <div class="form-group">
        <label class="rule-check">
          <input
            type="checkbox"
            checked={profile.availability_sync ?? teams.availability_sync}
            onchange={(e) => {
              profile.availability_sync = (e.currentTarget as HTMLInputElement).checked;
            }}
          />
          <span>{t('profiles.overlayAvailabilitySyncLabel')}</span>
        </label>
      </div>
      <div class="form-group">
        <label class="rule-check">
          <input
            type="checkbox"
            checked={profile.gate_when_out_of_office ?? teams.gate_when_out_of_office}
            onchange={(e) => {
              profile.gate_when_out_of_office = (e.currentTarget as HTMLInputElement).checked;
            }}
          />
          <span>{t('profiles.overlayGateOutOfOfficeLabel')}</span>
        </label>
      </div>
      <div class="form-group">
        <label class="rule-check">
          <input
            type="checkbox"
            checked={profile.gate_when_presenting ?? teams.gate_when_presenting}
            onchange={(e) => {
              profile.gate_when_presenting = (e.currentTarget as HTMLInputElement).checked;
            }}
          />
          <span>{t('profiles.overlayGatePresentingLabel')}</span>
        </label>
      </div>
      <div class="form-group">
        <label for={`profile-idle-${i}`}>{t('profiles.overlayIdleAwayLabel')}</label>
        <input
          id={`profile-idle-${i}`}
          type="number"
          min="0"
          max={MAX_PROFILE_IDLE_SECONDS}
          value={profile.idle_away_after_seconds === null || profile.idle_away_after_seconds === undefined
            ? ''
            : Number(profile.idle_away_after_seconds)}
          placeholder={String(Number(teams.idle_away_after_seconds))}
          oninput={(e) => {
            const raw = (e.currentTarget as HTMLInputElement).value;
            if (raw === '') {
              profile.idle_away_after_seconds = null;
            } else {
              const n = Math.min(MAX_PROFILE_IDLE_SECONDS, Math.max(0, Number(raw)));
              profile.idle_away_after_seconds = n;
            }
          }}
        />
      </div>
    </div>
  {/each}
  <button
    type="button"
    class="btn-secondary"
    onclick={() => {
      // Generate a unique default name like "Profile 1", "Profile 2", ...
      // by finding the lowest positive integer suffix that does not
      // collide with an existing name. The Rust side will dedupe again
      // on load, so a concurrent edit cannot wedge the form.
      let n = 1;
      while (presenceProfiles.some((p) => p.name === defaultProfileName(n))) {
        n += 1;
      }
      presenceProfiles.push({ name: defaultProfileName(n) });
      onchange();
    }}
  >{t('profiles.addProfile')}</button>
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
  /* Issue #432: status-rule rows reuse the card's form rhythm — a
  wrapping flex row for quiet-hours entries, column variant for the
  four-field track rules. */
  .rule-row {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--sp-2);
  }
  .rule-row input[type='text'] {
    flex: 1 1 120px;
    min-width: 0;
  }
  .rule-col {
    flex-direction: column;
    align-items: stretch;
  }
  .rule-check {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    font-size: var(--fs-sm);
    color: var(--fg);
  }
</style>
