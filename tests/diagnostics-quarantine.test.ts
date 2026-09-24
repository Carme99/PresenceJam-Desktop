/**
 * #537 (CfgDiag#2) — the quarantine banner on the Diagnostics page.
 *
 * A corrupt `config.json` is quarantined to `config.json.bak` and the app
 * boots on defaults; before this slice nothing told the user, so every
 * setting they had (client id, polling, quiet hours, track rules) silently
 * read as a factory default. The Rust half carries the state into
 * `ConfigSummary` (see `diagnostics::tests` for the snapshot contract);
 * this pins the half the user actually sees, through the real component:
 *
 *   - a quarantine from this process renders the banner, naming the backup
 *     file so the user can find it;
 *   - a `.bak` left over from an *earlier* launch still renders it — the
 *     `config_quarantined` flag is per-process, so a restart is exactly
 *     when a backup-only banner matters most;
 *   - a clean install renders nothing;
 *   - Dismiss is session-local: it invokes no command, so the only on-disk
 *     evidence (the backup) and the snapshot fields stay intact.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, cleanup, waitFor, fireEvent } from '@testing-library/svelte';

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));

vi.mock('@tauri-apps/api/core', () => ({ invoke }));

import Diagnostics from '$lib/components/Diagnostics.svelte';
import { currentView } from '$lib/stores/app';
import type { DiagnosticsSnapshot } from '$lib/types';

/**
 * A snapshot shaped like the command's reply. Wire numbers arrive as JS
 * numbers (ts-rs types them `bigint`), so the fixture uses numbers — the
 * page renders them and "Copy diagnostics" stringifies them.
 */
function snapshotWith(quarantined: boolean, backup: string | null): DiagnosticsSnapshot {
  return {
    app_version: '4.6.0',
    tauri_version: '2.0.0',
    os: {
      platform: 'linux',
      arch: 'x86_64',
      family: 'unix',
      os_version: 'Ubuntu 24.04.1 LTS',
      install_flavor: 'AppImage'
    },
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
      config_quarantined: quarantined,
      config_quarantine_backup: backup,
      respect_manual_status: true,
      gate_when_out_of_office: false,
      profanity_extra_words_count: 4,
      locale: 'de-DE',
      update_channel: 'beta',
      snoozed: true,
      snooze_minutes_left: 17,
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
    recent_logs: [],
    log_source_status: 'ok: last 0 of 0 lines',
    failed_update_install: null
  } as unknown as DiagnosticsSnapshot;
}

async function mountWith(snapshot: DiagnosticsSnapshot) {
  invoke.mockImplementation(async (cmd: string) =>
    cmd === 'get_diagnostics_snapshot' ? snapshot : undefined
  );
  const rendered = render(Diagnostics);
  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith('get_diagnostics_snapshot')
  );
  // Let the resolved snapshot render before the caller queries.
  await waitFor(() => expect(rendered.container.querySelector('.content dl')).not.toBeNull());
  return rendered;
}

function banner(container: HTMLElement): HTMLElement | null {
  return container.querySelector('.quarantine');
}

function diagnosticRow(container: HTMLElement, label: string): string {
  const term = [...container.querySelectorAll('dt')].find(
    (item) => item.textContent?.trim() === label
  );
  expect(term).toBeDefined();
  const value = term?.nextElementSibling?.textContent?.trim();
  expect(value).toBeDefined();
  return value ?? '';
}

beforeEach(() => {
  invoke.mockReset();
  currentView.set('dashboard');
});

afterEach(() => {
  cleanup();
});

describe('Diagnostics quarantine banner (#537)', () => {
  it('explains the reset and names the backup file after a quarantine', async () => {
    const { container } = await mountWith(snapshotWith(true, 'config.json.bak'));
    const notice = banner(container);
    expect(notice).not.toBeNull();
    const text = notice!.textContent ?? '';
    // The reset must be stated as the cause, not just as a weird default.
    expect(text).toContain('could not read');
    // The one pointer the user has to their lost settings.
    expect(text).toContain('config.json.bak');
    // …and it is announced, not decorative.
    expect(notice!.getAttribute('role')).toBe('alert');
  });

  it('still warns when the backup is left over from an earlier launch', async () => {
    // `config_was_quarantined` is per-process and the user relaunches before
    // they notice; the `.bak` on disk is the only surviving evidence.
    const { container } = await mountWith(snapshotWith(false, 'config.json.bak'));
    const text = banner(container)?.textContent ?? '';
    expect(text).toContain('config.json.bak');
    expect(text).toContain('previous launch');
  });

  it('reports a lost original it could not keep aside', async () => {
    // Rename failure: the flag is raised but no `.bak` exists.
    const { container } = await mountWith(snapshotWith(true, null));
    const text = banner(container)?.textContent ?? '';
    expect(text).toContain('still next to your settings file');
  });

  it('shows nothing for a config that loaded cleanly', async () => {
    const { container } = await mountWith(snapshotWith(false, null));
    await waitFor(() => expect(container.querySelector('.content dl')).not.toBeNull());
    expect(banner(container)).toBeNull();
  });

  it('dismisses for the session without touching the backup or the snapshot', async () => {
    const { container, getByRole } = await mountWith(snapshotWith(true, 'config.json.bak'));
    await fireEvent.click(getByRole('button', { name: 'Dismiss' }));
    await waitFor(() => expect(banner(container)).toBeNull());
    // No clear/delete command: the backup IS the user's lost settings, so a
    // dismiss must not be able to destroy it (unlike #244's failed-install
    // record, whose only purpose was to be acknowledged).
    expect(invoke.mock.calls.map((c) => c[0])).toEqual(['get_diagnostics_snapshot']);
  });
});

describe('Diagnostics runtime and config summary (#875, #786, #864)', () => {
  it('shows every requested runtime and config row', async () => {
    const { container } = await mountWith(snapshotWith(false, null));

    expect(diagnosticRow(container, 'OS')).toBe(
      'linux (x86_64, unix); Release: Ubuntu 24.04.1 LTS; Install flavor: appimage'
    );
    expect(diagnosticRow(container, 'Respect manual status')).toBe('Yes');
    expect(diagnosticRow(container, 'Gate while out of office')).toBe('No');
    expect(diagnosticRow(container, 'Locale')).toBe('de-DE');
    expect(diagnosticRow(container, 'Update channel')).toBe('beta');
    expect(diagnosticRow(container, 'Extra profanity words')).toBe('4');
    expect(diagnosticRow(container, 'Sync snoozed')).toBe('Yes');
  });

  it('marks every new row unknown when an older payload omits its metadata', async () => {
    const snapshot = snapshotWith(false, null);
    Reflect.deleteProperty(snapshot.os, 'os_version');
    Reflect.deleteProperty(snapshot.os, 'install_flavor');
    for (const field of [
      'respect_manual_status',
      'gate_when_out_of_office',
      'profanity_extra_words_count',
      'locale',
      'update_channel',
      'snoozed'
    ]) {
      Reflect.deleteProperty(snapshot.config, field);
    }

    const { container } = await mountWith(snapshot);

    expect(diagnosticRow(container, 'OS')).toBe(
      'linux (x86_64, unix); Release: unknown; Install flavor: unknown'
    );
    expect(diagnosticRow(container, 'Respect manual status')).toBe('unknown');
    expect(diagnosticRow(container, 'Gate while out of office')).toBe('unknown');
    expect(diagnosticRow(container, 'Extra profanity words')).toBe('unknown');
    expect(diagnosticRow(container, 'Locale')).toBe('unknown');
    expect(diagnosticRow(container, 'Update channel')).toBe('unknown');
    expect(diagnosticRow(container, 'Sync snoozed')).toBe('unknown');
  });

  it('keeps the documented English locale default for an explicit null', async () => {
    const snapshot = snapshotWith(false, null);
    snapshot.config.locale = null;

    const { container } = await mountWith(snapshot);

    expect(diagnosticRow(container, 'Locale')).toBe('en');
  });
});

describe('Diagnostics Rust-owned save (#921)', () => {
  it('requests a save without sending JSON or a destination', async () => {
    const snapshot = snapshotWith(false, null);
    const { getByRole } = await mountWith(snapshot);
    invoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'get_diagnostics_snapshot') return snapshot;
      if (cmd === 'save_diagnostics_snapshot') return '/home/test/Downloads/report.json';
      return undefined;
    });

    await fireEvent.click(getByRole('button', { name: 'Save to file' }));

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith('save_diagnostics_snapshot')
    );
    expect(invoke).not.toHaveBeenCalledWith(
      'save_diagnostics_snapshot',
      expect.anything()
    );
    await waitFor(() =>
      expect(getByRole('status').textContent).toContain(
        'Diagnostics saved to your downloads folder.'
      )
    );
  });

  it('shows a save failure while retaining the in-memory snapshot', async () => {
    const snapshot = snapshotWith(false, null);
    const { container, getByRole } = await mountWith(snapshot);
    invoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'get_diagnostics_snapshot') return snapshot;
      if (cmd === 'save_diagnostics_snapshot') throw new Error('disk full');
      return undefined;
    });

    await fireEvent.click(getByRole('button', { name: 'Save to file' }));

    await waitFor(() =>
      expect(getByRole('status').textContent).toContain(
        'Save failed — use "Copy diagnostics" instead.'
      )
    );
    expect(container.querySelector('.content dl')).not.toBeNull();
    expect(container.textContent).toContain('4.6.0');
    expect(
      (getByRole('button', { name: 'Save to file' }) as HTMLButtonElement).disabled
    ).toBe(false);
  });
});
