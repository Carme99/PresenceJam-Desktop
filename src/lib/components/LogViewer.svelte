<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { onMount, onDestroy } from 'svelte';
  import PageHeader from './PageHeader.svelte';
  import { currentView } from '$lib/stores/app';
  import type { LogPayload } from '$lib/types';

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

    unlisten.push(await listen<LogPayload>('log://log', (event) => {
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
  <PageHeader title="Logs" onBack={goBack} showLogo={false} showThemeToggle={false} />

  <div class="toolbar">
    <div class="seg" role="tablist" aria-label="Log level filter">
      {#each ['All', 'Debug', 'Info', 'Warning', 'Error'] as f}
        <button type="button" class="seg-btn btn-secondary"
          class:is-active={filter === f}
          onclick={() => (filter = f)} role="tab"
          aria-selected={filter === f}>{f}</button>
      {/each}
    </div>
    <span class="count" aria-live="polite">{filteredLogs.length} {filteredLogs.length === 1 ? 'entry' : 'entries'}</span>
    <button class="btn-secondary" onclick={clearLogs}>Clear</button>
    <button class="btn-secondary" onclick={openFolder}>Open folder</button>
  </div>

  <div class="log-list" bind:this={logContainer}>
    {#if filteredLogs.length === 0}
      <div class="empty-state">
        <p>No log entries yet</p>
        <p class="hint">Live entries stream here as the polling loop runs.</p>
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
    padding: var(--sp-5);
    max-width: 980px;
    margin: 0 auto;
    gap: var(--sp-4);
  }

  .toolbar {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
  }
  .count {
    margin-right: auto;
    font-size: var(--fs-xs);
    font-weight: 600;
    color: var(--fg-subtle);
    text-transform: uppercase;
    letter-spacing: 0.08em;
  }
  .toolbar .btn-secondary {
    width: auto;
    padding: var(--sp-2) var(--sp-4);
    font-size: var(--fs-sm);
  }

  .seg {
    display: inline-flex;
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    padding: 3px;
    gap: 2px;
  }
  .seg-btn {
    border: none;
    background: transparent;
    color: var(--fg-muted);
    padding: 4px var(--sp-3);
    font-size: var(--fs-sm);
    border-radius: var(--r-sm);
    width: auto;
    transition: background-color var(--dur-fast) var(--ease-out),
                color var(--dur-fast) var(--ease-out);
  }
  .seg-btn:hover { background: var(--bg-surface); color: var(--fg); }
  .seg-btn.is-active {
    background: var(--bg-surface);
    color: var(--fg);
    box-shadow: var(--shadow-1);
  }

  .log-list {
    flex: 1;
    overflow-y: auto;
    background: var(--bg-surface);
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    padding: var(--sp-2);
    font-family: var(--font-mono);
  }
  .empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: var(--sp-1);
    height: 100%;
    min-height: 240px;
    color: var(--fg-subtle);
    font-family: var(--font-sans);
  }
  .empty-state p {
    color: var(--fg-muted);
    font-size: var(--fs-base);
  }
  .empty-state .hint { font-size: var(--fs-sm); }

  .log-entry {
    display: grid;
    grid-template-columns: 88px 80px 1fr;
    align-items: flex-start;
    gap: var(--sp-3);
    padding: var(--sp-2) var(--sp-3);
    border-radius: var(--r-sm);
    font-size: var(--fs-sm);
  }
  .log-entry:hover { background: var(--bg-elevated); }

  .timestamp {
    color: var(--fg-subtle);
    font-variant-numeric: tabular-nums;
    font-size: var(--fs-xs);
  }
  .level-badge {
    justify-self: start;
    padding: 2px 8px;
    border-radius: var(--r-sm);
    font-size: 11px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .level-debug {
    background: var(--bg-elevated);
    color: var(--fg-subtle);
  }
  .level-info {
    background: var(--info-soft);
    color: var(--info);
  }
  .level-warning {
    background: var(--warning-soft);
    color: var(--warning);
  }
  .level-error {
    background: var(--danger-soft);
    color: var(--danger);
  }
  .message {
    color: var(--fg);
    word-break: break-all;
    line-height: 1.5;
  }
</style>
