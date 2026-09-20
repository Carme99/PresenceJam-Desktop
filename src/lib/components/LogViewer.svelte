<!-- Issue #434 scope: snapshot-copy ONLY — the Copy-snapshot button
  emits the backend redacted tail + version/platform. Virtualization is
  deferred (the RENDER_WINDOW tail cap below is the pre-existing #399
  fix, untouched here); jumpToLatest pre-exists for the #400 stickiness
  path.
  Issue #595 added one more command: the pane also reads the on-disk log
  tail through `get_recent_logs` to seed its buffer on mount. That read is
  raw and local-only (the redacted, paste-able artifact is still the
  snapshot above); no new OAuth scopes are involved. -->
<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { onMount, onDestroy, tick } from 'svelte';
  import PageHeader from './PageHeader.svelte';
  import { currentView } from '$lib/stores/app';
  import { t, tCount, i18n, type TKey } from '$lib/i18n';
  import type { LogPayload } from '$lib/types';
  // C7 multi-window detach: pop-out/pop-back controls.
  import { popOut, popIn } from '$lib/stores/detach';
  import { useListenerTeardown } from '$lib/utils/useAuthListeners';

  // When rendered in the detached `logs-detached` window, "Back" pops the
  // pane back into the main window (closes this one) instead of navigating
  // currentView; the in-main-window-only "Pop out" control is hidden.
  let { detached = false }: { detached?: boolean } = $props();

  interface LogEntry {
    seq: number;
    // Localized HH:MM:SS — `date` is filled only when the row's local day
    // differs from today, so today's rows stay uncluttered.
    timestamp: string;
    date?: string;
    level: string;
    message: string;
  }

  // #399: render-window cap (buffer stays 500, DOM renders the tail only).
  const RENDER_WINDOW = 100;
  // Buffer ceiling — one source for the live push path (#399) and for the
  // #595 backfill seed, and the number the backend clamps its own tail to.
  const MAX_BUFFER = 500;
  // #400: stickiness threshold in px (scrollHeight - scrollTop - clientHeight).
  const SCROLL_THRESHOLD = 48;

  let seqCounter = 0;
  // #400: true while the list is pinned to the bottom.
  let atBottom = $state(true);
  let logs = $state<LogEntry[]>([]);

  // Filter buttons show canonical English level values (compared against
  // the backend's level strings) with translated labels.
  const LEVEL_KEYS: Record<string, TKey> = {
    All: 'logs.level.all',
    Trace: 'logs.level.trace',
    Debug: 'logs.level.debug',
    Info: 'logs.level.info',
    Warning: 'logs.level.warning',
    Error: 'logs.level.error'
  };
  // Backend level strings -> i18n keys; unknown levels render raw.
  // #401: Trace is kept — the listener levelMap (~:112) produces Trace
  // entries, so LEVEL_LABELS must match LEVEL_KEYS or Trace logs are
  // unfilterable.
  const LEVEL_LABELS: Record<string, TKey> = {
    All: LEVEL_KEYS.All,
    Trace: LEVEL_KEYS.Trace,
    Debug: LEVEL_KEYS.Debug,
    Info: LEVEL_KEYS.Info,
    Warning: LEVEL_KEYS.Warning,
    Error: LEVEL_KEYS.Error
  };
  let filter = $state('All');
  // #692: `listen()` resolves asynchronously, so a pane that unmounts while
  // the registration is still in flight used to leak that subscription (the
  // unlisten landed in an array nobody swept again) — one leak per visit.
  // `useListenerTeardown` releases a registration that settles after
  // disposal, which is what every sibling component already uses.
  const teardown = useListenerTeardown();
  let logContainer: HTMLDivElement;

  // #400: stickiness helpers — single source for "pinned to bottom".
  function isAtBottom(): boolean {
    if (!logContainer) return atBottom;
    return logContainer.scrollHeight - logContainer.scrollTop - logContainer.clientHeight < SCROLL_THRESHOLD;
  }

  // #600: while the list is unpinned, hold the reader's place. The DOM
  // renders only the tail window (#399), so every appended entry evicts a
  // row from the top and the text would otherwise slide upward one row per
  // event — continuous at Trace level. Anchor on a row that survives the
  // eviction, then re-apply its content-space delta once the DOM updated.
  // The pinned path below is untouched.
  let anchorSeq = -1;
  let anchorOffset = 0;

  function captureScrollAnchor() {
    anchorSeq = -1;
    if (!logContainer || atBottom) return;
    const rows = logContainer.querySelectorAll<HTMLElement>('.log-entry');
    if (rows.length === 0) return;
    // Row 0 is exactly what an append evicts; anchor one behind it so the
    // anchor outlives the update in the common single-entry case.
    const row = rows[rows.length > 1 ? 1 : 0];
    const seq = Number(row.dataset.seq);
    if (!Number.isFinite(seq)) return;
    anchorSeq = seq;
    anchorOffset = row.offsetTop;
  }

  function restoreScrollAnchor() {
    if (anchorSeq < 0 || !logContainer) return;
    const seq = anchorSeq;
    anchorSeq = -1;
    // The reader re-pinned between capture and this frame (Jump to latest,
    // or a scroll back to the bottom): the pinned path owns the scroll now.
    if (atBottom) return;
    const row = logContainer.querySelector<HTMLElement>(`.log-entry[data-seq="${seq}"]`);
    // Evicted by a burst — nothing to hold on to; leave the browser alone.
    if (!row) return;
    logContainer.scrollTop += row.offsetTop - anchorOffset;
  }

  // Recompute stickiness and snap when pinned. Pinned state survives
  // content swaps (e.g. filter tabs); an unpinned view only auto-pins
  // when the new content fits entirely in view, hiding the Jump button.
  function updateStickinessAndSnap() {
    if (!logContainer) return;
    if (logContainer.scrollHeight <= logContainer.clientHeight + SCROLL_THRESHOLD) {
      atBottom = true;
    }
    if (!atBottom) {
      atBottom = isAtBottom();
      if (!atBottom) {
        // #600: unpinned — hold the reader's place instead of snapping.
        // Same rAF the pinned path uses, so both land in one frame.
        requestAnimationFrame(() => {
          restoreScrollAnchor();
          atBottom = isAtBottom();
        });
        return;
      }
    }
    // Snap inside rAF only, after re-reading: a mid-frame scroll-up
    // must not get yanked back to the bottom.
    const el = logContainer;
    requestAnimationFrame(() => {
      // Re-read geometry: a mid-frame scroll-up must not get yanked back.
      const still =
        el.scrollHeight - el.scrollTop - el.clientHeight < SCROLL_THRESHOLD;
      atBottom = still;
      if (still) {
        el.scrollTop = el.scrollHeight;
      }
    });
  }

  // Filter tabs swap the visible list: wait a tick for the DOM, then
  // recompute stickiness (pinned stays pinned and snaps).
  function selectFilter(f: string) {
    filter = f;
    void tick().then(() => updateStickinessAndSnap());
  }

  // Three-way count label without a nested template ternary.
  function describeCount(shown: number, total: number): string {
    if (shown < total) return t('logs.showingOf', { shown, total });
    return tCount('logs.count', total);
  }

  // #595: seed the buffer from the on-disk tail. The pane used to open empty
  // and stay that way until the next live event, while PresenceJam.log already
  // held the session's history — and "Copy snapshot" pasted that same history,
  // so the two surfaces contradicted each other.
  //
  // The backend read is raw (no redaction): it feeds the local viewer showing
  // the very file the user can open from the toolbar, not the paste-able
  // support artifact (#434/#487) — that one still comes solely from
  // `get_diagnostics_snapshot`.
  //
  // On-disk line format (tauri-plugin-log desktop default, written in UTC):
  //   [YYYY-MM-DD][HH:MM:SS][target][LEVEL] message
  const LOG_LINE_RE =
    /^\[(\d{4}-\d{2}-\d{2})\]\[(\d{2}:\d{2}:\d{2})\]\[([^\]]*)\]\[(\w+)\]\s?(.*)$/;
  // The backend clamps `limit` to this same ceiling.
  const BACKFILL_LINES = MAX_BUFFER;

  /** Backend level word (`WARN`) -> the canonical name the filter tabs use. */
  function canonicalLevel(raw: string): string {
    const l = raw.toUpperCase();
    if (l === 'TRACE') return 'Trace';
    if (l === 'DEBUG') return 'Debug';
    if (l === 'WARN' || l === 'WARNING') return 'Warning';
    if (l === 'ERROR') return 'Error';
    return 'Info';
  }

  /** YYYY-MM-DD in the system timezone, used to compare a row's day to today. */
  function localDateKey(d: Date): string {
    return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
  }

  /** One on-disk log line -> the shape the list renders. */
  function parseLogLine(line: string): Omit<LogEntry, 'seq'> {
    const m = LOG_LINE_RE.exec(line);
    if (!m) {
      // Not a plugin-formatted record (verbatim write, rotated fragment):
      // keep the text rather than dropping the line.
      return { timestamp: '', level: 'Info', message: line };
    }
    // The file is UTC; the live rows below are local, so convert — otherwise
    // one instant would print as two different times in the same column.
    const at = new Date(`${m[1]}T${m[2]}Z`);
    const locale = i18n.locale;
    if (Number.isNaN(at.getTime())) {
      return { date: m[1], timestamp: m[2], level: canonicalLevel(m[4]), message: `[${m[3]}] ${m[5]}` };
    }
    // #969: keep the date on rows whose local day differs from today, so up
    // to 500 backfilled lines from a multi-day PresenceJam.log are
    // distinguishable ("09:14:02 today" vs "09:14:02 yesterday"). Today's
    // rows stay uncluttered (no date span, no layout shift).
    const date =
      localDateKey(at) === localDateKey(new Date()) ? undefined : at.toLocaleDateString(locale);
    return {
      date,
      timestamp: at.toLocaleTimeString(locale),
      level: canonicalLevel(m[4]),
      message: `[${m[3]}] ${m[5]}`
    };
  }

  // #595: Clear is an instruction to empty the pane — a tail that lands after
  // it must not resurrect what the user just wiped.
  let seedCancelled = false;

  // #958: composite key for the seed-vs-live dedupe. A record logged in the
  // backfill window can land twice — once in the file tail the
  // `get_recent_logs` read returns, and once as a `log://log` event the
  // listener pushes to `logs` before the read resolves. The pre-#958
  // merge `[...seeded, ...logs].slice(-MAX_BUFFER)` therefore rendered
  // the row twice and counted it twice. We dedupe by
  // `(timestamp, level, message)` at merge time: drop any seeded entry
  // whose key already lives in `logs`. The file tail is the older copy,
  // so keeping the seeded entry at its position preserves chronological
  // order; the live counterpart collapses. Live events continue to push
  // straight to `logs` so the stream renders live and the #400/#600
  // stickiness behaviour stays intact.
  function dedupeKey(entry: { timestamp: string; level: string; message: string }): string {
    return `${entry.timestamp}|${entry.level}|${entry.message}`;
  }

  /**
   * Prepend the on-disk history to whatever the live stream has already
   * delivered. A pane the user has scrolled away from keeps its exact scroll
   * position: only a pane still pinned to the bottom follows the content.
   */
  async function seedHistory() {
    let lines: unknown;
    try {
      lines = await invoke<unknown>('get_recent_logs', { limit: BACKFILL_LINES });
    } catch (e) {
      // Backfill is an enhancement — the live stream keeps working without it.
      console.warn('[LOGVIEWER] get_recent_logs failed:', e);
      return;
    }
    if (!Array.isArray(lines) || lines.length === 0 || seedCancelled) return;
    const seeded: LogEntry[] = (lines as string[]).map(line => ({
      seq: seqCounter++,
      ...parseLogLine(line)
    }));

    // #958: drop any seeded entry whose key already lives in `logs`. The
    // listener has already pushed the live events that landed during the
    // read into `logs`, so the union we want is `seeded ∪ logs` with the
    // duplicate collapsed to one row. `slice(-MAX_BUFFER)` runs AFTER the
    // dedupe so the size invariant survives the merge.
    const liveKeys = new Set(logs.map(dedupeKey));
    const dedupedSeeded = seeded.filter((s) => !liveKeys.has(dedupeKey(s)));
    logs = [...dedupedSeeded, ...logs].slice(-MAX_BUFFER);
    if (!atBottom) return;
    await tick();
    updateStickinessAndSnap();
  }

  onMount(async () => {
    // Register the live stream FIRST: the disk read below is slower than the
    // first events, and `log://log` stays authoritative for everything from
    // here on — the seed only prepends what already happened. The
    // registration is handed to the teardown rather than awaited, so an
    // unmount before it resolves still releases it (#692).
    teardown.add(listen<LogPayload>('log://log', (event) => {
      // #400: capture stickiness BEFORE the push changes the scroll height.
      atBottom = isAtBottom();
      // #600: anchor before the push shifts the rendered window.
      captureScrollAnchor();
      // Map numeric level (1=Trace, 2=Debug, 3=Info, 4=Warning, 5=Error) to string
      const levelMap: Record<number, string> = { 1: 'Trace', 2: 'Debug', 3: 'Info', 4: 'Warning', 5: 'Error' };
      const numericLevel = event.payload?.level;
      const levelStr = typeof numericLevel === 'number' ? (levelMap[numericLevel] ?? 'Info') : (numericLevel ?? 'Info');
      logs.push({
        seq: seqCounter++,
        timestamp: new Date().toLocaleTimeString(i18n.locale),
        level: levelStr,
        message: event.payload?.message || ''
      });
      if (logs.length > MAX_BUFFER) logs.shift();
      updateStickinessAndSnap();
    }));

    await seedHistory();
  });

  // #692: releasing an in-flight registration is the teardown's job, so
  // there is nothing to await here.
  onDestroy(() => {
    void teardown.dispose();
  });

  let filteredLogs = $derived(
    filter === 'All' ? logs : logs.filter(l => l.level === filter)
  );

  // #399: render-window over the tail — buffer keeps 500, DOM renders <= 100.
  let visibleLogs = $derived(filteredLogs.slice(-RENDER_WINDOW));

  // Issue #979: a failed open must be visible in the pane, not just in
  // the developer console. Reuse `snapshotFeedback` so the user sees
  // a localised toast the same way they see copy-success / copy-fail.
  // Auto-clears on the next open attempt.
  let openFolderFeedback = $state('');

  // Count label as a derived string (cases live in describeCount).
  let countLabel = $derived(describeCount(visibleLogs.length, filteredLogs.length));

  function handleScroll() {
    atBottom = isAtBottom();
  }

  function jumpToLatest() {
    atBottom = true;
    if (logContainer) {
      logContainer.scrollTop = logContainer.scrollHeight;
    }
  }

  async function openFolder() {
    openFolderFeedback = '';
    try {
      await invoke('open_logs_folder');
    } catch (e) {
      // Issue #979: surface in-pane; the backend now returns a non-empty
      // error string instead of silently dispatching a no-op spawn.
      console.warn('[LOGVIEWER] open_logs_folder failed:', e);
      const msg = String((e as Error)?.message ?? e).slice(0, 180);
      openFolderFeedback = msg || t('logs.openFolderError');
    }
  }
  // Issue #434: one-click redacted support snapshot. Clipboard text comes
  // SOLELY from the backend `get_diagnostics_snapshot` — its `recent_logs`
  // already passed through `redact_sensitive` (no tokens/emails/absolute
  // paths; #487 allowlist) — plus app version + platform for triage. The
  // live in-memory `logs` buffer is deliberately NOT included: it never
  // passes the redactor, so pasting it would leak exactly what #434
  // forbids. Clipboard-only, no new commands, no new OAuth scopes.
  let snapshotFeedback = $state('');
  async function copySnapshot() {
    snapshotFeedback = '';
    try {
      const snap = await invoke<{
        app_version: string;
        os: { platform: string; arch: string; family: string };
        log_source_status: string;
        recent_logs: string[];
      }>('get_diagnostics_snapshot');
      const header = [
        `PresenceJam support snapshot v${snap.app_version}`,
        `Platform: ${snap.os.platform}/${snap.os.arch} (${snap.os.family})`,
        `Log source: ${snap.log_source_status} (redacted)`
      ].join('\n');
      await navigator.clipboard.writeText(`${header}\n\n${snap.recent_logs.join('\n')}`);
      snapshotFeedback = t('logs.snapshotCopied');
    } catch (e) {
      console.warn('[LOGVIEWER] copySnapshot failed:', e);
      snapshotFeedback = t('logs.snapshotCopyFailed');
    }
  }

  // #403: catch-and-surface — WebviewWindow creation/focus can reject;
  // never leave the promise floating from an inline onclick.
  function handlePopOut() {
    popOut('logs').catch((e: unknown) => console.warn('[LOGVIEWER] popOut failed:', e));
  }

  function clearLogs() {
    // #595: drop a tail still in flight — Clear must empty the pane, not
    // have history reappear in it a moment later.
    seedCancelled = true;
    logs = [];
  }

  function goBack() {
    if (detached) {
      // #403: catch-and-surface — a detached window close can reject
      // (e.g. the window was already closed); never leave it floating.
      popIn('logs').catch((e: unknown) => console.warn('[LOGVIEWER] popIn failed:', e));
      return;
    }
    currentView.set('dashboard');
  }

  function getLevelClass(level: string): string {
    const l = level.toLowerCase();
    // #401: Trace shares the muted debug badge until it gets its own
    // accent; must not silently fall through to level-info.
    if (l === 'trace') return 'level-debug';
    if (l === 'debug') return 'level-debug';
    if (l === 'info') return 'level-info';
    if (l === 'warning' || l === 'warn') return 'level-warning';
    if (l === 'error') return 'level-error';
    return 'level-info';
  }
</script>

<div class="log-viewer">
  <PageHeader title={t('logs.title')} onBack={goBack} showLogo={false} showThemeToggle={false}
    backLabel={detached ? t('settings.popBackIn') : t('common.back')} />

  <div class="toolbar">
    <div class="seg" role="tablist" aria-label={t('logs.filterAria')}>
      {#each (Object.keys(LEVEL_LABELS) as (keyof typeof LEVEL_LABELS)[]) as f}
        <button type="button" class="seg-btn btn-secondary"
          class:is-active={filter === f}
          onclick={() => selectFilter(f)} role="tab"
          aria-selected={filter === f}>{t(LEVEL_LABELS[f])}</button>
      {/each}
    </div>
    <span class="count" aria-live="polite">{countLabel}</span>
    {#if !detached}
      <button class="btn-secondary" onclick={handlePopOut}>{t('logs.popOut')}</button>
    {/if}
    <button class="btn-secondary" onclick={clearLogs}>{t('logs.clear')}</button>
    <button class="btn-secondary" onclick={openFolder}>{t('logs.openFolder')}</button>
    <button class="btn-secondary" onclick={copySnapshot}>{t('logs.copySnapshot')}</button>
  </div>
  {#if snapshotFeedback}
    <p class="snapshot-feedback" role="status" aria-live="polite">{snapshotFeedback}</p>
  {/if}
  {#if openFolderFeedback}
    <p class="snapshot-feedback" role="status" aria-live="polite">{openFolderFeedback}</p>
  {/if}

  <div class="log-wrap">
    <div class="log-list" bind:this={logContainer} onscroll={handleScroll}>
      {#if filteredLogs.length === 0}
        <div class="empty-state">
          <p>{t('logs.empty')}</p>
          <p class="hint">{t('logs.emptyHint')}</p>
        </div>
      {:else}
        {#each visibleLogs as log (log.seq)}
          <div class="log-entry" data-seq={log.seq}>
            <span class="timestamp">
              {#if log.date}<span class="date">{log.date}</span>{/if}
              {log.timestamp}
            </span>
            <span class="level-badge {getLevelClass(log.level)}">{LEVEL_KEYS[log.level] ? t(LEVEL_KEYS[log.level]) : log.level}</span>
            <span class="message">{log.message}</span>
          </div>
        {/each}
      {/if}
    </div>
    {#if !atBottom && filteredLogs.length > 0}
      <button class="jump-latest" onclick={jumpToLatest}>{t('logs.jumpToLatest')}</button>
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
  .log-wrap {
    position: relative;
    flex: 1;
    display: flex;
    min-height: 0;
  }
  .log-list {
    flex: 1;
    overflow-y: auto;
    /* #600: the anchoring below is the single source of truth. Chromium's
     * own scroll anchoring (WebView2 on Windows) already compensates for
     * rows evicted above the viewport, so leaving it on would apply the
     * correction twice and shove the reader the other way. WebKit has no
     * anchoring and ignores this. */
    overflow-anchor: none;
    background: var(--bg-surface);
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    padding: var(--sp-2);
    font-family: var(--font-mono);
  }
  .jump-latest {
    position: absolute;
    bottom: 12px;
    left: 50%;
    transform: translateX(-50%);
    padding: var(--sp-2) var(--sp-4);
    font-size: var(--fs-sm);
    background: var(--bg-elevated);
    color: var(--fg);
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    box-shadow: var(--shadow-1);
    cursor: pointer;
  }
  .snapshot-feedback {
    margin: 0;
    font-size: var(--fs-xs);
    color: var(--fg-subtle);
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
  /* #969: a non-today row carries its date in front of the time. Smaller and
   * dimmer than the timestamp so the common (today) row keeps its visual
   * weight — the criterion is distinguishability, not prominence. */
  .date {
    color: var(--fg-subtle);
    opacity: 0.7;
    font-size: var(--fs-xs);
    margin-right: var(--sp-2);
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
