/**
 * Store/logic invariants (#420, #422, #423, #425, #500, #497, #499, #498).
 *
 * Fail pre-fix: saveConfig stored the raw reply (numbers, not bigints),
 * configStore aliased defaultConfig, popOut/focusDetached never cleared a
 * zombie flag, theme had no cross-window listener, Reconnect claimed a
 * fresh Teams success on mount, lib.rs logged a stale version / bare
 * ENOENT, layout threw outside Tauri. Pass post-fix: each source guard
 * below holds.
 */
import { describe, it, expect } from 'vitest';
import { readFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');

function read(p: string): string {
  return readFileSync(join(root, p), 'utf8');
}

describe('store and runtime invariants', () => {
  it('saveConfig normalizes the persisted reply (#420)', () => {
    const cfg = read('src/lib/stores/config.ts');
    expect(cfg).toMatch(/const normalized = normalizeLoadedConfig\(persisted\)/);
    expect(cfg).toMatch(/configStore\.set\(normalized\)/);
  });

  it('configStore never aliases defaultConfig (#425)', () => {
    const cfg = read('src/lib/stores/config.ts');
    expect(cfg).toMatch(/writable<AppConfig>\(structuredClone\(defaultConfig\)\)/);
    expect(cfg).toMatch(/const fallback = structuredClone\(defaultConfig\)/);
    expect(cfg).not.toMatch(/configStore\.set\(defaultConfig\)/);
  });

  it('zombie windows clear the detached flag (#422)', () => {
    const detach = read('src/lib/stores/detach.ts');
    // popOut catch clears + falls through (no early return inside catch).
    expect(detach).toMatch(/markDetached\(pane, false\);\s*\n\s*\}/);
    // focusDetached miss path clears.
    expect(detach).toMatch(/\} else \{\s*\n\s*\/\/ #422[^\n]*\n\s*markDetached\(pane, false\);/);
  });

  it('theme converges across windows with pre-paint bootstrap (#423)', () => {
    const theme = read('src/lib/stores/theme.ts');
    expect(theme).toMatch(/export const STORAGE_KEY/);
    expect(theme).toMatch(/addEventListener\('storage'/);
    const html = read('src/app.html');
    expect(html).toMatch(/presencejam:theme/);
    expect(html).toMatch(/%sveltekit\.head%/);
  });

  it('Reconnect reserves success wording for in-session reconnects (#500)', () => {
    const rec = read('src/lib/components/Reconnect.svelte');
    expect(rec).toMatch(/teamsReconnectedThisSession/);
    expect(rec).toMatch(/phase === 'done' && teamsReconnectedThisSession/);
    expect(rec).not.toContain('teamsAlreadyConnected');
  });

  it('lib.rs reports the real version and names the missing helper (#497, #499)', () => {
    const lib = read('src-tauri/src/lib.rs');
    expect(lib).toMatch(/PresenceJam \{\} started successfully", env!\("CARGO_PKG_VERSION"\)/);
    expect(lib).not.toContain('PresenceJam 2.0 started successfully');
    expect(lib).toMatch(/update-desktop-database/);
    expect(lib).toMatch(/xdg-mime default/);
  });

  it('layout renders a notice outside Tauri instead of throwing (#498)', () => {
    const layout = read('src/routes/+layout.svelte');
    expect(layout).toMatch(/__TAURI_INTERNALS__/);
    expect(layout).toMatch(/\{#if !isTauriRuntime\}/);
    expect(layout).toMatch(/if \(!isTauriRuntime \|\| !isMainWindow\) return;/);
  });
});
