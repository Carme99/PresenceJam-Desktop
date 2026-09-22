/**
 * #766 — the Diagnostics token-storage reset affordance, through the real
 * component (the pattern set by tests/diagnostics-quarantine.test.ts).
 *
 * The stored token encryption key can go corrupt (or tokens.json can fail
 * AES-GCM authentication), and before this slice the UI offered no way out:
 * the keychain error text names a reset, but no command existed and no view
 * invoked one. This pins the half the user actually sees:
 *
 *   - the connections card offers the reset behind an arm step — a single
 *     click surfaces the confirmation naming what is deleted, never the
 *     destructive invoke;
 *   - confirming invokes `reset_local_token_storage`, reports the re-sign-in
 *     copy on success, and reloads the snapshot so the keychain rows read
 *     the emptied state;
 *   - a rejected invoke reports the failure without reloading (the snapshot
 *     still describes the un-reset storage).
 *
 * Fails pre-fix: with `src/` at the pre-#766 revision there is no reset
 * button at all, so the first assertion finds nothing to click.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, cleanup, waitFor, fireEvent } from '@testing-library/svelte';

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));

vi.mock('@tauri-apps/api/core', () => ({ invoke }));

import Diagnostics from '$lib/components/Diagnostics.svelte';
import { currentView } from '$lib/stores/app';
import type { DiagnosticsSnapshot } from '$lib/types';

function snapshotWith(): DiagnosticsSnapshot {
  return {
    app_version: '4.6.0',
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
      config_quarantine_backup: null
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
    keychain: { spotify_client_secret_present: false, tokens_encryption_key_present: true },
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

beforeEach(() => {
  invoke.mockReset();
  currentView.set('dashboard');
});

afterEach(() => {
  cleanup();
});

describe('Diagnostics token-storage reset (#766)', () => {
  it('offers the reset behind a confirmation naming what is deleted', async () => {
    invoke.mockImplementation(async (cmd: string) =>
      cmd === 'get_diagnostics_snapshot' ? snapshotWith() : undefined
    );
    const { container, getByRole } = render(Diagnostics);
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('get_diagnostics_snapshot'));
    await waitFor(() => expect(container.querySelector('.content dl')).not.toBeNull());

    // One click arms, never resets: the confirm names the deleted storage.
    await fireEvent.click(getByRole('button', { name: 'Reset local token storage' }));
    await waitFor(() =>
      expect(container.textContent).toContain('tokens.json')
    );
    expect(container.textContent).toContain('sign in again');
    expect(invoke.mock.calls.map((c) => c[0])).toEqual(['get_diagnostics_snapshot']);
  });

  it('resets, reports the re-sign-in copy, and reloads the snapshot', async () => {
    const emptied = snapshotWith();
    emptied.keychain = {
      spotify_client_secret_present: false,
      tokens_encryption_key_present: false
    } as DiagnosticsSnapshot['keychain'];
    invoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'reset_local_token_storage') return undefined;
      if (cmd === 'get_diagnostics_snapshot') {
        return invoke.mock.calls.some(([c]) => c === 'reset_local_token_storage')
          ? emptied
          : snapshotWith();
      }
      return undefined;
    });
    const { container, getByRole } = render(Diagnostics);
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('get_diagnostics_snapshot'));
    await waitFor(() => expect(container.querySelector('.content dl')).not.toBeNull());

    await fireEvent.click(getByRole('button', { name: 'Reset local token storage' }));
    // The armed confirm repeats the action name: arm button + confirm button
    // share it, with Dismiss beside them.
    await waitFor(() =>
      expect(container.querySelectorAll('.reset-confirm button').length).toBe(2)
    );
    const confirms = container.querySelectorAll('.reset-confirm button');
    await fireEvent.click(confirms[0]);

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith('reset_local_token_storage')
    );
    await waitFor(() =>
      expect(container.textContent).toContain('Token storage reset')
    );
    // The snapshot reload is what makes the keychain rows truthful.
    await waitFor(() =>
      expect(invoke.mock.calls.filter(([c]) => c === 'get_diagnostics_snapshot').length).toBe(2)
    );
  });

  it('reports a rejected reset without reloading the snapshot', async () => {
    invoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'reset_local_token_storage') throw new Error('keychain locked');
      return cmd === 'get_diagnostics_snapshot' ? snapshotWith() : undefined;
    });
    const { container, getByRole } = render(Diagnostics);
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('get_diagnostics_snapshot'));
    await waitFor(() => expect(container.querySelector('.content dl')).not.toBeNull());

    await fireEvent.click(getByRole('button', { name: 'Reset local token storage' }));
    await waitFor(() =>
      expect(container.querySelectorAll('.reset-confirm button').length).toBe(2)
    );
    await fireEvent.click(container.querySelectorAll('.reset-confirm button')[0]);

    await waitFor(() =>
      expect(container.textContent).toContain('Reset failed')
    );
    expect(
      invoke.mock.calls.filter(([c]) => c === 'get_diagnostics_snapshot').length
    ).toBe(1);
  });
});
