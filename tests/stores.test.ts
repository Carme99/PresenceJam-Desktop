/**
 * Runtime store tests (#420, #425, #422, #423) — import and call the
 * real stores with mocked Tauri IPC. Fail pre-fix (raw numbers stored,
 * aliased default, zombie flag stuck, no cross-window sync); pass
 * post-fix.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';

const invoke = vi.fn();

vi.mock('@tauri-apps/api/core', () => ({ invoke }));

const winState = {
  win: null as null | { setFocus: () => Promise<void> },
  created: 0
};

class FakeWebviewWindow {
  static async getByLabel(_label: string) {
    return winState.win;
  }
  constructor() {
    winState.created++;
  }
  async setFocus() {
    await winState.win?.setFocus();
  }
  once() {
    return 0;
  }
}

vi.mock('@tauri-apps/api/webviewWindow', () => ({
  WebviewWindow: FakeWebviewWindow
}));

beforeEach(() => {
  invoke.mockReset();
  winState.win = null;
  winState.created = 0;
});

describe('config store runtime (#420, #425)', () => {
  it('saveConfig stores bigints, not raw numbers', async () => {
    const { get } = await import('svelte/store');
    const cfg = await import('$lib/stores/config');
    invoke.mockResolvedValueOnce({
      polling: {
        default_interval_seconds: 30,
        minimum_interval_seconds: 10,
        max_interval_seconds: 60,
        expiry_buffer_seconds: 10
      }
    });
    const out = await cfg.saveConfig({
      ...cfg.defaultConfig
    });
    for (const k of [
      'default_interval_seconds',
      'minimum_interval_seconds',
      'max_interval_seconds',
      'expiry_buffer_seconds'
    ] as const) {
      expect(typeof out.polling[k]).toBe('bigint');
      expect(typeof get(cfg.configStore).polling[k]).toBe('bigint');
    }
  });

  it('configStore never aliases defaultConfig', async () => {
    const { get } = await import('svelte/store');
    const cfg = await import('$lib/stores/config');
    expect(get(cfg.configStore)).not.toBe(cfg.defaultConfig);
    invoke.mockRejectedValueOnce(new Error('load failed'));
    const out = await cfg.loadConfig();
    expect(out).not.toBe(cfg.defaultConfig);
    expect(get(cfg.configStore)).not.toBe(cfg.defaultConfig);
  });

  it('backfills a partial polling section so a save can never send null (#541)', async () => {
    // `await import()` (not a static import) is deliberate: the module under
    // test must load after the hoisted `vi.mock` factory has initialised the
    // `invoke` it captures — this file's whole pattern.
    const cfg = await import('$lib/stores/config');
    // The shape a hand-edited config.json produces: the section exists but
    // only one of its fields does. Before the fix the rest stayed
    // `undefined`, `toSavePayload` turned that into `NaN` → `null`, and Rust
    // rejected the `u64` with a raw English IPC error.
    invoke.mockResolvedValueOnce({
      polling: { default_interval_seconds: 42 }
    });
    const out = await cfg.loadConfig();
    expect(out.polling.default_interval_seconds).toBe(BigInt(42));
    for (const k of [
      'default_interval_seconds',
      'minimum_interval_seconds',
      'max_interval_seconds',
      'expiry_buffer_seconds'
    ] as const) {
      expect(typeof out.polling[k]).toBe('bigint');
    }
    expect(out.polling.minimum_interval_seconds).toBe(
      cfg.defaultConfig.polling.minimum_interval_seconds
    );
  });

  it('toSavePayload never emits a value Rust will reject (#541)', async () => {
    const cfg = await import('$lib/stores/config');
    const broken = structuredClone(cfg.defaultConfig);
    const rawPolling = broken.polling as unknown as Record<string, unknown>;
    rawPolling.default_interval_seconds = undefined;
    rawPolling.max_interval_seconds = 'not-a-number';

    const payload = cfg.toSavePayload(broken);

    for (const k of [
      'default_interval_seconds',
      'minimum_interval_seconds',
      'max_interval_seconds',
      'expiry_buffer_seconds'
    ] as const) {
      expect(Number.isFinite(payload.polling[k])).toBe(true);
      expect(typeof payload.polling[k]).toBe('number');
    }
    expect(payload.polling.default_interval_seconds).toBe(
      Number(cfg.defaultConfig.polling.default_interval_seconds)
    );
  });
});

describe('detach store runtime (#422)', () => {
  it('zombie popOut clears the flag and re-creates the window', async () => {
    const { get } = await import('svelte/store');
    const d = await import('$lib/stores/detach');
    winState.win = {
      setFocus: async () => {
        throw new Error('stale handle');
      }
    };
    await d.popOut('logs');
    // Fell through to creation (not stuck on the zombie handle).
    expect(winState.created).toBe(1);
    expect(get(d.detachedPanes).logs).toBe(true);
  });

  it('focusDetached on a missing window clears the badge', async () => {
    const { get } = await import('svelte/store');
    const d = await import('$lib/stores/detach');
    winState.win = null;
    await d.focusDetached('settings');
    expect(get(d.detachedPanes).settings).toBe(false);
  });

  it('concurrent popOut creates at most one window', async () => {
    const d = await import('$lib/stores/detach');
    winState.win = null;
    await Promise.all([d.popOut('settings'), d.popOut('settings')]);
    expect(winState.created).toBeLessThanOrEqual(1);
  });
});

describe('theme cross-window runtime (#423)', () => {
  it('storage event flips the theme; same value is a no-op', async () => {
    const { get } = await import('svelte/store');
    const th = await import('$lib/stores/theme');
    const cur = get(th.theme);
    const other = cur === 'dark' ? 'light' : 'dark';
    let notifies = 0;
    const unsub = th.theme.subscribe(() => {
      notifies++;
    });
    notifies = 0;
    window.dispatchEvent(
      new StorageEvent('storage', { key: th.STORAGE_KEY, newValue: other })
    );
    expect(get(th.theme)).toBe(other);
    expect(notifies).toBe(1);
    notifies = 0;
    window.dispatchEvent(
      new StorageEvent('storage', { key: th.STORAGE_KEY, newValue: other })
    );
    expect(notifies).toBe(0);
    unsub();
  });
});
