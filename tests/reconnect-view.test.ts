/**
 * #557 / #558 — the Reconnect view's two card contracts, exercised through the
 * real component (the pattern set by tests/logviewer.test.ts for #492).
 *
 * - #557: an already-connected Teams must read as connected on *every* mount,
 *   not only in the mount that completed the reconnect. `authFlow.teams.phase`
 *   is module-level and survives navigation, while `teamsReconnectedThisSession`
 *   is per-mount — so the body used to fall through to the "click below to
 *   reconnect" branch while the badge two lines above said "Connected".
 * - #558: a Spotify flow stuck in `waiting` (browser tab abandoned) must offer
 *   a way out — a restart button and the manual redirect-URL paste — instead of
 *   the dead end that auto-start deliberately never recovers from.
 *
 * Both fail pre-fix and pass post-fix.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, cleanup, waitFor, fireEvent } from '@testing-library/svelte';

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));

vi.mock('@tauri-apps/api/core', () => ({ invoke }));
vi.mock('@tauri-apps/api/event', () => ({
  // Mirror the real signature: listen(eventName, handler) → unlisten.
  listen: vi.fn(async () => () => {})
}));

// Static imports: the vi.mock calls above hoist, so the Tauri mocks are in
// place before the component and its stores load.
import Reconnect from '$lib/components/Reconnect.svelte';
import { defaultConfig } from '$lib/stores/config';
import { resetAuthFlow, setSpotifyPhase, setTeamsPhase } from '$lib/stores/authFlow.svelte';
import { currentView } from '$lib/stores/app';
import { t } from '$lib/i18n';

const CLIENT_ID = 'a'.repeat(32);
const SYNC_CONNECTED = {
  is_syncing: false,
  current_track: null,
  spotify_connected: true,
  teams_connected: true
};

function mockBackend() {
  invoke.mockImplementation(async (cmd: string) => {
    switch (cmd) {
      case 'load_config':
        // #560: the keychain answer now travels with the config, so the view
        // reads it from there instead of probing the bool-only command. The
        // `is_spotify_client_secret_set` case is gone on purpose: if a future
        // change reintroduces that probe, it must answer `undefined` here and
        // fail these tests rather than quietly pass.
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
        return { ...SYNC_CONNECTED };
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
  // Unmount each render: jsdom's document is shared per file, so a leftover
  // view would satisfy the next test's text queries.
  cleanup();
});

describe('Reconnect view (#557, #558)', () => {
  it('#557 renders an already-connected Teams as connected on a fresh mount', async () => {
    // A reconnect completed earlier in this session: the module-level phase is
    // still 'done' after navigating away, but the per-mount "just reconnected"
    // flag was reset by the remount.
    setTeamsPhase('done');

    const { container } = render(Reconnect);
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('get_sync_status'));
    await Promise.resolve();

    expect(container.textContent).not.toContain('Reconnect Teams');
    expect(container.textContent).not.toContain(
      'Click below to reconnect your Microsoft Teams account.'
    );
    expect(container.textContent).toContain('Connected');
  });

  /**
   * #783 — the reconnect action is named by `settings.reconnectSpotify`, so the
   * assertion has to come from the rendered view: a source-text containment
   * check on the dictionary passes even when the view stops rendering the key
   * (the button then prints the raw key and `t()` no longer matches it).
   */
  it('#783 names the Spotify reconnect action from the dictionary', async () => {
    const { getByRole } = render(Reconnect);
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('get_sync_status'));

    expect(getByRole('button', { name: t('settings.reconnectSpotify') })).toBeTruthy();
  });

  it('#558 offers a restart (and the manual paste) when the Spotify flow is stuck waiting', async () => {
    setSpotifyPhase('waiting');

    const { container, getByRole } = render(Reconnect);
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('get_sync_status'));

    // The waiting branch is no longer a dead end.
    expect(container.querySelector('#spotify-manual-url')).not.toBeNull();
    const restart = getByRole('button', { name: 'Restart sign-in' });

    await fireEvent.click(restart);

    // Restarting clears the stuck phase (reconnectSpotify refuses to restart a
    // 'waiting' flow) and re-runs the flow through the keychain-backed command.
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith('start_spotify_reconnect', {
        clientId: CLIENT_ID,
        redirectUri: 'presencejam://callback'
      })
    );
  });

  it('#558 completes a stuck flow from a pasted redirect URL', async () => {
    setSpotifyPhase('waiting');

    const { container, getByRole } = render(Reconnect);
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('get_sync_status'));

    const input = container.querySelector('#spotify-manual-url') as HTMLInputElement;
    await fireEvent.input(input, {
      target: { value: 'presencejam://callback?code=the-code&state=the-state' }
    });
    await fireEvent.click(getByRole('button', { name: 'Submit code' }));

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith('complete_spotify_auth_manual', {
        code: 'the-code',
        oauthState: 'the-state'
      })
    );
    await waitFor(() =>
      expect(container.textContent).toContain('Spotify reconnected successfully.')
    );
  });

  it('#558 rejects a paste with no code instead of calling the backend', async () => {
    setSpotifyPhase('waiting');

    const { container, getByRole } = render(Reconnect);
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('get_sync_status'));

    const input = container.querySelector('#spotify-manual-url') as HTMLInputElement;
    await fireEvent.input(input, { target: { value: 'https://example.com/not-a-redirect' } });
    await fireEvent.click(getByRole('button', { name: 'Submit code' }));

    await waitFor(() =>
      expect(container.textContent).toContain('That URL has no sign-in code in it')
    );
    // No backend call: a paste without a code must not consume anything.
    expect(invoke.mock.calls.some(([cmd]) => cmd === 'complete_spotify_auth_manual')).toBe(false);
  });
});
