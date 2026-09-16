/**
 * #560 — the OS keychain's *tri-state* as the UI renders it.
 *
 * The bug this defends: `has_spotify_client_secret()` is a `Present`-only
 * projection, so a locked/missing Secret Service answered `false` — the same
 * answer as "the user never configured a secret". Every gate that read it
 * (Settings' credential row, the Reconnect card) then told a fully
 * credentialed user that nothing was configured and offered to set Spotify up
 * again, while their secret was still stored and merely unreadable.
 *
 * The fix surfaces Rust's `ClientSecretState` (`present`/`absent`/
 * `unavailable`) through the config the frontend already loads, and both views
 * branch on it. These tests render the two states that matter — `unavailable`
 * (the keychain could not answer) and `absent` (it positively has nothing) —
 * through the real components and assert they produce opposite advice.
 *
 * Fails pre-fix: both states render the identical "not configured / missing
 * credentials" copy.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, cleanup, waitFor } from '@testing-library/svelte';

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));

vi.mock('@tauri-apps/api/core', () => ({ invoke }));
vi.mock('@tauri-apps/api/event', () => ({
  // Mirror the real signature: listen(eventName, handler) → unlisten.
  listen: vi.fn(async () => () => {}),
  emitTo: vi.fn(async () => {})
}));
vi.mock('@tauri-apps/plugin-notification', () => ({
  isPermissionGranted: vi.fn(async () => true),
  requestPermission: vi.fn(async () => 'granted')
}));
vi.mock('$lib/stores/detach', () => ({
  popOut: vi.fn(async () => {}),
  popIn: vi.fn(async () => {}),
  focusDetached: vi.fn(async () => {})
}));

import Reconnect from '$lib/components/Reconnect.svelte';
import Settings from '$lib/components/Settings.svelte';
import {
  clientSecretStateOf,
  configStore,
  defaultConfig,
  mergeWizardConfig
} from '$lib/stores/config';
import { resetAuthFlow } from '$lib/stores/authFlow.svelte';
import { currentView } from '$lib/stores/app';
import type { AppConfig } from '$lib/types';

const CLIENT_ID = 'a'.repeat(32);
const SYNC_CONNECTED = {
  is_syncing: false,
  current_track: null,
  spotify_connected: true,
  teams_connected: true
};

/**
 * The keychain answer the mock backend reports, as Rust serializes it. The
 * field is written as a loose value so a checkout whose generated bindings do
 * not carry it yet still type-checks — the component reads it through
 * `clientSecretStateOf`, which is what makes that safe.
 */
type SecretStateWire = 'present' | 'absent' | 'unavailable';

function configWith(state: SecretStateWire): AppConfig {
  const cfg = structuredClone(defaultConfig) as AppConfig & {
    spotify: { client_secret_state?: string };
  };
  cfg.spotify.client_id = CLIENT_ID;
  cfg.spotify.client_secret_set = state === 'present';
  cfg.spotify.client_secret_state = state;
  return cfg;
}

function mockBackend(state: SecretStateWire) {
  invoke.mockImplementation(async (cmd: string) => {
    switch (cmd) {
      case 'load_config':
        return configWith(state);
      case 'get_sync_status':
        return { ...SYNC_CONNECTED };
      case 'get_spotify_granted_scopes':
        return ['user-modify-playback-state'];
      case 'get_teams_granted_scopes':
        return ['Presence.Read', 'profile'];
      // `is_spotify_client_secret_set` is deliberately NOT mocked: neither view
      // may fall back to the bool-only probe for this decision (#560).
      default:
        return undefined;
    }
  });
}

beforeEach(() => {
  invoke.mockReset();
  resetAuthFlow();
  currentView.set('dashboard');
});

afterEach(() => {
  cleanup();
});

describe('config tri-state accessor (#560)', () => {
  it('passes the wire spelling through unchanged', () => {
    expect(clientSecretStateOf(configWith('unavailable'))).toBe('unavailable');
    expect(clientSecretStateOf(configWith('absent'))).toBe('absent');
    expect(clientSecretStateOf(configWith('present'))).toBe('present');
  });

  it('falls back to the legacy flag when the field is missing entirely', () => {
    // The payload a build without the field produces — which is exactly the
    // shape under test, so it is asserted in from a loose literal rather than
    // built by deleting a field the declared type requires.
    const missingFieldWire = {
      spotify: {
        client_id: CLIENT_ID,
        client_secret_set: true,
        redirect_uri: 'presencejam://callback'
      }
    } as unknown as AppConfig;

    // `client_secret_set` is the only evidence left. It must not be read as
    // "unavailable" (that would accuse a healthy keychain) and must not be
    // read as "absent" (that would offer setup for a stored secret).
    expect(clientSecretStateOf(missingFieldWire)).toBe('present');
  });

  it('is "not absent" for every state that still holds a credential', () => {
    // The predicate every routing gate uses (`clientSecretStateOf(cfg) !==
    // 'absent'`; see +page.svelte's boot probe, +layout.svelte's
    // `spotify-reconnect-required` handler and Dashboard's `goToSetup`): an
    // unavailable keychain still *has* the secret, so it must not be treated
    // as a missing credential. Only a positively absent entry justifies the
    // setup wizard.

    expect(clientSecretStateOf(configWith('present')) !== 'absent').toBe(true);
    expect(clientSecretStateOf(configWith('unavailable')) !== 'absent').toBe(true);
    expect(clientSecretStateOf(configWith('absent')) !== 'absent').toBe(false);
  });
});

describe('Reconnect view (#560)', () => {
  it('renders the keychain state, not "missing credentials", when the keychain cannot answer', async () => {
    mockBackend('unavailable');
    const { container } = render(Reconnect);
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('get_sync_status'));
    await Promise.resolve();

    expect(container.textContent).toContain('Keychain unavailable');
    expect(container.textContent).toContain('could not read your saved Spotify Client Secret');
    expect(container.textContent).not.toContain('Missing credentials');
    expect(container.textContent).not.toContain('Go to full setup');
    // No reconnect button either: the flow reads the secret from the keychain
    // that just failed, so offering it would only fail in the browser.
    expect(container.textContent).not.toContain('Reconnect Spotify');
    // And the view must not have launched it on mount.
    expect(invoke).not.toHaveBeenCalledWith('start_spotify_reconnect', expect.anything());
  });

  it('still sends the user to full setup when the credential is genuinely absent', async () => {
    mockBackend('absent');
    const { container } = render(Reconnect);
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('get_sync_status'));
    await Promise.resolve();

    expect(container.textContent).toContain('Missing credentials');
    expect(container.textContent).toContain('Go to full setup');
    expect(container.textContent).not.toContain('Keychain unavailable');
  });

  it('offers the reconnect button when the keychain reports the secret present', async () => {
    mockBackend('present');
    const { container } = render(Reconnect);
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('get_sync_status'));
    await Promise.resolve();

    expect(container.textContent).toContain('Reconnect Spotify');
    expect(container.textContent).not.toContain('Missing credentials');
    expect(container.textContent).not.toContain('Keychain unavailable');
  });
});

describe('Settings credential row (#560)', () => {
  function mountSettings() {
    const result = render(Settings);
    return waitFor(() => {
      expect(result.container.querySelector('.badge.success')).not.toBeNull();
    }).then(() => result);
  }

  it('explains a locked keychain instead of claiming nothing is configured', async () => {
    mockBackend('unavailable');
    const { container } = await mountSettings();

    expect(container.textContent).toContain('System keychain unavailable');
    expect(container.textContent).not.toContain('Not configured.');
    expect(container.textContent).not.toContain('Run Onboarding');
  });

  it('still offers onboarding when the secret really is missing', async () => {
    mockBackend('absent');
    const { container } = await mountSettings();

    expect(container.textContent).toContain('Not configured.');
    expect(container.textContent).toContain('Run Onboarding');
    expect(container.textContent).not.toContain('System keychain unavailable');
  });

  it('reports a stored secret as stored', async () => {
    mockBackend('present');
    const { container } = await mountSettings();

    expect(container.textContent).toContain("Stored securely in your operating system's keychain");
    expect(container.textContent).not.toContain('Not configured.');
    expect(container.textContent).not.toContain('System keychain unavailable');
  });
});

describe('wizard merge keeps the derived keychain pair consistent (#560)', () => {
  it('reports the freshly stored secret as present, not absent', () => {
    // The set-up wizard stores the secret in the OS keychain, then hands the
    // merged config straight to the store. `clientSecretStateOf` prioritises
    // the state, so if only `client_secret_set` were written the pair would
    // disagree and Settings' credential row would read "Not configured" until
    // the next `load_config`.
    const merged = mergeWizardConfig(configWith('absent'), {
      spotify_client_id: CLIENT_ID,
      status_format: '🎵 {artist} - {track} 🎧',
      default_interval_seconds: BigInt(30),
      autostart: false
    });

    expect(merged.spotify.client_secret_set).toBe(true);
    expect(merged.spotify.client_secret_state).toBe('present');
    expect(clientSecretStateOf(merged)).toBe('present');
  });
});

// `configStore` is a module-level store shared across the file; reset it so a
// later test cannot inherit an earlier test's config through a component that
// renders before its `loadConfig` resolves.
afterEach(() => {
  configStore.set(structuredClone(defaultConfig));
});
