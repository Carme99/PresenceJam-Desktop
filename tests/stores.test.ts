/**
 * Runtime store tests (#420, #425, #422, #423, #618) — import and call the
 * real stores with mocked Tauri IPC. Fail pre-fix (raw numbers stored,
 * aliased default, zombie flag stuck, no cross-window sync, a badge that
 * survives its window); pass post-fix.
 *
 * Every store is pulled in with `await import()` inside the test that needs
 * it: the module state under test (`detachedPanes`, `configStore`) is
 * process-wide, so the load boundary has to sit inside the per-test reset.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';

const invoke = vi.fn();

vi.mock('@tauri-apps/api/core', () => ({ invoke }));

type FakeWindow = {
  setFocus: () => Promise<void>;
  close: () => Promise<void>;
  once: (event: string, handler: (e?: unknown) => void) => void;
};

const winState = {
  win: null as FakeWindow | null,
  created: 0,
  closed: 0,
  // tauri://destroyed handler the store registered on the window it created —
  // recorded so the badge-teardown path is reachable. Creation failures no
  // longer arrive as `tauri://error` events on the window (issue #922): the
  // store invokes the `detach_pane` command instead and rejects in its own
  // catch block, which is driven directly in the test below.
  handlers: {} as Record<string, (e?: unknown) => void>
};

class FakeWebviewWindow {
  static async getByLabel(_label: string) {
    return winState.win;
  }
  constructor() {
    winState.created++;
  }
  async setFocus(): Promise<void> {
    // no-op — the real setFocus focuses an already-open child window.
  }
  async close(): Promise<void> {
    winState.closed++;
  }
  once(event: string, handler: (e?: unknown) => void) {
    winState.handlers[event] = handler;
  }
}

// Issue #922: detached windows are now opened by the Rust `detach_pane`
// command, not by the main window's `WebviewWindow` constructor. Tests stub
// the command by handing the store a fake window handle so `getByLabel` can
// resolve it after the command "returns".
invoke.mockImplementation(async (cmd: string) => {
  if (cmd === 'detach_pane') {
    const win = new FakeWebviewWindow();
    winState.win = win as unknown as FakeWindow;
    return undefined;
  }
  return undefined;
});

vi.mock('@tauri-apps/api/webviewWindow', () => ({
  WebviewWindow: FakeWebviewWindow
}));

beforeEach(() => {
  invoke.mockReset();
  // Issue #922: re-install the default `detach_pane` handler after every
  // reset — `mockReset` clears the implementation as well as the call
  // history, and the detach store relies on it.
  invoke.mockImplementation(async (cmd: string) => {
    if (cmd === 'detach_pane') {
      const win = new FakeWebviewWindow();
      winState.win = win as unknown as FakeWindow;
      return undefined;
    }
    return undefined;
  });
  winState.win = null;
  winState.created = 0;
  winState.closed = 0;
  winState.handlers = {};
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

  /**
   * `logging.max_file_size_mb` joined the ts-rs `u64` fields in 4.7.0 (#673),
   * so it needs the same two-way treatment polling gets: a bigint in the store
   * (what the backend types are) and a plain number in the payload (a bigint
   * cannot be encoded, a NaN serializes as null and Rust's `u64` rejects it).
   * A pre-4.7 or hand-edited file omits the key entirely — it must land on the
   * default, not `undefined`.
   */
  it('normalises the logging u64 exactly like the polling ones (#673)', async () => {
    const cfg = await import('$lib/stores/config');
    invoke.mockResolvedValueOnce({
      logging: { enabled: true, log_level: 'Debug', keep_files: 7 },
      polling: { default_interval_seconds: 30 }
    });

    const loaded = await cfg.loadConfig();

    expect(loaded.logging.max_file_size_mb).toBe(BigInt(10));
    expect(typeof loaded.logging.max_file_size_mb).toBe('bigint');
    expect(loaded.logging.keep_files).toBe(7);

    const payload = cfg.toSavePayload(loaded);

    expect(typeof payload.logging.max_file_size_mb).toBe('number');
    expect(payload.logging.max_file_size_mb).toBe(10);
    expect(payload.logging.keep_files).toBe(7);
    expect(() => JSON.stringify(payload)).not.toThrow();
  });
});

describe('detach store runtime (#422)', () => {
  it('zombie popOut clears the flag and re-creates the window', async () => {
    const { get } = await import('svelte/store');
    const d = await import('$lib/stores/detach');
    winState.win = {
      setFocus: async () => {
        throw new Error('stale handle');
      },
      close: async () => {},
      once: () => {}
    };
    await d.popOut('logs');
    // Fell through to creation (not stuck on the zombie handle).
    expect(winState.created).toBe(1);
    expect(get(d.detachedPanes).logs).toBe(true);
  });

  it('re-creates exactly one window when two popOut calls race', async () => {
    const d = await import('$lib/stores/detach');
    winState.win = null;
    await Promise.all([d.popOut('settings'), d.popOut('settings')]);
    // Not `toBeLessThanOrEqual(1)`: a guard that returned early and created
    // nothing would satisfy that. Coalescing means exactly one creation.
    expect(winState.created).toBe(1);
  });

  it('clears the badge when the detached window is destroyed or fails to open', async () => {
    const { get } = await import('svelte/store');
    const d = await import('$lib/stores/detach');
    winState.win = null;

    await d.popOut('logs');
    expect(get(d.detachedPanes).logs).toBe(true);
    winState.handlers['tauri://destroyed']?.();
    expect(get(d.detachedPanes).logs).toBe(false);

    // Issue #922: a refused creation now comes back through `invoke()`,
    // not a `tauri://error` event on the window. Drive that path instead:
    // when the Rust `detach_pane` command rejects, the badge must clear.
    invoke.mockImplementationOnce(async () => {
      throw new Error('creation failed');
    });
    await d.popOut('logs');
    expect(get(d.detachedPanes).logs).toBe(false);
  });

  it('popIn closes a live window and clears the badge when it is gone', async () => {
    const { get } = await import('svelte/store');
    const d = await import('$lib/stores/detach');

    let closeCalls = 0;
    winState.win = {
      setFocus: async () => {},
      close: async () => {
        closeCalls++;
      },
      once: () => {}
    };
    d.detachedPanes.set({ logs: false, settings: true });
    await d.popIn('settings');
    expect(closeCalls).toBe(1);

    // No window behind the badge (closed from its own title bar): the flag
    // must still clear, or the Dashboard keeps offering "focus".
    winState.win = null;
    d.detachedPanes.set({ logs: false, settings: true });
    await d.popIn('settings');
    expect(get(d.detachedPanes).settings).toBe(false);
  });

  it('focusDetached on a missing window clears the badge', async () => {
    const { get } = await import('svelte/store');
    const d = await import('$lib/stores/detach');
    winState.win = null;
    d.detachedPanes.set({ logs: false, settings: true });
    await d.focusDetached('settings');
    expect(get(d.detachedPanes).settings).toBe(false);
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
