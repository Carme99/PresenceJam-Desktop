/**
 * #781 — the Diagnostics Save, Copy and Dismiss actions, exercised through
 * the real component with a mocked `invoke`.
 *
 * The bug class this pins is a handler that reports success unconditionally:
 * `save_diagnostics_snapshot` rejects, the clipboard rejects, or
 * `clear_failed_update_install` rejects, and the pane still says it worked.
 * Every case below asserts the OBSERVED outcome for both the resolving and
 * the rejecting path, so a handler that always took the happy branch fails
 * the suite rather than shipping green.
 *
 * The `saving` double-click guard is asserted by counting the IPC calls while
 * the first save is still in flight.
 *
 * The sibling `tests/diagnostics-quarantine.test.ts` owns the session
 * quarantine banner (including its "dismissing invokes no command" contract)
 * and is left untouched.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, cleanup, waitFor, fireEvent } from '@testing-library/svelte';

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));

vi.mock('@tauri-apps/api/core', () => ({ invoke }));

import Diagnostics from '$lib/components/Diagnostics.svelte';
import { currentView } from '$lib/stores/app';
import type { DiagnosticsSnapshot } from '$lib/types';

/** A snapshot carrying the #244 failed-update-install record. */
function snapshotWithFailedInstall(): DiagnosticsSnapshot {
  return baseSnapshot();
}

function baseSnapshot(): DiagnosticsSnapshot {
  return {
    app_version: '4.7.0',
    tauri_version: '2.0.0',
    os: { platform: 'linux', arch: 'x86_64', family: 'unix' },
    config: {
      spotify_client_id: '',
      redirect_uri: 'http://127.0.0.1:8899/callback',
      client_secret_set: false,
      clear_on_pause: true,
      profanity_filter: true,
      start_minimized: false,
      availability_sync: false,
      presence_gate: false,
      default_interval_seconds: 30,
      minimum_interval_seconds: 10,
      maximum_interval_seconds: 60,
      expiry_buffer_seconds: 60,
      logging_enabled: true,
      log_level: 'Info',
      autostart: false,
      quiet_hours_count: 0,
      quiet_hours_enabled_count: 0,
      track_rules_count: 0,
      track_rules_enabled_count: 0,
      config_quarantined: false,
      config_quarantine_backup: null,
      respect_manual_status: true,
      gate_when_out_of_office: false,
      profanity_extra_words_count: 4,
      locale: 'en',
      update_channel: 'stable',
      snoozed: false,
      snooze_minutes_left: null
    },
    tokens: {
      spotify_connected: false,
      spotify_expires_at: null,
      spotify_expired: false,
      teams_connected: false,
      teams_expires_at: null,
      teams_expired: false,
      teams_refresh_token_present: null
    },
    keychain: { spotify_client_secret_present: false, tokens_encryption_key_present: false },
    sync_state: {
      is_syncing: false,
      snoozed: false,
      snooze_minutes_left: null,
      manual_status_blocks: false,
      presence_gate_reason: null,
      transient_failure_count: 0,
      consecutive_network_failures: 0
    },
    recent_logs: ['[2026-01-01][12:00:00][pj][INFO] hello'],
    log_source_status: 'ok: last 1 of 1 lines',
    failed_update_install: {
      version: '4.6.9',
      error: 'exit status 1',
      timestamp: '2026-01-01 12:00:00'
    }
  } as unknown as DiagnosticsSnapshot;
}

/** Mount Diagnostics with `snapshot` as the command's reply. */
async function mountWith(snapshot: DiagnosticsSnapshot) {
  invoke.mockImplementation(async (cmd: string) =>
    cmd === 'get_diagnostics_snapshot' ? snapshot : undefined
  );
  const rendered = render(Diagnostics);
  await waitFor(() =>
    expect(rendered.container.querySelector('.content dl')).not.toBeNull()
  );
  return rendered;
}

/** Counts of each command the component invoked, in call order. */
function commandCounts(): Record<string, number> {
  const counts: Record<string, number> = {};
  for (const call of invoke.mock.calls) {
    const name = call[0] as string;
    counts[name] = (counts[name] ?? 0) + 1;
  }
  return counts;
}

/**
 * The clipboard write double installed by `stubClipboard` below: a callable
 * that also records the text it was handed, which is what the Copy
 * assertions read back.
 */
interface ClipboardWriteDouble {
  (text: string): Promise<void>;
  mock: { calls: [string][] };
}
let writeText: ClipboardWriteDouble;

/** Install a clipboard double whose write outcome the test controls. */
function stubClipboard(impl: () => Promise<void>) {
  // The mock's own call type is untyped-parameterised; the recorded argument
  // is always the clipboard text this component passes.
  writeText = vi.fn(impl) as unknown as ClipboardWriteDouble;
  Object.defineProperty(navigator, 'clipboard', {
    configurable: true,
    value: { writeText }
  });
}

beforeEach(() => {
  invoke.mockReset();
  currentView.set('dashboard');
});

afterEach(() => {
  cleanup();
});

describe('Diagnostics Save to file (#781)', () => {
  it('reports the saved copy and re-enables the button when the write resolves', async () => {
    const { getByRole } = await mountWith(baseSnapshot());
    invoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'get_diagnostics_snapshot') return baseSnapshot();
      if (cmd === 'save_diagnostics_snapshot') return '/home/test/Downloads/report.json';
      return undefined;
    });

    await fireEvent.click(getByRole('button', { name: 'Save to file' }));

    await waitFor(() =>
      expect(getByRole('status').textContent).toContain(
        'Diagnostics saved to your downloads folder.'
      )
    );
    // The webview supplies neither bytes nor a destination (#921).
    expect(invoke).toHaveBeenCalledWith('save_diagnostics_snapshot');
    expect(invoke).not.toHaveBeenCalledWith(
      'save_diagnostics_snapshot',
      expect.anything()
    );
    expect((getByRole('button', { name: 'Save to file' }) as HTMLButtonElement).disabled).toBe(
      false
    );
  });

  it('reports the failure and re-enables the button when the write rejects', async () => {
    const { container, getByRole } = await mountWith(baseSnapshot());
    invoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'get_diagnostics_snapshot') return baseSnapshot();
      if (cmd === 'save_diagnostics_snapshot') throw new Error('disk full');
      return undefined;
    });

    await fireEvent.click(getByRole('button', { name: 'Save to file' }));

    // The failure copy, not the success copy — this is the assertion that a
    // handler reporting success unconditionally cannot satisfy.
    await waitFor(() =>
      expect(getByRole('status').textContent).toContain(
        'Save failed — use "Copy diagnostics" instead.'
      )
    );
    expect(getByRole('status').textContent).not.toContain('saved to your downloads folder');
    // The in-memory snapshot survives a failed save, so Copy still works.
    expect(container.querySelector('.content dl')).not.toBeNull();
    expect((getByRole('button', { name: 'Save to file' }) as HTMLButtonElement).disabled).toBe(
      false
    );
  });

  it('starts exactly one save when the button is double-clicked while one is pending', async () => {
    await mountWith(baseSnapshot());

    // A save the test controls: it stays in flight until released.
    let release: (() => void) | undefined;
    invoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'get_diagnostics_snapshot') return baseSnapshot();
      if (cmd === 'save_diagnostics_snapshot') {
        await new Promise<void>((resolve) => {
          release = resolve;
        });
        return '/home/test/Downloads/report.json';
      }
      return undefined;
    });

    const button = getSaveButton(document.body);
    await fireEvent.click(button);
    await fireEvent.click(button);
    await fireEvent.click(button);

    // Three clicks, one save — the #598 `saving` guard.
    expect(commandCounts()['save_diagnostics_snapshot']).toBe(1);
    expect((button as HTMLButtonElement).disabled).toBe(true);

    release?.();
    await waitFor(() => expect((button as HTMLButtonElement).disabled).toBe(false));
    expect(commandCounts()['save_diagnostics_snapshot']).toBe(1);
  });
});

function getSaveButton(root: HTMLElement): HTMLElement {
  const button = [...root.querySelectorAll('button')].find(
    (b) => b.textContent?.trim() === 'Save to file'
  );
  expect(button, 'Save to file button must be present').toBeDefined();
  return button as HTMLElement;
}

describe('Diagnostics Copy diagnostics (#781)', () => {
  it('puts the rendered snapshot on the clipboard and confirms it', async () => {
    const snapshot = baseSnapshot();
    const { getByRole } = await mountWith(snapshot);
    stubClipboard(async () => undefined);

    await fireEvent.click(getByRole('button', { name: 'Copy diagnostics' }));

    await waitFor(() => expect(writeText).toHaveBeenCalledTimes(1));
    // The clipboard payload parses back to the snapshot the pane rendered.
    expect(JSON.parse(writeText.mock.calls[0][0] as string)).toEqual(snapshot);
    await waitFor(() =>
      expect(getByRole('status').textContent).toContain(
        'Diagnostics copied to clipboard.'
      )
    );
  });

  it('reports the failure when the clipboard rejects', async () => {
    const { getByRole } = await mountWith(baseSnapshot());
    stubClipboard(async () => {
      throw new Error('clipboard denied');
    });

    await fireEvent.click(getByRole('button', { name: 'Copy diagnostics' }));

    await waitFor(() =>
      expect(getByRole('status').textContent).toContain(
        'Copy failed — use "Save to file" instead.'
      )
    );
    expect(getByRole('status').textContent).not.toContain('copied to clipboard');
  });
});

describe('Diagnostics dismiss failed install (#781)', () => {
  it('clears the record on disk and drops the section', async () => {
    const { container } = await mountWith(snapshotWithFailedInstall());
    expect(container.querySelector('.failed-install')).not.toBeNull();

    invoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'get_diagnostics_snapshot') return baseSnapshot();
      if (cmd === 'clear_failed_update_install') return null;
      return undefined;
    });

    const dismiss = failedInstallDismiss(container);
    await fireEvent.click(dismiss);

    await waitFor(() =>
      expect(commandCounts()['clear_failed_update_install']).toBe(1)
    );
    await waitFor(() => expect(container.querySelector('.failed-install')).toBeNull());
  });

  it('keeps the section and reports the failure when the clear rejects', async () => {
    const { container, getByRole } = await mountWith(snapshotWithFailedInstall());

    invoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'get_diagnostics_snapshot') return baseSnapshot();
      if (cmd === 'clear_failed_update_install') throw new Error('record file is read-only');
      return undefined;
    });

    await fireEvent.click(failedInstallDismiss(container));

    await waitFor(() =>
      expect(getByRole('status').textContent).toContain(
        'Could not dismiss the failed-install record.'
      )
    );
    // The record is still on disk, so the UI must still show it.
    expect(container.querySelector('.failed-install')).not.toBeNull();
  });
});

/** The Dismiss button inside the failed-install section (not the quarantine one). */
function failedInstallDismiss(container: HTMLElement): HTMLElement {
  const section = container.querySelector('.failed-install');
  expect(section, 'failed-install section must be rendered').not.toBeNull();
  const button = [...section!.querySelectorAll('button')].find(
    (b) => b.textContent?.trim() === 'Dismiss'
  );
  expect(button, 'failed-install Dismiss button must be present').toBeDefined();
  return button as HTMLElement;
}