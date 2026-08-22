<script lang="ts">
  /**
   * PresenceJam inline logomark.
   * One SVG, scaled with `size`, used in Dashboard header, About,
   * Onboarding chrome. Mirrors the packaged app icon for continuity.
   */
  interface Props {
    size?: number;
    /** Optional wordmark beside the mark — used by the README render path. */
    withWordmark?: boolean;
    /** Decorative-only when `withWordmark` or surrounded by a label. */
    title?: string | null;
  }
  let { size = 28, withWordmark = false, title = 'PresenceJam' }: Props = $props();
  import { t } from '$lib/i18n';
</script>

{#if withWordmark}
  <span class="logo-lockup" style="--logo-size: {size}px" aria-label={title}>
    <svg viewBox="0 0 64 64" width={size} height={size} role="img" aria-hidden="true">
      <defs>
        <linearGradient id="lp-pill" x1="0" y1="0" x2="1" y2="1">
          <stop offset="0%" stop-color="#8E78FF" />
          <stop offset="100%" stop-color="#6A4DFF" />
        </linearGradient>
        <linearGradient id="lp-wave" x1="0" y1="1" x2="0" y2="0">
          <stop offset="0%" stop-color="#1ED760" />
          <stop offset="100%" stop-color="#1DB954" />
        </linearGradient>
      </defs>
      <rect x="8" y="22" width="48" height="20" rx="10" ry="10" fill="url(#lp-pill)" />
      <circle cx="15" cy="32" r="3" fill="#FFFFFF" />
      <rect x="23" y="36" width="4" height="6" rx="2" fill="url(#lp-wave)" />
      <rect x="31" y="29" width="4" height="13" rx="2" fill="url(#lp-wave)" />
      <rect x="39" y="23" width="4" height="19" rx="2" fill="url(#lp-wave)" />
      <rect x="47" y="30" width="4" height="12" rx="2" fill="url(#lp-wave)" />
    </svg>
    <span class="logo-wordmark">
      <span class="logo-name">Presence<strong>Jam</strong></span>
      <span class="logo-tag">{t('common.tagline')}</span>
    </span>
  </span>
{:else}
  <svg
    class="logo-mark"
    viewBox="0 0 64 64"
    width={size}
    height={size}
    role={title ? 'img' : 'presentation'}
    aria-label={title ?? undefined}
  >
    {#if title}<title>{title}</title>{/if}
    <defs>
      <linearGradient id="lm-pill" x1="0" y1="0" x2="1" y2="1">
        <stop offset="0%" stop-color="#8E78FF" />
        <stop offset="100%" stop-color="#6A4DFF" />
      </linearGradient>
      <linearGradient id="lm-wave" x1="0" y1="1" x2="0" y2="0">
        <stop offset="0%" stop-color="#1ED760" />
        <stop offset="100%" stop-color="#1DB954" />
      </linearGradient>
    </defs>
    <rect width="64" height="64" rx="14" ry="14" fill="#0F1226" />
    <rect x="8" y="22" width="48" height="20" rx="10" ry="10" fill="url(#lm-pill)" />
    <circle cx="15" cy="32" r="3" fill="#FFFFFF" />
    <rect x="23" y="36" width="4" height="6" rx="2" fill="url(#lm-wave)" />
    <rect x="31" y="29" width="4" height="13" rx="2" fill="url(#lm-wave)" />
    <rect x="39" y="23" width="4" height="19" rx="2" fill="url(#lm-wave)" />
    <rect x="47" y="30" width="4" height="12" rx="2" fill="url(#lm-wave)" />
  </svg>
{/if}

<style>
  .logo-mark {
    display: inline-block;
    flex-shrink: 0;
    border-radius: var(--r-md);
  }
  .logo-lockup {
    display: inline-flex;
    align-items: center;
    gap: var(--sp-3);
  }
  .logo-wordmark {
    display: inline-flex;
    flex-direction: column;
    line-height: 1.1;
  }
  .logo-name {
    font-size: calc(var(--logo-size) * 0.62);
    font-weight: 700;
    letter-spacing: -0.015em;
    color: var(--fg);
  }
  .logo-name strong {
    color: var(--accent);
    font-weight: 700;
  }
  .logo-tag {
    margin-top: 3px;
    font-size: calc(var(--logo-size) * 0.26);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.16em;
    color: var(--fg-subtle);
  }
</style>
