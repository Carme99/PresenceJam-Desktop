<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { t, i18n, type Locale } from '$lib/i18n';
  import type { AppConfig } from '$lib/types';
  import { theme, density } from '$lib/stores/theme';
  import { updateConfig } from '$lib/stores/config';
  import { shortcutReasonLabel, normalizeShortcutReason } from '$lib/utils/shortcuts';
  import SettingsCard from './SettingsCard.svelte';

  interface Props {
    /** Bindable slice: the locale lives on the draft (kept in step on change). */
    locale: AppConfig['locale'];
    /** Bindable slice: the autostart flag (converged through the command). */
    autostart: AppConfig['autostart'];
    /** `data-no-draft` save message channel the parent footer also reads. */
    saveMessage: string;
    /** #984: follow-system mirror (a resolution policy, not a language). */
    followSystemChecked: boolean;
    /** Reset callback: restore shipped defaults (system/comfortable/en). */
    onreset: () => void;
    /** Parent's auto-clear for the self-applying autostart failure line. */
    onAutostartError: (message: string) => void;
  }

  let {
    locale = $bindable(),
    autostart = $bindable(),
    saveMessage = $bindable(),
    followSystemChecked = $bindable(),
    onreset,
    onAutostartError
  }: Props = $props();

  // #552: a radiogroup must own `role="radio"`/`aria-checked` children with a
  // roving tabindex and arrow-key navigation.
  let themeDarkButton: HTMLButtonElement | undefined = $state();
  let themeLightButton: HTMLButtonElement | undefined = $state();
  let themeSystemButton: HTMLButtonElement | undefined = $state();

  // #680: `system` joins the radiogroup, so the arrow-key walk has to cycle
  // through three cards instead of toggling two.
  const THEME_OPTIONS = ['dark', 'light', 'system'] as const;
  type ThemeOption = (typeof THEME_OPTIONS)[number];

  function themeRadioKeydown(e: KeyboardEvent, current: ThemeOption) {
    const isNext = e.key === 'ArrowRight' || e.key === 'ArrowDown';
    const isPrev = e.key === 'ArrowLeft' || e.key === 'ArrowUp';
    if (!isNext && !isPrev) return;
    e.preventDefault();
    const step = isNext ? 1 : THEME_OPTIONS.length - 1;
    const next = THEME_OPTIONS[(THEME_OPTIONS.indexOf(current) + step) % THEME_OPTIONS.length];
    theme.set(next);
    // Selection follows focus, and the roving tabindex moves with it.
    const buttons: Record<ThemeOption, HTMLButtonElement | undefined> = {
      dark: themeDarkButton,
      light: themeLightButton,
      system: themeSystemButton
    };
    buttons[next]?.focus();
  }

  async function toggleAutostart(e: Event) {
    const target = e.currentTarget as HTMLInputElement;
    const enabled = target.checked;
    const previous = !enabled;
    autostart = enabled;
    try {
      await invoke('set_autostart_enabled', { enabled });
      // Issue #811: the command now owns the `config.autostart` flag
      // too, so converge the store immediately. `updateConfig` merges just
      // this field backend-side and adopts the persisted document. The
      // input carries `data-no-draft` (like the notification toggles): the
      // toggle applies itself, so it must not mark the form dirty.
      const converged = await updateConfig({ autostart: enabled });
      autostart = converged.autostart;
    } catch (err) {
      console.warn('[SETTINGS] set_autostart_enabled failed:', err);
      autostart = previous;
      target.checked = previous;
      saveMessage = t('settings.shortcutRejected', {
        reason: shortcutReasonLabel(normalizeShortcutReason(err) ?? { kind: 'Unknown', message: String(err).slice(0, 120) })
      });
      // #750 slice 2: the parent owns the 3000ms auto-clear (main's
      // `saveTimeout`), so a stuck failure line cannot outlive its window.
      onAutostartError(saveMessage);
    }
  }
</script>

<SettingsCard title={t('settings.sectionAppearance')} resetLabel={t('common.resetToDefault')} {onreset}>
  <div class="form-group">
    <span class="form-label">{t('settings.themeLabel')}</span>
    <div class="theme-grid" role="radiogroup" aria-label={t('settings.themeLabel')}>
      <button type="button" class="theme-card" role="radio" bind:this={themeDarkButton}
        aria-checked={$theme === 'dark'} tabindex={$theme === 'dark' ? 0 : -1}
        class:is-active={$theme === 'dark'}
        data-no-draft
        onclick={() => theme.set('dark')}
        onkeydown={(e) => themeRadioKeydown(e, 'dark')}>
        <span class="swatch swatch-dark"></span>
        <span class="theme-name">{t('settings.themeDark')}</span>
      </button>
      <button type="button" class="theme-card" role="radio" bind:this={themeLightButton}
        aria-checked={$theme === 'light'} tabindex={$theme === 'light' ? 0 : -1}
        class:is-active={$theme === 'light'}
        data-no-draft
        onclick={() => theme.set('light')}
        onkeydown={(e) => themeRadioKeydown(e, 'light')}>
        <span class="swatch swatch-light"></span>
        <span class="theme-name">{t('settings.themeLight')}</span>
      </button>
      <button type="button" class="theme-card" role="radio" bind:this={themeSystemButton}
        aria-checked={$theme === 'system'} tabindex={$theme === 'system' ? 0 : -1}
        class:is-active={$theme === 'system'}
        data-no-draft
        onclick={() => theme.set('system')}
        onkeydown={(e) => themeRadioKeydown(e, 'system')}>
        <span class="swatch swatch-system"></span>
        <span class="theme-name">{t('settings.themeSystem')}</span>
      </button>
    </div>
    <p class="hint">{t('settings.themeHint')}</p>
  </div>
  <!-- #680: spacing/type density. Token-scale override only (app.css
    `[data-density="compact"]`), independent of the theme picker. -->
  <div class="toggle-row">
    <label for="compact-density">{t('settings.densityCompactLabel')}</label>
    <input
      id="compact-density"
      data-no-draft
      type="checkbox"
      checked={$density === 'compact'}
      onchange={(e) =>
        density.set((e.currentTarget as HTMLInputElement).checked ? 'compact' : 'comfortable')}
    />
  </div>
  <p class="hint">{t('settings.densityHint')}</p>
  <div class="form-group">
    <label for="language">{t('settings.languageLabel')}</label>
    <!-- Language names are endonyms: shown in their own language by convention. -->
    <select
      id="language"
      data-no-draft
      value={i18n.locale}
      disabled={followSystemChecked}
      onchange={(e) => {
        const next = (e.currentTarget as HTMLSelectElement).value as Locale;
        // 4.7.0 (issue #674): `config.locale` is the single source of
        // truth. The store applies the locale to this webview, persists
        // it and relabels the tray + native application menu; the draft is
        // kept in step so a language change alone never marks the form
        // dirty.
        locale = next;
        void i18n.set(next);
      }}
    >
      <option value="en">English</option>
      <option value="de">Deutsch</option>
      <option value="fr">Français</option>
      <option value="es">Español</option>
      <option value="it">Italiano</option>
      <option value="pl">Polski</option>
      <option value="pt">Português (BR)</option>
      <option value="nl">Nederlands</option>
    </select>
    <p class="hint">{t('settings.languageHint')}</p>
  </div>
  <div class="toggle-row">
    <label for="follow-system-language">{t('settings.languageFollowSystemLabel')}</label>
    <input
      id="follow-system-language"
      data-no-draft
      type="checkbox"
      checked={followSystemChecked}
      onchange={(e) => {
        // #984: follow-system is a resolution policy, not a language — it
        // lives in the i18n store's own mirror, never in `config.locale`.
        // Checking re-resolves from the OS language and persists the
        // resolution; unchecking pins the current resolution as the
        // explicit choice, so the draft's tag is exactly what the user
        // keeps. Either way the language select stays in step.
        const on = (e.currentTarget as HTMLInputElement).checked;
        if (on) {
          void i18n.followSystemLanguage().then(() => {
            locale = i18n.locale;
            followSystemChecked = i18n.followSystem;
          });
        } else {
          const pinned = i18n.locale;
          locale = pinned;
          void i18n.set(pinned).then(() => {
            followSystemChecked = i18n.followSystem;
          });
        }
      }}
    />
  </div>
  <p class="hint">{t('settings.languageFollowSystemHint')}</p>
  <div class="toggle-row">
    <label for="autostart">{t('common.launchAtLogin')}</label>
    <input
      id="autostart"
      type="checkbox"
      data-no-draft
      checked={autostart}
      onchange={toggleAutostart}
    />
  </div>
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
  .theme-grid {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: var(--sp-3);
  }
  .theme-card {
    display: flex;
    flex-direction: column;
    align-items: stretch;
    gap: var(--sp-2);
    padding: var(--sp-3);
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    cursor: pointer;
    width: auto;
    transition: border-color var(--dur-fast) var(--ease-out),
                background-color var(--dur-fast) var(--ease-out);
  }
  .theme-card:hover {
    background: var(--bg-surface);
    border-color: var(--border-strong);
  }
  .theme-card.is-active {
    border-color: var(--accent);
    box-shadow: 0 0 0 3px var(--accent-soft);
  }
  /* #904: the swatches paint through app.css tokens, not hex literals. The
     light swatch is on screen while the dark theme is live, so these cannot
     be the theme-scoped surface tokens — `--preview-*` is the preview-only
     pair app.css declares for exactly this, and `--swatch-h` is what compact
     density scales. Changing the palette moves the swatch from here alone. */
  .swatch {
    display: block;
    height: var(--swatch-h);
    border-radius: var(--r-sm);
    border: 1px solid var(--border);
  }
  .swatch-dark { background: linear-gradient(135deg, var(--preview-dark-1) 0%, var(--preview-dark-2) 100%); }
  .swatch-light { background: linear-gradient(135deg, var(--preview-light-1) 0%, var(--preview-light-2) 100%); }
  .swatch-system { background: linear-gradient(100deg, var(--preview-dark-1) 0 48%, var(--preview-light-1) 48% 100%); }
  .theme-name {
    font-size: var(--fs-sm);
    font-weight: 600;
    color: var(--fg);
    text-align: left;
  }
</style>
