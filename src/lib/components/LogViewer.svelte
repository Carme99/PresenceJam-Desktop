<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { onMount, onDestroy } from 'svelte';
  import { currentView } from '$lib/stores/app';

  interface LogEntry {
    timestamp: string;
    level: string;
    message: string;
  }

  let logs = $state<LogEntry[]>([]);
  let filter = $state('All');
  let unlisten: (() => void)[] = [];
  let logContainer: HTMLDivElement;

  onMount(async () => {
    // Note: get_recent_logs is a placeholder in v2 — tauri_plugin_log streams live via Webview
    // The listener below handles all log entries in real-time.

    unlisten.push(await listen<any>('log://log', (event) => {
      // Map numeric level (1=Trace, 2=Debug, 3=Info, 4=Warning, 5=Error) to string
      const levelMap: Record<number, string> = { 1: 'Trace', 2: 'Debug', 3: 'Info', 4: 'Warning', 5: 'Error' };
      const numericLevel = event.payload?.level;
      const levelStr = typeof numericLevel === 'number' ? (levelMap[numericLevel] ?? 'Info') : (numericLevel ?? 'Info');
      logs.push({
        timestamp: new Date().toLocaleTimeString(),
        level: levelStr,
        message: event.payload?.message || ''
      });
      if (logs.length > 500) logs.shift();
      if (logContainer) {
        requestAnimationFrame(() => {
          logContainer.scrollTop = logContainer.scrollHeight;
        });
      }
    }));
  });

  onDestroy(() => unlisten.forEach(fn => fn()));

  let filteredLogs = $derived(
    filter === 'All' ? logs : logs.filter(l => l.level === filter)
  );

  async function openFolder() {
    await invoke('open_logs_folder');
  }

  function clearLogs() {
    logs = [];
  }

  function goBack() {
    currentView.set('dashboard');
  }

  function getLevelClass(level: string): string {
    const l = level.toLowerCase();
    if (l === 'debug') return 'level-debug';
    if (l === 'info') return 'level-info';
    if (l === 'warning' || l === 'warn') return 'level-warning';
    if (l === 'error') return 'level-error';
    return 'level-info';
  }
</script>

<div class="log-viewer">
  <header class="header">
    <button class="back-btn" onclick={goBack}>← Back</button>
    <h1>Logs</h1>
  </header>

  <div class="toolbar">
    <select bind:value={filter}>
      <option value="All">All</option>
      <option value="Debug">Debug</option>
      <option value="Info">Info</option>
      <option value="Warning">Warning</option>
      <option value="Error">Error</option>
    </select>
    <button class="btn-secondary" onclick={clearLogs}>Clear</button>
    <button class="btn-secondary" onclick={openFolder}>Open Folder</button>
  </div>

  <div class="log-list" bind:this={logContainer}>
    {#if filteredLogs.length === 0}
      <div class="empty-state">
        <p>No logs yet</p>
      </div>
    {:else}
      {#each filteredLogs as log}
        <div class="log-entry">
          <span class="timestamp">{log.timestamp}</span>
          <span class="level-badge {getLevelClass(log.level)}">{log.level}</span>
          <span class="message">{log.message}</span>
        </div>
      {/each}
    {/if}
  </div>
</div>

<style>
  .log-viewer {
    display: flex;
    flex-direction: column;
    height: 100vh;
    padding: 20px;
    max-width: 900px;
    margin: 0 auto;
  }

  .header {
    display: flex;
    align-items: center;
    gap: 16px;
    margin-bottom: 16px;
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

  .toolbar {
    display: flex;
    gap: 8px;
    margin-bottom: 12px;
  }

  .toolbar select {
    width: auto;
    min-width: 100px;
  }

  .btn-secondary {
    background: var(--bg-elevated);
    border: 1px solid var(--border-color);
    color: var(--text-primary);
    width: auto;
    padding: 8px 12px;
    font-size: 13px;
  }

  .btn-secondary:hover {
    background: var(--bg-surface);
    border-color: var(--color-accent);
  }

  .log-list {
    flex: 1;
    overflow-y: auto;
    background: var(--bg-surface);
    border: 1px solid var(--border-color);
    border-radius: 8px;
    padding: 8px;
  }

  .log-list::-webkit-scrollbar {
    width: 8px;
  }

  .log-list::-webkit-scrollbar-track {
    background: var(--bg-elevated);
    border-radius: 4px;
  }

  .log-list::-webkit-scrollbar-thumb {
    background: var(--border-color);
    border-radius: 4px;
  }

  .log-list::-webkit-scrollbar-thumb:hover {
    background: var(--text-secondary);
  }

  .empty-state {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
    min-height: 200px;
    color: var(--text-secondary);
  }

  .log-entry {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 6px 8px;
    border-radius: 4px;
    font-size: 13px;
    font-family: 'Cascadia Code', 'Fira Code', monospace;
  }

  .log-entry:hover {
    background: var(--bg-elevated);
  }

  .timestamp {
    color: var(--text-secondary);
    flex-shrink: 0;
  }

  .level-badge {
    flex-shrink: 0;
    padding: 2px 8px;
    border-radius: 4px;
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
  }

  .level-debug {
    background: rgba(128, 128, 128, 0.2);
    color: #888;
  }

  .level-info {
    background: rgba(0, 188, 212, 0.2);
    color: #00bcd4;
  }

  .level-warning {
    background: rgba(251, 191, 36, 0.2);
    color: var(--color-warning);
  }

  .level-error {
    background: rgba(239, 68, 68, 0.2);
    color: var(--color-error);
  }

  .message {
    color: var(--text-primary);
    word-break: break-all;
  }
</style>
