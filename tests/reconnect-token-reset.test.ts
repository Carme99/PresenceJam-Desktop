/**
 * #766 — the Reconnect corrupt-key reset affordance, through the real
 * component (the pattern set by tests/reconnect-view.test.ts).
 *
 * A corrupt stored encryption key (or a tokens.json that fails AES-GCM
 * authentication) lands in the auth error channels as English Rust-side
 * copy, and a plain reconnect cannot succeed against it. This pins the
 * half the user actually sees:
 *
 *   - an error carrying the corrupt-key marker raises the reset banner
 *     (naming the deleted storage is in the confirm, not the headline);
 *   - the reset is two-step: arm surfaces the confirm, confirm invokes
 *     `reset_local_token_storage` and reports the re-sign-in copy;
 *   - a non-corrupt error raises no reset banner at all.
 *
 * Fails pre-fix: with `src/` at the pre-#766 revision no error text raises
 * a reset affordance, so the marker-error assertion finds no button.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, cleanup, waitFor, fireEvent } from '@testing-library/svelte';

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));

vi.mock('@tauri-apps/api/core', () => ({ invoke }));
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async () => () => {})
}));

import Reconnect from '$lib/components/Reconnect.svelte';
import { defaultConfig } from '$lib/stores/config';
import { authFlow, resetAuthFlow, setSpotifyPhase } from '$lib/stores/authFlow.svelte';
import { currentView } from '$lib/stores/app';

const CLIENT_ID = 'a'.repeat(32);

function mockBackend() {
  invoke.mockImplementation(async (cmd: string) => {
    switch (cmd) {
      case 'load_config':
        return {
          ...structuredClone(defaultConfig),
          spotify: {
            ...defaultConfig.spotify,
            client_id: CLIENT_ID,
            client_secret_set: true,
            client_secret_state: 'present'
          }
        };
      case 'get_sync_status':
        return {
          is_syncing: false,
          current_track: null,
          spotify_connected: false,
          teams_connected: false
        };
      case 'reset_local_token_storage':
        return undefined;
      default:
        return undefined;
    }
  });
}

beforeEach(() => {
  invoke.mockReset();
  mockBackend();
  resetAuthFlow();
  currentView.set('dashboard');
});

afterEach(() => {
  cleanup();
});

describe('Reconnect corrupt-key reset (#766)', () => {
  it('raises the reset banner for a corrupt-key error', async () => {
    const { container, getByRole } = render(Reconnect);
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('get_sync_status'));
    // The autostarted OAuth flow sets phase to 'waiting' asynchronously after
    // mount; the corrupt-key error must land after that write or the autostart
    // clobbers it (real-world errors arrive via event, always post-mount).
    await waitFor(() => expect(authFlow.spotify.phase).toBe('waiting'), { timeout: 3000 });
    setSpotifyPhase(
      'error',
      'The stored tokens encryption key is unusable, so tokens.json cannot be decrypted.'
    );
    await waitFor(() => expect(container.textContent).toContain('cannot be read'));
    const arm = getByRole('button', { name: 'Reset local token storage' });
    await fireEvent.click(arm);
    await waitFor(() => expect(container.textContent).toContain('tokens.json'));
    expect(container.textContent).toContain('sign in again');
    // Arming alone never resets.
    expect(invoke.mock.calls.some(([cmd]) => cmd === 'reset_local_token_storage')).toBe(false);
  });

  it('resets behind the confirm and reports the re-sign-in copy', async () => {
    const { container } = render(Reconnect);
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('get_sync_status'));
    await waitFor(() => expect(authFlow.spotify.phase).toBe('waiting'), { timeout: 3000 });
    setSpotifyPhase(
      'error',
      'tokens file failed AES-GCM authentication (corrupt ciphertext, tampered file, or key mismatch — re-authentication required)'
    );
    let armButton: HTMLButtonElement | undefined;
    await waitFor(() => {
      armButton = [...container.querySelectorAll('button')].find(
        (b) => b.textContent === 'Reset local token storage'
      ) as HTMLButtonElement | undefined;
      expect(armButton).toBeTruthy();
    });
    await fireEvent.click(armButton!);
    await waitFor(() => expect(container.textContent).toContain('tokens.json'));
    const confirm = [...container.querySelectorAll('.info-box button')].find(
      (b) => b.textContent === 'Reset local token storage' && !b.disabled
    );
    expect(confirm).toBeTruthy();
    await fireEvent.click(confirm!);

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith('reset_local_token_storage')
    );
    await waitFor(() =>
      expect(container.textContent).toContain('Token storage reset')
    );
  });

  it('offers no reset for an ordinary auth error', async () => {
    const { container } = render(Reconnect);
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('get_sync_status'));
    await waitFor(() => expect(authFlow.spotify.phase).toBe('waiting'), { timeout: 3000 });
    setSpotifyPhase('error', 'Invalid grant: refresh token is expired');
    await waitFor(() => expect(container.textContent).toContain('Invalid grant'));
    expect(container.textContent).not.toContain('Reset local token storage');
  });
});
