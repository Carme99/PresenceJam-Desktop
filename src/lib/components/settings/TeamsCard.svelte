<script lang="ts">
  import { t } from '$lib/i18n';
  import { authFlow, teamsPollMutex } from '$lib/stores/authFlow.svelte';
  import { presence, clearAuthPersistWarning } from '$lib/stores/presence';
  import { pickReconnectProvider } from '$lib/utils/routeReconnect';
  import DeviceCodeBox from '../DeviceCodeBox.svelte';
  import SettingsCard from './SettingsCard.svelte';

  interface Props {
    /** Live connection flag (adopted from `get_sync_status` by the parent). */
    teamsConnected: boolean;
    /** Whether the flow is waiting on the device code (`authFlow.teams.phase`). */
    waiting: boolean;
    /** Device-code expiry countdown, in ms (`null` = unknown expiry). */
    remainingMs: number | null;
    /** Whether the device code has expired. */
    codeExpired: boolean;
    /** Presence scopes missing on the stored token (issue #3.0-P1/P2). */
    scopesMissing: boolean;
    /** Reconnect entry point owned by the parent (guards + detached). */
    onReconnect: () => void;
    /** Spotify reconnect: the #932 banner routes by provider discriminator. */
    onReconnectSpotify: () => void;
    /** Post-success poll owned by the parent (marks connected on success). */
    onCheckNow: () => void;
  }

  let {
    teamsConnected,
    waiting,
    remainingMs,
    codeExpired,
    scopesMissing,
    onReconnect,
    onReconnectSpotify,
    onCheckNow
  }: Props = $props();

  /**
   * #932 (rework): the auth-persist banner's reconnect action, label and
   * "in flight" disabled state all route off the `provider` discriminator.
   * An unknown value falls back to the Spotify reconnect (the Spotify banner
   * is the newer of the two, #932 B1) so a future backend payload cannot
   * crash the banner. The pure decision lives in `routeReconnect.ts` so a
   * Vitest spec can exercise it without the component harness.
   */
  function reconnectFor(provider: 'teams' | 'spotify' | string): {
    label: string;
    handler: () => void;
  } {
    const target = pickReconnectProvider(provider);
    if (target === 'teams') {
      return { label: t('settings.sectionTeams'), handler: onReconnect };
    }
    return { label: t('settings.sectionSpotify'), handler: onReconnectSpotify };
  }
</script>

<SettingsCard title={t('settings.sectionTeams')}>
  <span class="badge" class:success={teamsConnected && !waiting}
        class:warning={waiting}
        class:error={!teamsConnected && !waiting}>
    <span class="dot"></span>
    {#if waiting}{t('common.reconnecting')}{:else if teamsConnected}{t('common.connected')}{:else}{t('common.notConnected')}{/if}
  </span>
  <p class="hint">{t('settings.teamsAuthHint')}</p>
  <div class="connection-row">
    {#if teamsConnected && !waiting}
      <button class="btn-secondary" onclick={onReconnect} disabled={waiting}>{t('reconnect.reconnectTeams')}</button>
    {:else if waiting}
      <!-- #952: the same device-code block Onboarding and Reconnect render
           — accent URL pill, monospace select-all code, one countdown.
           #735: that countdown is deliberately not a live region; the code
           arrives under `aria-live` and the expiry under `role="alert"`. -->
      <DeviceCodeBox
        userCode={authFlow.teams.userCode}
        verificationUrl={authFlow.teams.verificationUrl}
        remainingMs={remainingMs}
        expired={codeExpired}
        busy={teamsPollMutex.inFlight}
        onCheckNow={onCheckNow}
        onNewCode={onReconnect}
      />
    {:else}
      <button class="btn-secondary" onclick={onReconnect}>{t('reconnect.reconnectTeams')}</button>
    {/if}
  </div>
  <!-- #816: the failure belongs to the card, not to the waiting branch.
       `setTeamsPhase('error', …)` is what clears the waiting flag, so a
       block nested inside that branch unmounted the moment the error
       arrived and the card fell back to a green Connected badge with the
       same button and no reason shown. `reconnectTeams` calls
       `resetTeamsAuthFlow()` on entry, so the next attempt clears it. -->
  {#if authFlow.teams.error}
    <p class="error-message" role="alert">{authFlow.teams.error}</p>
  {/if}
  {#if scopesMissing}
    <div class="scope-banner">
      <span class="hint">{t('settings.presenceScopeBanner')}</span>
      <button type="button" class="btn-link" onclick={onReconnect} disabled={waiting}>{t('common.reconnect')}</button>
    </div>
  {/if}
  {#if $presence.authPersistWarning}
    <!-- Issue #932: the banner is now provider-aware. Both Teams
         (#562) and Spotify (#932) sign-in flows feed this banner with
         a `provider` discriminator; the copy and the reconnect action
         follow. #693: dismissible as well as retryable — the cause can be one
         the user cannot fix in-session (a permanently locked keychain,
         a read-only disk), and a banner that only clears after a
         *successful* reconnect would be undismissable there. -->
    {@const warning = $presence.authPersistWarning}
    {@const reconnect = reconnectFor(warning.provider)}
    <div class="persist-banner" role="alert">
      <span class="hint">{t('settings.authPersistWarning', { provider: reconnect.label })}</span>
      <button
        type="button"
        class="btn-link"
        onclick={reconnect.handler}
        disabled={waiting}
      >{t('common.reconnect')}</button>
      <button type="button" class="btn-link dismiss" onclick={clearAuthPersistWarning}>{t('common.dismiss')}</button>
    </div>
  {/if}
</SettingsCard>

<style>
  .connection-row {
    display: flex;
    align-items: center;
    gap: var(--sp-3);
    flex-wrap: wrap;
  }
  .connection-row .btn-secondary {
    width: auto;
    padding: var(--sp-2) var(--sp-4);
    font-size: var(--fs-sm);
  }
  /* One-time-reconnect banner for the missing presence scopes
     (issue #3.0-P1/P2). */
  .scope-banner {
    margin-top: var(--sp-3);
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    padding: var(--sp-2) var(--sp-3);
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: var(--r-md);
  }
  .scope-banner .hint { margin: 0; }
  /* #693: the Teams session could not be persisted (locked keychain, full
     disk). Amber, like the dirty banner: the sign-in itself succeeded, so
     this is a warning the user can still act on by reconnecting. */
  .persist-banner {
    margin-top: var(--sp-3);
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    padding: var(--sp-2) var(--sp-3);
    background: var(--warning-soft);
    color: var(--warning);
    border-radius: var(--r-md);
  }
  .persist-banner .hint { margin: 0; color: inherit; }
</style>
