<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { t } from '$lib/i18n';
  import type { AppConfig } from '$lib/types';
  import { clientSecretStateOf, type ClientSecretState } from '$lib/stores/config';
  import { authFlow, setSpotifyPhase, resetSpotifyAuthFlow } from '$lib/stores/authFlow.svelte';
  import SettingsCard from './SettingsCard.svelte';

  interface Props {
    /** Bindable slice: the card edits `spotify.client_id` in place. */
    spotify: AppConfig['spotify'];
    /** Live connection flag (adopted from `get_sync_status` by the parent). */
    isConnected: boolean;
    /** Whether the flow is waiting on the browser (`authFlow.spotify.phase`). */
    waiting: boolean;
    /** The tray-playback scope banner (`user-modify-playback-state`). */
    playbackScopeMissing: boolean;
    /** Legacy-plaintext secret conflict (issues #376/#813). */
    secretConflict: boolean;
    /** Detached pane: reconnect navigates home instead of invoking. */
    detached: boolean;
    /** Reconnect entry points owned by the parent (client_id + guards). */
    onReconnect: () => void;
    onGoToOnboarding: () => void;
    onRestartSignIn: () => void;
    onForwardToMain: () => void;
  }

  let {
    spotify = $bindable(),
    isConnected,
    waiting,
    playbackScopeMissing,
    secretConflict,
    detached,
    onReconnect,
    onGoToOnboarding,
    onRestartSignIn,
    onForwardToMain
  }: Props = $props();

  // #560: the OS keychain's answer about the stored client_secret —
  // `present` / `absent` / `unavailable`. The credential row must branch on
  // this rather than on `client_secret_set`, which cannot tell "the user never
  // configured a secret" from "the keychain would not answer".
  let secretState: ClientSecretState = $derived(clientSecretStateOf({ spotify } as AppConfig));

  // ── #964: the waiting-state escape hatch ──────────────────────────────
  //
  // The poller's `spotify-reconnect-required` event lands the user on this
  // pane with the flow already waiting, so this card is where a lost browser
  // tab strands them.
  let manualUrl = $state('');
  let manualSubmitBusy = $state(false);
  let manualUrlError = $state('');

  /** Extract `code`/`state` from a pasted Spotify redirect URL. */
  function extractCodeFromUrl(url: string): { code: string; state: string } | null {
    try {
      const parsed = new URL(url);
      const code = parsed.searchParams.get('code');
      if (!code) return null;
      // A missing `state` still passes (empty string) — the backend rejects it,
      // mirroring the deep-link CSRF check (#162).
      return { code, state: parsed.searchParams.get('state') ?? '' };
    } catch {
      return null;
    }
  }

  /** Complete the flow from a pasted redirect URL (the #385 fallback). */
  async function submitManualUrl() {
    if (manualSubmitBusy) return;
    const extracted = extractCodeFromUrl(manualUrl);
    if (!extracted) {
      manualUrlError = t('validation.noCodeInUrl');
      return;
    }
    manualSubmitBusy = true;
    manualUrlError = '';
    try {
      await invoke('complete_spotify_auth_manual', {
        code: extracted.code,
        oauthState: extracted.state
      });
      setSpotifyPhase('done');
    } catch (e) {
      console.error('[SETTINGS] complete_spotify_auth_manual failed:', e);
      manualUrlError = String(e);
      setSpotifyPhase('error', String(e));
    } finally {
      manualSubmitBusy = false;
    }
  }

  function restartSignIn() {
    // `onReconnect` refuses a restart while the phase is `waiting`, so the
    // phase is cleared first.
    resetSpotifyAuthFlow();
    onRestartSignIn();
  }
</script>

<SettingsCard title={t('settings.sectionSpotify')}>
  <span class="badge" class:success={isConnected && !waiting}
        class:warning={waiting}
        class:error={!isConnected && !waiting}>
    <span class="dot"></span>
    {#if waiting}{t('common.reconnecting')}{:else if isConnected}{t('common.connected')}{:else}{t('common.notConnected')}{/if}
  </span>
  <div class="form-group">
    <label for="spotify-client-id">{t('settings.clientId')}</label>
    <input
      id="spotify-client-id"
      type="text"
      bind:value={spotify.client_id}
      readonly={isConnected}
      placeholder={t('settings.clientIdPlaceholder')}
    />
  </div>
  <div class="form-group">
    <span class="form-label">{t('settings.clientSecret')}</span>
    <p class="hint">
      <!-- #560: three states, not two. `client_secret_set` is the
           `Present`-only projection, so branching on it alone told a user
           whose keyring was locked that nothing was configured and
           pointed them at onboarding to re-enter a secret that is still
           stored. -->
      {#if secretState === 'present'}
        {t('settings.secretStoredHint')}
      {:else if secretState === 'unavailable'}
        {t('settings.secretKeychainUnavailable')}
      {:else}
        {t('settings.secretNotConfigured')} <button type="button" class="btn-link" onclick={onGoToOnboarding}>{t('settings.runOnboarding')}</button> {t('settings.toSetUpSpotify')}
      {/if}
    </p>
  </div>
  <div class="connection-row">
    {#if isConnected && !waiting}
      <button class="btn-secondary" onclick={onReconnect} disabled={waiting}>{t('settings.reconnectSpotify')}</button>
    {:else if waiting}
      <div class="spotify-waiting">
        <span class="hint">{t('settings.completeAuthInBrowser')}</span>
        <!-- #964: both escapes Reconnect offers for a stuck flow — the
             browser tab may be gone, or the sign-in may have finished
             after the `presencejam://` deep link was lost. -->
        <button type="button" class="btn-secondary" onclick={restartSignIn}>{t('reconnect.restartSignIn')}</button>
        <p class="hint" id="spotify-manual-url-hint">{t('onboarding.manualUrlHint')}</p>
        <input
          id="spotify-manual-url"
          data-no-draft
          type="text"
          bind:value={manualUrl}
          aria-label={t('onboarding.manualUrlLabel')}
          placeholder={t('onboarding.manualUrlPlaceholder')}
          aria-describedby="spotify-manual-url-hint"
          onkeydown={(e) => e.key === 'Enter' && submitManualUrl()}
        />
        <button type="button" class="btn-secondary" onclick={submitManualUrl} disabled={manualSubmitBusy}>
          {t('onboarding.submitCode')}
        </button>
        {#if manualUrlError}
          <p class="error-message" role="alert">{manualUrlError}</p>
        {/if}
      </div>
    {:else if secretState === 'absent'}
      <!-- #965: a reconnect cannot succeed without a stored client secret
           (the flow starts from the one in the keychain), so the card
           points at onboarding instead of a button that cannot work. -->
      <button class="btn-secondary" onclick={onGoToOnboarding}>{t('settings.runOnboarding')}</button>
    {:else}
      <!-- #965: disconnected with no flow running had no action at all —
           the card said "Not connected" and offered nothing, while the
           Teams row beside it falls through to its own reconnect. -->
      <button class="btn-secondary" onclick={onReconnect} disabled={waiting}>{t('settings.reconnectSpotify')}</button>
    {/if}
  </div>
  {#if authFlow.spotify.error}
    <p class="error-message" role="alert">{authFlow.spotify.error}</p>
  {/if}
  {#if playbackScopeMissing}
    <div class="scope-banner">
      <span class="hint">{t('settings.playbackScopeBanner')}</span>
      <button type="button" class="btn-link" onclick={onReconnect} disabled={waiting}>{t('common.reconnect')}</button>
    </div>
  {/if}
  {#if secretConflict}
    <div class="scope-banner">
      <span class="hint">{t('settings.spotifySecretConflict')}</span>
      <button type="button" class="btn-link" onclick={onReconnect} disabled={waiting}>{t('common.reconnect')}</button>
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
  /* #964: the waiting state is the only Spotify state with more than one
     control, so it stacks instead of sharing the row's baseline. */
  .connection-row .spotify-waiting {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: var(--sp-2);
    width: 100%;
  }
  .connection-row .spotify-waiting .hint { margin: 0; }
  .connection-row .spotify-waiting input { width: 100%; }
  /* One-time-reconnect banner for the missing tray-playback scope
     (issue #3.0-P3). */
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
</style>
