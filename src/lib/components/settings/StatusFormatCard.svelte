<script lang="ts">
  import { t } from '$lib/i18n';
  import type { AppConfig } from '$lib/types';
  import SettingsCard from './SettingsCard.svelte';

  interface Props {
    /** Bindable slice: the card edits the status-format fields on `teams.*`. */
    teams: AppConfig['teams'];
    /** The lexicon textarea draft (owned by the parent: save adopts it). */
    extraWordsText: string;
    /** Display value for the placeholder input (localized default when unset). */
    placeholderDisplay: string;
    /** Live preview text rendered from `preview_status` by the parent. */
    previewText: string;
    /** Whether the preview runs through a profane sample (issue #342). */
    previewProfaneSample: boolean;
    /** Reset callback: restore shipped defaults (incl. the lexicon). */
    onreset: () => void;
    /** Lexicon textarea input: the parent owns the draft buffer. */
    onExtraWordsInput: (value: string) => void;
    /** Profane-sample toggle: the parent owns the preview flag. */
    onProfaneSampleChange: (value: boolean) => void;
  }

  let {
    teams = $bindable(),
    extraWordsText,
    placeholderDisplay,
    previewText,
    previewProfaneSample,
    onreset,
    onExtraWordsInput,
    onProfaneSampleChange
  }: Props = $props();

  const EXTRA_WORDS_MAX_ENTRIES = 64;
  const EXTRA_WORDS_MAX_CHARS = 32;

  /**
   * Rust bounds the lexicon to 64 entries of 32 chars at the IPC boundary
   * (issue #538). Counted here so the hint can say what will actually be
   * matched. The parent applies the clamp on save.
   */
  let extraWordsClamp = $derived.by(() => {
    const raw = extraWordsText
      .split('\n')
      .map((line) => line.trim())
      .filter((line) => line.length > 0);
    const kept = raw.map((word) => [...word].slice(0, EXTRA_WORDS_MAX_CHARS).join(''));
    const truncated = kept.slice(0, EXTRA_WORDS_MAX_ENTRIES);
    return {
      active: raw.length > EXTRA_WORDS_MAX_ENTRIES || kept.some((word, i) => word !== raw[i]),
      kept: truncated.length,
      maxEntries: EXTRA_WORDS_MAX_ENTRIES,
      maxChars: EXTRA_WORDS_MAX_CHARS,
      clamped: truncated
    };
  });
</script>

<SettingsCard title={t('settings.sectionStatusFormat')} resetLabel={t('common.resetToDefault')} {onreset}>
  <div class="form-group">
    <label for="status-format">{t('settings.formatTemplate')}</label>
    <input
      id="status-format"
      type="text"
      bind:value={teams.status_format}
      placeholder={t('settings.formatTemplatePlaceholder')}
    />
  </div>
  <div class="form-group">
    <!-- #748: the sample is a reading-order element, not a live region.
         Announcing it re-read the whole sample after every typing pause,
         layered on top of the field's own echo, which made the template
         unusable with a screen reader. -->
    <span class="form-label">{t('settings.livePreview')}</span>
    <div class="preview-box">{previewText}</div>
  </div>
  <p class="hint">
    {t('settings.placeholdersHint')}
  </p>
  <!-- Issue #581: episodes use their own template, so a user editing the
       music template must know it does not apply to podcasts. -->
  <p class="hint">{t('settings.episodeFormatHint')}</p>
  <div class="toggle-row">
    <label for="profanity-filter">{t('settings.profanityFilterLabel')}</label>
    <input
      id="profanity-filter"
      type="checkbox"
      bind:checked={teams.profanity_filter}
    />
  </div>
  {#if teams.profanity_filter}
    <div class="form-group">
      <label for="profanity-placeholder">{t('settings.placeholderTextLabel')}</label>
      <p class="hint">
        {t('settings.placeholderTextHint')}
      </p>
      <input
        id="profanity-placeholder"
        type="text"
        value={placeholderDisplay}
        oninput={(e) => {
          teams.profanity_placeholder = (e.currentTarget as HTMLInputElement).value;
        }}
        placeholder={t('settings.placeholderTextPlaceholder')}
      />
    </div>
    <div class="toggle-row">
      <label for="profanity-preview-sample">{t('settings.profaneSampleToggle')}</label>
      <input
        id="profanity-preview-sample"
        data-no-draft
        type="checkbox"
        checked={previewProfaneSample}
        onchange={(e) => onProfaneSampleChange((e.currentTarget as HTMLInputElement).checked)}
      />
    </div>
    <!-- Issue #538: `teams.profanity_extra_words` was config-only until
         4.6. The counter mirrors Rust's `clamp_teams` (64 entries × 32
         chars) so the truncation is never silent. -->
    <div class="form-group">
      <label for="profanity-extra-words">{t('settings.extraWordsLabel')}</label>
      <p class="hint">{t('settings.extraWordsHint')}</p>
      <textarea
        id="profanity-extra-words"
        rows="3"
        value={extraWordsText}
        oninput={(e) => onExtraWordsInput((e.currentTarget as HTMLTextAreaElement).value)}
        placeholder={t('settings.extraWordsPlaceholder')}
      ></textarea>
      {#if extraWordsClamp.active}
        <p class="clamp-hint" role="status">
          {t('settings.extraWordsClampHint', {
            max: extraWordsClamp.maxEntries,
            chars: extraWordsClamp.maxChars,
            kept: extraWordsClamp.kept
          })}
        </p>
      {/if}
    </div>
  {/if}
</SettingsCard>

<style>
  .form-group {
    display: flex;
    flex-direction: column;
    gap: var(--sp-2);
  }
  .form-group label,
  .form-group .form-label {
    font-size: var(--fs-sm);
    font-weight: 600;
    color: var(--fg);
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
  .preview-box {
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    padding: var(--sp-3) var(--sp-4);
    font-size: var(--fs-base);
    color: var(--fg);
    word-break: break-word;
    min-height: 40px;
  }
  /* #750 slice 2: `.clamp-hint` lives in app.css now — this copy deleted. */
</style>
