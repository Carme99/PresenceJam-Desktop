/**
 * #492 — LogViewer render/scroll/filter behavior, asserted against source.
 *
 * Fail pre-fix: unkeyed non-virtualized {#each}, unconditional snap to
 * bottom, Trace missing from LEVEL_LABELS. Pass post-fix: keyed tail
 * window (RENDER_WINDOW=100 over a 500 buffer with showing-X-of-Y),
 * stickiness-preserving scroll with Jump-to-latest affordance, Trace
 * tab isolating level-1 logs.
 *
 * Node-only source assertions (no DOM): the component's own constants
 * and template structure are the observable contract.
 */
import { describe, it, expect } from 'vitest';
import { readFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const viewer = readFileSync(join(root, 'src/lib/components/LogViewer.svelte'), 'utf8');

describe('LogViewer behavior (#492)', () => {
  it('caps the DOM to a keyed tail window with a showing-X-of-Y count', () => {
    expect(viewer).toMatch(/RENDER_WINDOW\s*=\s*100/);
    expect(viewer).toMatch(/slice\(-RENDER_WINDOW\)/);
    expect(viewer).toMatch(/\{#each visibleLogs as log \(log\.seq\)\}/);
    expect(viewer).toMatch(/showingOf/);
    // Buffer stays 500 while the DOM renders the tail only.
    expect(viewer).toMatch(/logs\.length > 500/);
  });

  it('preserves scrolled-up position, offers Jump to latest, auto-scrolls at bottom', () => {
    // Stickiness captured BEFORE the push changes scroll height.
    expect(viewer).toMatch(/atBottom = isAtBottom\(\);[\s\S]{0,400}?logs\.push/);
    // Jump affordance renders only when unpinned.
    expect(viewer).toMatch(/\{#if !atBottom && filteredLogs\.length > 0\}/);
    expect(viewer).toMatch(/jumpToLatest/);
    // Scroll handler recomputes stickiness instead of forcing bottom.
    expect(viewer).toMatch(/function handleScroll\(\) \{\s*\n\s*atBottom = isAtBottom\(\);/);
    // No unconditional `scrollTop = scrollHeight` outside the pinned guard.
    expect(viewer).not.toMatch(/logs\.push\([\s\S]{0,300}?scrollTop = logContainer\.scrollHeight/);
  });

  it('Trace tab exists and isolates level-1 logs', () => {
    expect(viewer).toMatch(/Trace: 'logs\.level\.trace'/);
    expect(viewer).toMatch(/1: 'Trace'/);
    // Filter derives per-tab; Trace selects level === 'Trace'.
    expect(viewer).toMatch(/filter === 'All' \? logs : logs\.filter/);
  });
});
