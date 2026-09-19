<script lang="ts">
  /**
   * #952: the Teams device-code step, shared by Onboarding, Settings and
   * Reconnect. Before this, each pane rendered the step its own way —
   * Reconnect printed the code as bare `<strong>` body text with none of the
   * affordances (monospace, select-all, boxed boundary) that make a code
   * meant for transcription safe to copy — and each pane carried its own
   * drifted copy of the CSS.
   *
   * Ownership: the device code is the one thing the user has to carry into a
   * browser, so this component owns the box, the accent verification-URL pill,
   * the monospace select-all code pill, the expiry countdown, the busy state
   * and both in-box actions (`common.checkNow`, `common.getNewCode`), leaving
   * callers to supply only state and callbacks.
   */
  import { t } from '$lib/i18n';
  import { formatCountdownMs, isSafeHttpUrl } from '$lib/stores/authFlow.svelte';

  let {
    /** The device code the user types into the browser. */
    userCode,
    /** `verification_uri` from the device-code response. */
    verificationUrl,
    /** Milliseconds until the code expires; `null` hides the countdown. */
    remainingMs = null,
    /** `true` once the code is dead — offers a fresh one (#429). */
    expired = false,
    /** `true` while a poll is in flight (shared mutex, #396). */
    busy = false,
    /** "Check now" — caller runs the shared poll. */
    onCheckNow,
    /** "Get new code" — caller restarts the device-code flow. */
    onNewCode
  }: {
    userCode: string;
    verificationUrl: string;
    remainingMs?: number | null;
    expired?: boolean;
    busy?: boolean;
    onCheckNow: () => void;
    onNewCode: () => void;
  } = $props();
</script>

<div class="device-code-box">
  <p class="hint">{t('common.openSignInPage')}</p>
  {#if isSafeHttpUrl(verificationUrl)}
    <a class="verification-url" href={verificationUrl} target="_blank" rel="noopener">{verificationUrl}</a>
  {:else}
    <span class="verification-url">{verificationUrl}</span>
  {/if}
  <p class="hint">{t('common.enterCodeWhenAsked')}</p>
  <div class="code-display" aria-live="polite">{userCode}</div>
  {#if expired}
    <p class="error-message" role="alert">{t('common.codeExpired')}</p>
    <button class="btn-secondary" onclick={onNewCode}>{t('common.getNewCode')}</button>
  {:else}
    {#if remainingMs != null}
      <!-- #735: the visible countdown keeps ticking, but it is NOT a live
           region. It is recomputed once per second for the whole ~15-minute
           lifetime of the code, and announcing each tick buried the code
           arrival (the polite `aria-live` on `.code-display` above) and the
           expiry alert (`role="alert"` below) under a queue that never
           drained. -->
      <p class="hint">{t('common.codeExpiresIn', { time: formatCountdownMs(remainingMs) })}</p>
    {/if}
    <div class="spinner" aria-hidden="true"></div>
    <p>{t('common.waitingForSignIn')}</p>
    <button class="btn-secondary" onclick={onCheckNow} disabled={busy}>{t('common.checkNow')}</button>
  {/if}
</div>

<style>
  .device-code-box {
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    padding: var(--sp-5);
    /* Spans the row it is dropped into (Settings' `.connection-row` is a flex
       row that used to have a scoped `.device-code-box { width: 100% }`; no
       scoped rule can reach another component's element). Harmless in the
       wizard and Reconnect, whose parents already stretch their children. */
    width: 100%;
    text-align: center;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--sp-3);
  }
  /* #751: `.hint` and `.error-message` are global now. The box keeps only the
     one place where its own rhythm differs from a prose pane: the hints are
     `--fs-sm` and carry no paragraph margins, because the box already spaces
     them with `gap`. */
  .device-code-box .hint {
    margin: 0;
    font-size: var(--fs-sm);
  }
  .verification-url {
    display: inline-block;
    padding: var(--sp-2) var(--sp-4);
    background: var(--accent-soft);
    color: var(--accent);
    border-radius: var(--r-md);
    font-weight: 600;
    word-break: break-all;
    text-decoration: none;
    font-family: var(--font-mono);
    font-size: var(--fs-sm);
  }
  .verification-url:hover { background: var(--bg-base); }

  /* #751: one size for the transcription target, in every pane that shows
     it — 26px rather than the wizard's 32px so it also fits Settings' pane. */
  .code-display {
    font-family: var(--font-mono);
    font-size: var(--fs-2xl);
    font-weight: 700;
    letter-spacing: 0.2em;
    color: var(--fg);
    background: var(--bg-base);
    border: 2px dashed var(--border-strong);
    border-radius: var(--r-md);
    padding: var(--sp-4);
    user-select: all;
    font-variant-numeric: tabular-nums;
  }


  /* #952: the same action looks the same everywhere — the in-box actions are
     full-width secondary buttons, never `.btn-full`. */
  .device-code-box .btn-secondary { width: 100%; }
</style>
