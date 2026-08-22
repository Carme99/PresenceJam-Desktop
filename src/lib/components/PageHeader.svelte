<script lang="ts">
  import Logo from './Logo.svelte';
  import { theme, toggleTheme } from '$lib/stores/theme';
  import { t } from '$lib/i18n';

  /**
   * Standard top-of-page chrome used by Settings, Reconnect, LogViewer.
   * Provides a back button (left), an optional wordmark + title (centre),
   * and an optional theme-toggle icon button (right).
   *
   * On About: pass `showWordmark` to render Logo+title together,
   * otherwise use the centred `<h1>` form.
   *
   * C7: `backLabel` renames the left button (e.g. "Pop back in" when the
   * view is rendered in a detached window) and `onAction`/`actionLabel`
   * render one extra right-aligned button (e.g. "Pop out").
   */
  interface Props {
    title: string;
    onBack: () => void;
    backLabel?: string;
    showThemeToggle?: boolean;
    showLogo?: boolean;
    actionLabel?: string;
    actionTitle?: string;
    onAction?: () => void;
  }
  let {
    title,
    onBack,
    backLabel = 'Back',
    showThemeToggle = true,
    showLogo = true,
    actionLabel = '⧉',
    actionTitle = '',
    onAction
  }: Props = $props();
</script>

<header class="page-header">
  <button type="button" class="back-btn btn-secondary" onclick={onBack}>
    <span aria-hidden="true">←</span>
    <span>{backLabel}</span>
  </button>
  <div class="title-block">
    {#if showLogo}
      <Logo size={28} />
    {/if}
    <h1>{title}</h1>
  </div>
  {#if onAction}
    <button type="button" class="icon-btn" onclick={onAction}
      aria-label={actionTitle || actionLabel} title={actionTitle}>
      {actionLabel}
    </button>
  {/if}
  {#if showThemeToggle}
    <button type="button" class="icon-btn theme-btn" onclick={toggleTheme}
      aria-label={t('common.themeToggle')} title={t('common.themeToggle')}>
      {$theme === 'dark' ? '☀' : '☾'}
    </button>
  {:else}
    <span class="theme-slot" aria-hidden="true"></span>
  {/if}
</header>

<style>
  .page-header {
    display: flex;
    align-items: center;
    gap: var(--sp-3);
  }
  .back-btn {
    width: auto;
    padding: var(--sp-2) var(--sp-4);
    font-size: var(--fs-sm);
    flex-shrink: 0;
    display: inline-flex;
    align-items: center;
    gap: var(--sp-2);
  }
  .title-block {
    display: flex;
    align-items: center;
    gap: var(--sp-3);
    flex: 1;
    min-width: 0;
  }
  h1 {
    font-size: var(--fs-2xl);
    font-weight: 700;
    letter-spacing: -0.02em;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .theme-btn { flex-shrink: 0; }
  .theme-slot { width: 36px; height: 36px; flex-shrink: 0; }
</style>
