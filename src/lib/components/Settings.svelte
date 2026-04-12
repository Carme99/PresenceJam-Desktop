<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { onMount, onDestroy } from 'svelte';
  import { currentView } from '$lib/stores/app';
  import { configStore, saveConfig, loadConfig, type AppConfig } from '$lib/stores/config';
  import { spotifyConnected } from '$lib/stores/spotify';
  import { teamsConnected } from '$lib/stores/teams';

  let localConfig = $state<AppConfig>({ ...$configStore });
  let isConnected = $state(false);
  let teamsStatusConnected = $state(false);
  let isSaving = $state(false);
  let reconnectingSpotify = $state(false);
  let reconnectingTeams = $state(false);
  let saveMessage = $state('');
  let saveTimeout: ReturnType<typeof setTimeout> | null = null;

  let previewArtist = $state('Radiohead');
  let previewTrack = $state('Karma Police');
  let previewAlbum = $state('OK Computer');
  let previewEmoji = $state('🎵');

  let preview = $derived(
    localConfig.teams.status_format
      .replace('{artist}', previewArtist)
      .replace('{track}', previewTrack)
      .replace('{album}', previewAlbum)
      .replace('{emoji}', previewEmoji)
  );

  onMount(async () => {
    await loadConfig();
    localConfig = JSON.parse(JSON.stringify($configStore));
    
    try {
      const syncStatus = await invoke<any>('get_sync_status');
      isConnected = syncStatus.spotify_connected ?? false;
      teamsStatusConnected = syncStatus.teams_connected ?? false;
    } catch {
      isConnected = $spotifyConnected;
      teamsStatusConnected = $teamsConnected;
    }
  });

  onDestroy(() => {
    if (saveTimeout) {
      clearTimeout(saveTimeout);
      saveTimeout = null;
    }
  });

  async function handleSave() {
    isSaving = true;
    saveMessage = '';
    try {
      await saveConfig(localConfig);
      saveMessage = 'Settings saved!';
      if (saveTimeout) clearTimeout(saveTimeout);
      saveTimeout = setTimeout(() => saveMessage = '', 2000);
    } catch (e) {
      saveMessage = 'Failed to save';
    }
    isSaving = false;
  }

  async function openLogs() {
    await invoke('open_logs_folder');
  }

  async function reconnectSpotify() {
    if (reconnectingSpotify) return;
    reconnectingSpotify = true;
    try {
      await invoke('reconnect_spotify');
    } finally {
      reconnectingSpotify = false;
    }
  }

  async function reconnectTeams() {
    if (reconnectingTeams) return;
    reconnectingTeams = true;
    try {
      await invoke('reconnect_teams');
    } finally {
      reconnectingTeams = false;
    }
  }

  function goBack() {
    currentView.set('dashboard');
  }
</script>

<div class="settings">
  <header class="header">
    <button class="back-btn" onclick={goBack}>← Back</button>
    <h1>Settings</h1>
  </header>

  <div class="sections">
    <section class="card">
      <h2>Spotify</h2>
      <div class="form-group">
        <label for="spotify-client-id">Client ID</label>
        <input 
          id="spotify-client-id"
          type="text" 
          bind:value={localConfig.spotify.client_id}
          readonly={isConnected}
          placeholder="Enter Spotify Client ID"
        />
      </div>
      <div class="form-group">
        <label for="spotify-client-secret">Client Secret</label>
        <input 
          id="spotify-client-secret"
          type="password" 
          bind:value={localConfig.spotify.client_secret}
          readonly={isConnected}
          placeholder={isConnected ? '••••••••' : 'Enter Spotify Client Secret'}
        />
      </div>
      <div class="connection-row">
        {#if isConnected}
          <span class="badge success">Connected</span>
          <button class="btn-secondary" onclick={reconnectSpotify} disabled={reconnectingSpotify}>Reconnect Spotify</button>
        {:else}
          <span class="badge warning">Not Connected</span>
        {/if}
      </div>
    </section>

    <section class="card">
      <h2>Microsoft Teams</h2>
      <p class="hint">Teams authentication uses your Microsoft 365 account. No additional configuration required.</p>
      <div class="connection-row">
        {#if teamsStatusConnected}
          <span class="badge success">Connected</span>
          <button class="btn-secondary" onclick={reconnectTeams} disabled={reconnectingTeams}>Reconnect Teams</button>
        {:else}
          <span class="badge warning">Not Connected</span>
        {/if}
      </div>
    </section>

    <section class="card">
      <h2>Status Format</h2>
      <div class="form-group">
        <label for="status-format">Format Template</label>
        <input 
          id="status-format"
          type="text" 
          bind:value={localConfig.teams.status_format}
          placeholder="🎵 {'{artist}'} - {'{track}'} 🎧"
        />
      </div>
      <div class="form-group">
        <label for="live-preview">Live Preview</label>
        <div id="live-preview" class="preview-box">{preview}</div>
      </div>
      <p class="hint">
        Available placeholders: <code>{'{artist}'}</code>, <code>{'{track}'}</code>, <code>{'{album}'}</code>, <code>{'{emoji}'}</code>
      </p>
      <div class="toggle-row">
        <label for="profanity-filter">Filter profanity in status</label>
        <input
          id="profanity-filter"
          type="checkbox"
          bind:checked={localConfig.teams.profanity_filter}
        />
      </div>
      {#if localConfig.teams.profanity_filter}
        <div class="form-group">
          <label for="profanity-placeholder">Placeholder text</label>
          <p class="hint">
            Use <code>{'{emoji}'}</code> for play state (🎵 playing / ⏸️ paused).
            Shown when profanity is detected in track info.
          </p>
          <input
            id="profanity-placeholder"
            type="text"
            bind:value={localConfig.teams.profanity_placeholder}
            placeholder="Currently Listening to Spotify"
          />
        </div>
      {/if}
    </section>

    <section class="card">
      <h2>Polling</h2>
      <div class="form-group">
        <label for="default-interval">Default Interval: {localConfig.polling.default_interval_seconds}s</label>
        <input 
          id="default-interval"
          type="range" 
          min="10" 
          max="60" 
          step="5"
          bind:value={localConfig.polling.default_interval_seconds}
        />
      </div>
      <div class="row-2">
        <div class="form-group">
          <label for="min-interval">Min Interval (s)</label>
          <input 
            id="min-interval"
            type="number" 
            min="5"
            max="30"
            bind:value={localConfig.polling.minimum_interval_seconds}
          />
        </div>
        <div class="form-group">
          <label for="max-interval">Max Interval (s)</label>
          <input 
            id="max-interval"
            type="number" 
            min="30"
            max="120"
            bind:value={localConfig.polling.max_interval_seconds}
          />
        </div>
      </div>
      <div class="form-group">
        <label for="expiry-buffer">Expiry Buffer (s)</label>
        <input 
          id="expiry-buffer"
          type="number" 
          min="5"
          max="60"
          bind:value={localConfig.polling.expiry_buffer_seconds}
        />
      </div>
    </section>

    <section class="card">
      <h2>Startup</h2>
      <div class="toggle-row">
        <label for="launch-login">Launch at login</label>
        <input 
          id="launch-login"
          type="checkbox" 
          bind:checked={localConfig.teams.launch_at_login}
        />
      </div>
      <div class="toggle-row">
        <label for="start-minimized">Start minimized to tray</label>
        <input 
          id="start-minimized"
          type="checkbox" 
          bind:checked={localConfig.teams.start_minimized}
        />
      </div>
    </section>

    <section class="actions">
      <button class="btn-secondary" onclick={openLogs}>Open Logs Folder</button>
      <button onclick={handleSave} disabled={isSaving}>
        {isSaving ? 'Saving...' : 'Save Settings'}
      </button>
      {#if saveMessage}
        <span class="save-message">{saveMessage}</span>
      {/if}
    </section>
  </div>
</div>

<style>
  .settings {
    padding: 20px;
    max-width: 600px;
    margin: 0 auto;
    height: 100vh;
    display: flex;
    flex-direction: column;
    box-sizing: border-box;
    overflow: hidden;
  }

  .header {
    display: flex;
    align-items: center;
    gap: 16px;
    margin-bottom: 24px;
  }

  .back-btn {
    background: transparent;
    border: 1px solid var(--border-color);
    color: var(--text-secondary);
    padding: 6px 12px;
    font-size: 13px;
  }

  .back-btn:hover {
    background: var(--bg-elevated);
    color: var(--text-primary);
  }

  h1 {
    font-size: 24px;
    font-weight: 600;
  }

  h2 {
    font-size: 16px;
    font-weight: 600;
    margin-bottom: 16px;
    color: var(--text-primary);
  }

  .sections {
    display: flex;
    flex-direction: column;
    gap: 16px;
    flex: 1;
    overflow-y: auto;
  }

  .card {
    background: var(--bg-surface);
    border: 1px solid var(--border-color);
    border-radius: 12px;
    padding: 20px;
  }

  .form-group {
    margin-bottom: 14px;
  }

  .form-group:last-child {
    margin-bottom: 0;
  }

  .form-group label {
    display: block;
    margin-bottom: 6px;
    font-size: 13px;
    font-weight: 500;
    color: var(--text-secondary);
  }

  .connection-row {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-top: 12px;
  }

  .preview-box {
    background: var(--bg-elevated);
    border: 1px solid var(--border-color);
    border-radius: 6px;
    padding: 12px;
    font-size: 14px;
    word-break: break-all;
  }

  .hint {
    font-size: 12px;
    color: var(--text-secondary);
    margin-top: 8px;
  }

  .hint code {
    background: var(--bg-elevated);
    padding: 2px 6px;
    border-radius: 4px;
    font-family: monospace;
  }

  .row-2 {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
  }

  .toggle-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 0;
  }

  .toggle-row label {
    font-size: 14px;
    color: var(--text-primary);
  }

  .toggle-row input[type="checkbox"] {
    width: 18px;
    height: 18px;
    accent-color: var(--color-accent);
  }

  .actions {
    display: flex;
    flex-direction: column;
    gap: 12px;
    margin-top: 8px;
  }

  .btn-secondary {
    background: var(--bg-elevated);
    border: 1px solid var(--border-color);
    color: var(--text-primary);
  }

  .btn-secondary:hover {
    background: var(--bg-surface);
    border-color: var(--color-accent);
  }

  button {
    width: 100%;
    padding: 12px 16px;
  }

  .save-message {
    text-align: center;
    font-size: 13px;
    color: var(--color-success);
  }
</style>
