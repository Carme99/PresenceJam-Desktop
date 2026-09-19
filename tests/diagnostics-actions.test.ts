/**
 * #781 — the Diagnostics Save, Copy and Dismiss actions.
 *
 * `tests/diagnostics-quarantine.test.ts` asserts that the quarantine's
 * Dismiss invokes no command, which is correct for that banner but left the
 * three real actions uncovered: `save_diagnostics_snapshot`,
 * `clear_failed_update_install` and the clipboard write appear nowhere under
 * `tests/`. That is the bug class where a handler reports success
 * unconditionally — the user is told a diagnostics file was written when
 * nothing was (#598's pre-fix shape), and nothing fails.
 *
 * These tests drive the real component: the Save feedback is asserted for
 * both outcomes, the captured `json` argument is parsed back and compared
 * with the fixture the page rendered, a double click while the first write
 * is in flight must start exactly one save (#598's `saving` guard), Copy is
 * asserted for a delivered and for a refused clipboard write, and Dismiss is
 * asserted to both clear the record on disk and drop it from the snapshot
 * the other two actions serialize.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, cleanup, waitFor, fireEvent } from '@testing-library/svelte';

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));

vi.mock('@tauri-apps/api/core', () => ({ invoke }));

import Diagnostics from '$lib/components/Diagnostics.svelte';
import { currentView } from '$lib/stores/app';
import { t } from '$lib/i18n';
import type { DiagnosticsSnapshot } from '$lib/types';

type FailedInstall = NonNullable<DiagnosticsSnapshot['failed_update_install']>;

const FAILED_INSTALL: FailedInstall = {
  version: '4.7.0',
  error: 'installer exited 1',
  timestamp: '2026-09-18T21:04:11+00:00'
};

/**
 * A snapshot shaped like the command's reply. Wire numbers arrive as JS
 * numbers (ts-rs types some of them `bigint`); the page renders and
 * stringifies them, so the fixture uses numbers throughout.
 */
function snapshot(failedInstall: FailedInstall | null = null): DiagnosticsSnapshot {
  return {
    app_version: '4.7.0',
    tauri_version: '2.0.0',
    os: { platform: 'linux', arch: 'x86_64', family: 'unix' },
    config: {
      spotify_client_id: 'test-client-id',
      redirect_uri: 'http://127.0.0.1:8899/callback',
      client_secret_set: true,
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
      config_quarantine_backup: null
    },
    tokens: {
      spotify_connected: true,
      spotify_expires_at: null,
      spotify_expired: false,
      teams_connected: true,
      teams_expires_at: null,
      teams_expired: false,
      teams_refresh_token_present: true
    },
    keychain: { spotify_client_secret_present: true, tokens_encryption_key_present: true },
    recent_logs: ['[INFO] [APP] ready'],
    log_source_status: 'ok: last 1 of 1 lines',
    failed_update_install: failedInstall
  };
}

const writeText = vi.fn<(text: string) => Promise<void>>();

/** The JSON the last clipboard write carried. */
function clipboardText(): string {
  expect(writeText).toHaveBeenCalled();
  return writeText.mock.calls[writeText.mock.calls.length - 1][0];
}

/** Mount Diagnostics and wait until the collected snapshot is on screen. */
async function mountDiagnostics(snap: DiagnosticsSnapshot) {
  invoke.mockImplementation(async (cmd: string) =>
    cmd === 'get_diagnostics_snapshot' ? snap : undefined
  );
  const rendered = render(Diagnostics);
  await waitFor(() => expect(rendered.container.querySelector('.content dl')).not.toBeNull());
  return rendered;
}

/** The `role="status"` line the three actions report through. */
function feedback(container: HTMLElement): string {
  return container.querySelector('.feedback')?.textContent?.trim() ?? '';
}

beforeEach(() => {
  invoke.mockReset();
  currentView.set('dashboard');
  // The failure paths log the rejection; keep the run output clean.
  vi.spyOn(console, 'warn').mockImplementation(() => {});
  writeText.mockReset();
  writeText.mockResolvedValue(undefined);
  Object.defineProperty(window.navigator, 'clipboard', {
    value: { writeText },
    configurable: true
  });
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe('Diagnostics save-to-file (#781)', () => {
  it('writes the snapshot it is showing, and reports the saved file', async () => {
    const snap = snapshot();
    const { container, getByRole } = await mountDiagnostics(snap);

    await fireEvent.click(getByRole('button', { name: t('diagnostics.saveToFile') }));

    await waitFor(() => expect(feedback(container)).toBe(t('diagnostics.savedToDownloads')));
    // The file's contents are the on-screen snapshot, not a re-collected or
    // summarised variant of it.
    const [cmd, args] = invoke.mock.calls.filter(
      ([name]) => name === 'save_diagnostics_snapshot'
    )[0];
    expect(cmd).toBe('save_diagnostics_snapshot');
    expect(JSON.parse((args as { json: string }).json)).toEqual(snap);
  });

  it('reports a refused write as a failure and re-enables the button', async () => {
    const { container, getByRole } = await mountDiagnostics(snapshot());
    // Only the write itself fails; the snapshot load above must succeed.
    invoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'save_diagnostics_snapshot') throw new Error('read-only filesystem');
      return undefined;
    });
    const save = () =>
      getByRole('button', { name: t('diagnostics.saveToFile') }) as HTMLButtonElement;

    await fireEvent.click(save());

    await waitFor(() => expect(feedback(container)).toBe(t('diagnostics.saveFailed')));
    // A handler that reported success unconditionally would show the "saved"
    // copy here instead — the assertion above is what catches it.
    expect(feedback(container)).not.toBe(t('diagnostics.savedToDownloads'));
    expect(save().disabled).toBe(false);
  });

  it('starts one save when the button is clicked twice before the first resolves', async () => {
    const pending = Promise.withResolvers<string>();
    const { container, getByRole } = await mountDiagnostics(snapshot());
    invoke.mockImplementation(async (cmd: string) =>
      cmd === 'save_diagnostics_snapshot' ? pending.promise : undefined
    );
    const save = getByRole('button', { name: t('diagnostics.saveToFile') });

    // `saving` reaches the DOM on Svelte's next flush, so the second dispatch
    // here reaches the handler while the first write is still pending — only
    // the `saving` guard (#598) keeps it to one write.
    const first = fireEvent.click(save);
    const second = fireEvent.click(save);
    pending.resolve('/home/user/Downloads/presencejam-diagnostics.json');
    await Promise.all([first, second]);

    expect(
      invoke.mock.calls.filter(([name]) => name === 'save_diagnostics_snapshot')
    ).toHaveLength(1);
    await waitFor(() => expect(feedback(container)).toBe(t('diagnostics.savedToDownloads')));
  });
});

describe('Diagnostics copy (#781)', () => {
  it('puts the pretty-printed snapshot on the clipboard and confirms it', async () => {
    const snap = snapshot(FAILED_INSTALL);
    const { container, getByRole } = await mountDiagnostics(snap);

    await fireEvent.click(getByRole('button', { name: t('diagnostics.copy') }));

    await waitFor(() => expect(feedback(container)).toBe(t('diagnostics.copied')));
    expect(clipboardText()).toBe(JSON.stringify(snap, null, 2));
  });

  it('reports a refused clipboard write instead of claiming the copy worked', async () => {
    writeText.mockRejectedValue(new Error('clipboard denied'));
    const { container, getByRole } = await mountDiagnostics(snapshot());

    await fireEvent.click(getByRole('button', { name: t('diagnostics.copy') }));

    // The fallback copy names the action that still works: claiming success
    // here is exactly the regression this pins.
    await waitFor(() => expect(feedback(container)).toBe(t('diagnostics.copyFailed')));
    expect(feedback(container)).not.toBe(t('diagnostics.copied'));
  });
});

describe('Diagnostics failed-install dismiss (#781)', () => {
  it('clears the record on disk and out of what Copy would paste', async () => {
    const snap = snapshot(FAILED_INSTALL);
    const { container, getByRole } = await mountDiagnostics(snap);
    expect(container.querySelector('.failed-install')).not.toBeNull();

    await fireEvent.click(getByRole('button', { name: t('common.dismiss') }));

    await waitFor(() => expect(container.querySelector('.failed-install')).toBeNull());
    expect(invoke.mock.calls.map(([cmd]) => cmd)).toContain('clear_failed_update_install');
    // The snapshot the other two actions serialize drops the record too, so
    // "Copy diagnostics" cannot contradict the screen (#244's stated policy).
    await fireEvent.click(getByRole('button', { name: t('diagnostics.copy') }));
    expect(JSON.parse(clipboardText())).toMatchObject({ failed_update_install: null });
  });
});
