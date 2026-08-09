<script lang="ts">
  import '../app.css';
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  // Side-effect import — installs the module-level subscribe that
  // applies the persisted theme (and keeps it in sync with future
  // changes). Without this, theme only applies when Settings mounts.
  import '$lib/stores/theme';
  import { devLog } from '$lib/utils/dev';
  import { currentView } from '$lib/stores/app';
  import { authFlow, setTeamsPhase, setTeamsDeviceCode } from '$lib/stores/authFlow.svelte';
  import type { DeviceCodeResponse, TeamsTokens } from '$lib/types';

  devLog(`[LAYOUT] PresenceJam build: ${import.meta.env.VITE_APP_BUILD ?? 'dev build'}`);

  // `teams-reconnect-required` is emitted by the polling loop (refresh
  // failed / 401-403 on set-status) and by `reconnect_teams`. The normal
  // failure case finds the user on the Dashboard, where Settings is not
  // mounted — so the always-mounted layout owns this single listener
  // chain (issue #157): it sets the authFlow phase, navigates to
  // Settings, and runs the full device-code flow (mint code, store it,
  // open the verification URL, start polling). Settings renders the
  // stored code/URI and offers a manual "check now" poll.
  onMount(async () => {
    const unlisten = await listen('teams-reconnect-required', async () => {
      devLog('[LAYOUT] teams-reconnect-required received');
      setTeamsPhase('waiting');
      currentView.set('settings');
      try {
        const response = await invoke<DeviceCodeResponse>('start_teams_auth_device_code');
        setTeamsDeviceCode({
          userCode: response.user_code,
          verificationUrl: response.verification_url,
          deviceCode: response.device_code,
          interval: response.interval
        });
        await invoke('open_external_url', { url: response.verification_url });
        void pollTeamsAuth();
      } catch (e) {
        console.error('[LAYOUT] teams-reconnect-required: start_teams_auth_device_code failed:', e);
        setTeamsPhase('error', String(e));
      }
    });

    // Polls the backend for device-code completion. The cadence is
    // Rust-side; `interval` comes from the DeviceCodeResponse stored in
    // the authFlow store so the server's requested polling rate is
    // honored — see issue #152.
    async function pollTeamsAuth() {
      if (!authFlow.teams.deviceCode) return;
      setTeamsPhase('waiting');
      try {
        const tokens = await invoke<TeamsTokens>('poll_teams_auth', {
          deviceCode: authFlow.teams.deviceCode,
          interval: authFlow.teams.interval
        });
        if (tokens) {
          setTeamsPhase('done');
        }
      } catch (e) {
        console.error('[LAYOUT] poll_teams_auth failed:', e);
        setTeamsPhase('error', String(e));
      }
    }

    return () => {
      unlisten();
    };
  });
</script>

<svelte:head>
  <link rel="icon" type="image/svg+xml" href="/icon.svg" />
  <link rel="alternate icon" type="image/png" href="/favicon.png" />
  <meta name="color-scheme" content="dark light" />
</svelte:head>

<slot />
