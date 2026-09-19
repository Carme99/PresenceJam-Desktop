<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { currentView, type View } from '$lib/stores/app';
  import Onboarding from '$lib/components/Onboarding.svelte';
  import Dashboard from '$lib/components/Dashboard.svelte';
  import Settings from '$lib/components/Settings.svelte';
  import LogViewer from '$lib/components/LogViewer.svelte';
  import Diagnostics from '$lib/components/Diagnostics.svelte';
  import { devLog } from '$lib/utils/dev';
  import { useListenerTeardown } from '$lib/utils/useAuthListeners';
  import About from '$lib/components/About.svelte';
  import Reconnect from '$lib/components/Reconnect.svelte';
  import { bootView } from '$lib/utils/boot';
  import { clientSecretStateOf, loadConfig } from '$lib/stores/config';
  import { t } from '$lib/i18n';

  // Build info — injected at build time via vite.config.js define
  // (mirrors the consumer in About.svelte; the vite define key is the
  // path-based `import.meta.env.VITE_APP_BUILD`, not the bare token
  // `__APP_BUILD__` that esbuild's define plugin can't match against a
  // member expression).
  const BUILD = import.meta.env.VITE_APP_BUILD ?? 'dev build';

  let ready = $state(false);
  let bootError = $state('');
  // #967: boot's `is_onboarding_complete()` verdict, kept in state so the
  // navigate listener below can tell a first-run install (nothing behind the
  // wizard) from a configured one that re-entered it via Settings' "Run
  // onboarding" or the boot probe's fail-open path.
  let onboardingComplete = $state(false);
  // #615: one teardown for every `listen()` below — it releases a
  // registration that settles after this component is destroyed, so the
  // per-listener `destroyed` flag is gone.
  const teardown = useListenerTeardown();

  // #405: bounded boot — the invoke below must never hang the loading
  // screen forever (e.g. an IPC stall). The race rejects after
  // BOOT_TIMEOUT_MS so `ready` always resolves; failures fall back to
  // onboarding with a visible retry banner instead of a dead spinner.
  const BOOT_TIMEOUT_MS = 8000;

  function withBootTimeout<T>(p: Promise<T>): Promise<T> {
    return new Promise<T>((resolve, reject) => {
      const timer = setTimeout(() => reject(new Error('timed out')), BOOT_TIMEOUT_MS);
      p.then(
        (v) => { clearTimeout(timer); resolve(v); },
        (e) => { clearTimeout(timer); reject(e instanceof Error ? e : new Error(String(e))); }
      );
    });
  }

  async function boot() {
    devLog('[PAGE] boot: calling invoke is_onboarding_complete');
    bootError = '';
    try {
      // #530: the gate now refreshes an expired-but-refreshable session itself,
      // so an incomplete verdict means a sign-in is genuinely required. When the
      // credentials are still stored, that is a returning user, not a new one:
      // route them to Reconnect instead of the setup wizard. The probe is inside
      // the same timeout so a stalled keychain read cannot strand the spinner.
      const view = await withBootTimeout(
        (async () => {
          const complete = await invoke<boolean>('is_onboarding_complete');
          onboardingComplete = complete;
          devLog('[PAGE] boot: is_onboarding_complete SUCCESS, complete=', complete);
          return bootView(complete, await hasStoredSpotifyCredentials());
        })()
      );
      currentView.set(view);
      devLog('[PAGE] boot: currentView set to:', view);
    } catch (e) {
      console.error('[PAGE] boot: is_onboarding_complete FAILED:', e);
      bootError = e instanceof Error ? e.message : String(e);
      // #544: a failed boot probe (IPC stall, transient keychain error, the
      // 8s withBootTimeout) is not evidence that this is a new install, and
      // hardcoding 'onboarding' here re-introduced the exact outcome #530
      // removed for the success path. Route the failure through the same
      // decision so a returning user with stored credentials keeps it.
      //
      // Fail direction: the credential probe fails OPEN to the wizard
      // (`hasStoredSpotifyCredentials` returns false when its config read
      // fails) — deliberately, because (a) that is the documented contract
      // of the existing probe, (b) a keychain error means Reconnect could
      // not read the stored secret either, so routing there would dead-end,
      // and (c) since #542 the wizard MERGES into the stored config and
      // prefills from it, so it no longer destroys working settings.
      //
      // The probe is bounded by the same policy as the gate itself: an
      // unbounded second await would reintroduce the #405 hang on the very
      // path that exists because IPC stalled.
      const hasCredentials = await withBootTimeout(hasStoredSpotifyCredentials()).catch(
        () => false
      );
      const view = bootView(false, hasCredentials);
      currentView.set(view);
      devLog('[PAGE] boot: currentView set to', view, '(from error)');
    }
    ready = true;
    devLog('[PAGE] boot: ready=true');
  }

  // #530: does this install still hold the Spotify credentials (Client ID in
  // config + secret in the OS keychain) that a reconnect can reuse? Any probe
  // failure falls back to the wizard, which can recreate everything.
  //
  // 4.7.0 (issue #674): the read goes through the shared store instead of a raw
  // `invoke('load_config')`, so this probe — which runs on every boot, on both
  // the success and the failure path below — is what hydrates `configStore`.
  // Nothing else does on a Dashboard-first launch (the normal one for an
  // already-configured user), and the i18n store reads `config.locale` from
  // that store (and keys its one-shot legacy-locale migration on the store's
  // hydration flag): without it the webview kept the frontend defaults for the
  // whole session — a German UI with an English tray.
  //
  // `loadConfig()` never rejects: a failed read resolves with the frontend
  // defaults, whose secret state is `absent`, so this still fails OPEN to the
  // wizard, and `configHydrated` stays false so nothing is written on the
  // defaults' behalf.
  async function hasStoredSpotifyCredentials(): Promise<boolean> {
    try {
      const cfg = await loadConfig();
      if (!cfg.spotify.client_id?.trim()) return false;
      // #560: only a *positively absent* secret means there is nothing to
      // reuse. `is_spotify_client_secret_set` is the `Present`-only projection
      // of a tri-state, so it answers `false` both for "no credential" and for
      // "the keychain could not answer" (locked Secret Service, no daemon) —
      // and this gate then re-onboarded a user whose secret was still stored.
      // The config carries the state, so no extra probe is needed.
      return clientSecretStateOf(cfg) !== 'absent';
    } catch (e) {
      console.warn('[PAGE] boot: credential probe failed:', e);
      return false;
    }
  }

  function retryBoot() {
    ready = false;
    void boot();
  }
  onMount(() => {
    devLog('[PAGE] onMount: ENTRY');
    void boot();
    devLog('[PAGE] onMount: setting up tray-click listener');
    teardown.add(listen('tray-click', async () => {
      devLog('[PAGE] EVENT: tray-click received');
      devLog('[PAGE] EVENT: calling invoke show_window');
      try {
        await invoke('show_window');
      } catch (e) {
        console.warn('[PAGE] show_window failed:', e);
      }
    }));
    devLog('[PAGE] onMount: setting up app-shutdown listener');
    teardown.add(listen('app-shutdown', async () => {
      devLog('[PAGE] EVENT: app-shutdown received');
      try {
        await invoke('app_exit');
        devLog('[PAGE] EVENT: app_exit SUCCESS');
      } catch (e) {
        console.error('[PAGE] EVENT: app_exit FAILED:', e);
      }
    }));

    devLog('[PAGE] onMount: setting up navigate listener');
    teardown.add(listen<string>('navigate', (event) => {
      devLog('[PAGE] EVENT: navigate received:', event.payload);
      // C2: deep-link auth completions also emit 'navigate'. While the
      // Onboarding view is up it owns its own phase transitions — jumping
      // to another view would strand setup half-done — so programmatic
      // navigation is ignored until onboarding yields the view.
      //
      // #967: the one exception is the Dashboard for an install that is already
      // configured. The wizard offers that escape hatch itself (its header
      // control), and without this the app menu's "Show Dashboard" stayed inert
      // for a returning user who had been routed into the wizard. First-run
      // installs keep the old one-way behaviour.
      if (!ready) return;
      const wantsDashboard = event.payload === 'dashboard' && onboardingComplete;
      if ($currentView === 'onboarding' && !wantsDashboard) return;
      currentView.set(event.payload as View);
    }));

    devLog('[PAGE] onMount: setting up open-logs-folder listener');
    teardown.add(listen('open-logs-folder', async () => {
      devLog('[PAGE] EVENT: open-logs-folder received');
      try {
        await invoke('open_logs_folder');
      } catch (e) {
        console.error('[PAGE] EVENT: open_logs_folder FAILED:', e);
      }
    }));

    devLog('[PAGE] onMount: setting up show-about listener');
    teardown.add(listen('show-about', () => {
      devLog('[PAGE] EVENT: show-about received');
      currentView.set('about');
    }));

    devLog('[PAGE] onMount: setting up toggle-pause listener');
    teardown.add(listen('toggle-pause', async () => {
      devLog('[PAGE] EVENT: toggle-pause received');
      try {
        const status = await invoke<{ is_syncing: boolean }>('get_sync_status');
        if (status.is_syncing) {
          devLog('[PAGE] EVENT: calling invoke stop_syncing');
          await invoke('stop_syncing');
        } else {
          devLog('[PAGE] EVENT: calling invoke start_syncing');
          await invoke('start_syncing');
        }
      } catch (e) {
        console.warn('[PAGE] toggle-pause failed:', e);
      }
    }));

    return () => {
      devLog('[PAGE] onDestroy: ENTRY');
      void teardown.dispose();
      devLog('[PAGE] onDestroy: all listeners cleaned up');
      devLog('[PAGE] onDestroy: EXIT');
    };
  });

  devLog('[PAGE] currentView value:', $currentView);
</script>
{#if !ready}
  <div class="loading">
    <span>{t('common.loading')}</span>
  </div>
{:else}
  <div class="app-container" id="main-content" tabindex="-1">
    {#if bootError}
      <div class="boot-error" role="alert">
        <span>{t('common.bootFailed')}{bootError ? `: ${bootError}` : ''}</span>
        <button type="button" class="btn-secondary" onclick={retryBoot}>{t('common.retry')}</button>
      </div>
    {/if}
    {#if $currentView === 'onboarding'}
      <Onboarding />
    {:else if $currentView === 'dashboard'}
      <Dashboard />
    {:else if $currentView === 'settings'}
      <Settings />
    {:else if $currentView === 'logs'}
      <LogViewer />
    {:else if $currentView === 'diagnostics'}
      <Diagnostics />
    {:else if $currentView === 'about'}
      <About />
    {:else if $currentView === 'reconnect'}
      <Reconnect />
    {/if}
    <div class="version">{BUILD}</div>
  </div>
{/if}

<style>
  .loading {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100vh;
    background: var(--bg-base);
    color: var(--fg-muted);
    font-size: 16px;
  }
  .boot-error {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 12px;
    padding: 10px 16px;
    background: var(--danger-soft);
    color: var(--danger);
    font-size: 14px;
  }
  .app-container {
    height: 100vh;
    display: flex;
    flex-direction: column;
  }
  .version {
    position: fixed;
    bottom: 8px;
    right: 12px;
    font-size: 11px;
    color: var(--fg-subtle);
    opacity: 0.6;
    pointer-events: none;
  }
</style>
