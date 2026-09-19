/**
 * Issue #676 — the global-shortcut card in Settings, driven through the real
 * component. The Rust side owns the rules (parsing, the pair-wise conflict
 * check, the planner and the plugin call); these tests own what only the UI can
 * show: that the reason a combination was rejected is *named*, that a refused
 * registration is visible per row without taking the other binding down, and
 * that recording a combination does not fire it.
 *
 * Fails pre-fix: the card does not exist, and nothing seeds the status from a
 * registration pass.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, cleanup, fireEvent, waitFor } from '@testing-library/svelte';
import type { Mock } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/event', () => ({
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

import { invoke } from '@tauri-apps/api/core';
import Settings from '$lib/components/Settings.svelte';
import {
  configStore,
  defaultConfig,
  DEFAULT_SHORTCUTS,
  setShortcutBindings,
  shortcutBindingsOf,
  type ShortcutBindings
} from '$lib/stores/config';
import { currentView } from '$lib/stores/app';
import { resetSpotifyAuthFlow, resetTeamsAuthFlow } from '$lib/stores/authFlow.svelte';
import { presence, INITIAL_PRESENCE } from '$lib/stores/presence';
import { theme } from '$lib/stores/theme';
import { t } from '$lib/i18n';
// #784: one definition of the registration shapes. These are ts-rs output
// (re-exported from `$lib/types`), so the fixture cannot drift from the Rust
// struct the way a hand-copied local type could.
import type { ShortcutsStatus, SlotRegistration } from '$lib/types';

const invokeMock = invoke as unknown as Mock;

/** The bindings the backend has persisted — what registration reads from. */
let persisted: ShortcutBindings;
/** Accelerators this fake desktop refuses to grab, keyed to the reason. */
let refusals: Record<string, string>;
/** Accelerators the backend's validator rejects, keyed to the reason. */
let rejections: Record<string, string>;

/** A configured install with the given bindings. */
function configWith(bindings: ShortcutBindings) {
  const cfg = structuredClone(defaultConfig);
  cfg.spotify.client_id = 'test-client-id';
  return setShortcutBindings(cfg, { ...bindings });
}

/** The status a registration pass would report for the persisted bindings. */
function registrationStatus(): ShortcutsStatus {
  const slot = (accelerator: string | null): SlotRegistration => {
    if (accelerator === null) return { accelerator: null, registered: false, error: null };
    const error = refusals[accelerator] ?? null;
    return { accelerator, registered: error === null, error };
  };
  return {
    toggle_playback: slot(persisted.toggle_playback),
    toggle_sync: slot(persisted.toggle_sync)
  };
}

const field = (container: HTMLElement, slot: keyof ShortcutBindings) =>
  container.querySelector(`#shortcut-${slot}`) as HTMLInputElement;

const rowText = (container: HTMLElement, slot: keyof ShortcutBindings) =>
  field(container, slot).closest('.form-group')?.textContent ?? '';

const clearButton = (container: HTMLElement, slot: keyof ShortcutBindings) =>
  field(container, slot).closest('.shortcut-row')?.querySelector('button') as HTMLButtonElement;

const saveButton = (container: HTMLElement) =>
  [...container.querySelectorAll('button')].find(
    (b) => b.textContent?.trim() === t('settings.saveChanges')
  ) as HTMLButtonElement;

const commandsCalled = (cmd: string) =>
  invokeMock.mock.calls.filter((call) => call[0] === cmd).length;

/** Mount Settings and wait for `onMount` to adopt the loaded config. */
async function mountSettings() {
  const result = render(Settings, {});
  await waitFor(() => {
    expect(result.container.querySelector('.badge.success')).not.toBeNull();
  });
  return result;
}

beforeEach(() => {
  presence.set({ ...INITIAL_PRESENCE });
  currentView.set('dashboard');
  theme.set('dark');
  resetSpotifyAuthFlow();
  resetTeamsAuthFlow();

  persisted = { ...DEFAULT_SHORTCUTS };
  refusals = {};
  rejections = {};
  configStore.set(configWith(persisted));

  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
    switch (cmd) {
      case 'load_config':
        return configWith(persisted);
      case 'save_config':
        if (args !== null && typeof args === 'object' && 'config' in args) {
          persisted = shortcutBindingsOf(args.config as never);
          return args.config;
        }
        return undefined;
      case 'get_sync_status':
        return {
          is_syncing: false,
          current_track: null,
          spotify_connected: true,
          teams_connected: true
        };
      case 'get_spotify_granted_scopes':
        return ['user-modify-playback-state'];
      case 'get_teams_granted_scopes':
        return ['Presence.Read', 'profile'];
      case 'register_shortcuts':
        return registrationStatus();
      case 'unregister_shortcuts':
        return {
          toggle_playback: {
            accelerator: persisted.toggle_playback,
            registered: false,
            error: null
          },
          toggle_sync: { accelerator: persisted.toggle_sync, registered: false, error: null }
        };
      case 'validate_shortcut': {
        const accelerator = (args as { accelerator?: string } | undefined)?.accelerator ?? '';
        const reason = rejections[accelerator];
        if (reason) throw reason;
        return null;
      }
      default:
        return [];
    }
  });
});

afterEach(() => {
  cleanup();
});

describe('Settings — global shortcuts (#676)', () => {
  it('shows the configured bindings and reports them as active', async () => {
    const { container } = await mountSettings();

    await waitFor(() => {
      expect(field(container, 'toggle_playback').value).toBe('CmdOrCtrl+Alt+P');
    });
    expect(field(container, 'toggle_sync').value).toBe('CmdOrCtrl+Alt+S');
    await waitFor(() => {
      expect(rowText(container, 'toggle_playback')).toContain(t('settings.shortcutRegistered'));
    });
    expect(rowText(container, 'toggle_sync')).toContain(t('settings.shortcutRegistered'));
  });

  it('records the pressed combination, after asking the backend about it', async () => {
    const { container } = await mountSettings();
    const target = field(container, 'toggle_playback');

    await fireEvent.focus(target);
    await fireEvent.keyDown(target, { key: 'K', code: 'KeyK', ctrlKey: true, shiftKey: true });

    await waitFor(() => {
      expect(field(container, 'toggle_playback').value).toBe('CmdOrCtrl+Shift+K');
    });
    // Validated before it can be saved, naming the slot it is bound to *and*
    // the other row's pending value — the pair is what a conflict is judged
    // against, so dropping `other` here would silently disable that check
    // (Tauri ignores arguments the command does not declare).
    expect(invokeMock).toHaveBeenCalledWith('validate_shortcut', {
      accelerator: 'CmdOrCtrl+Shift+K',
      action: 'toggle_playback',
      other: 'CmdOrCtrl+Alt+S'
    });
  });

  /**
   * Clearing one row and moving its accelerator onto the other row is a single
   * unsaved edit, and the whole point of the clear is that the accelerator is
   * free. The card therefore sends a cleared row as an explicit empty string
   * (not `null`, which is indistinguishable from "says nothing"), and the save
   * that follows must go through.
   */
  it('sends a cleared row as an explicit blank so an in-flight swap is allowed', async () => {
    const { container } = await mountSettings();

    await fireEvent.click(clearButton(container, 'toggle_sync'));
    await waitFor(() => {
      expect(field(container, 'toggle_sync').value).toBe('');
    });

    const target = field(container, 'toggle_playback');
    await fireEvent.focus(target);
    await fireEvent.keyDown(target, { key: 'S', code: 'KeyS', ctrlKey: true, altKey: true });

    await waitFor(() => {
      expect(field(container, 'toggle_playback').value).toBe('CmdOrCtrl+Alt+S');
    });
    expect(invokeMock).toHaveBeenCalledWith('validate_shortcut', {
      accelerator: 'CmdOrCtrl+Alt+S',
      action: 'toggle_playback',
      other: ''
    });

    await fireEvent.click(saveButton(container));
    await waitFor(() => expect(commandsCalled('save_config')).toBe(1));
    expect(persisted.toggle_playback).toBe('CmdOrCtrl+Alt+S');
    expect(persisted.toggle_sync).toBeNull();
  });


  it('ignores a modifier-only press and a key it cannot name', async () => {
    const { container } = await mountSettings();
    const target = field(container, 'toggle_playback');

    await fireEvent.focus(target);
    await fireEvent.keyDown(target, { key: 'Shift', code: 'ShiftLeft', shiftKey: true });
    await fireEvent.keyDown(target, { key: 'IntlBackslash', code: 'IntlBackslash' });

    expect(field(container, 'toggle_playback').value).toBe('CmdOrCtrl+Alt+P');
  });

  /**
   * The bug this prevents: a live grab swallows the key at the OS level, so
   * re-recording a binding that is currently registered would fire its action
   * instead of being captured. The card releases the grabs while a field
   * records, and restores them when it stops.
   */
  it('releases the grabs while a field records and re-registers on blur', async () => {
    const { container } = await mountSettings();
    const target = field(container, 'toggle_playback');
    const before = commandsCalled('register_shortcuts');

    await fireEvent.focus(target);
    await waitFor(() => expect(commandsCalled('unregister_shortcuts')).toBe(1));

    await fireEvent.blur(target);
    await waitFor(() => expect(commandsCalled('register_shortcuts')).toBeGreaterThan(before));
  });

  it('names the reason a combination cannot be used and refuses to save it', async () => {
    const reason = 'Conflicts with the toggle_sync shortcut ("CmdOrCtrl+Alt+S")';
    rejections['CmdOrCtrl+Shift+K'] = reason;
    const { container } = await mountSettings();
    const target = field(container, 'toggle_playback');

    await fireEvent.focus(target);
    await fireEvent.keyDown(target, { key: 'K', code: 'KeyK', ctrlKey: true, shiftKey: true });

    await waitFor(() => {
      expect(rowText(container, 'toggle_playback')).toContain(reason);
    });

    await fireEvent.click(saveButton(container));
    // The rejected combination must not reach the backend's save path.
    expect(commandsCalled('save_config')).toBe(0);
  });

  /**
   * A desktop that refuses the grab (Wayland, or a combination another
   * application owns). The row must say so, and the other binding — and the
   * rest of the pane — must keep working.
   */
  it('shows a refused registration without taking the other binding down', async () => {
    refusals['CmdOrCtrl+Alt+P'] = 'accelerator CmdOrCtrl+Alt+P already in use';
    const { container } = await mountSettings();

    await waitFor(() => {
      expect(rowText(container, 'toggle_playback')).toContain(
        t('settings.shortcutRegistrationFailed', {
          reason: 'accelerator CmdOrCtrl+Alt+P already in use'
        })
      );
    });
    expect(rowText(container, 'toggle_sync')).toContain(t('settings.shortcutRegistered'));
    // The rest of the pane is unaffected by the refusal.
    expect(field(container, 'toggle_sync').value).toBe('CmdOrCtrl+Alt+S');
    expect(saveButton(container)).toBeTruthy();
  });

  it('names an unparsable stored binding at mount', async () => {
    persisted = { toggle_playback: 'NotAKey', toggle_sync: 'CmdOrCtrl+Alt+S' };
    rejections['NotAKey'] = '"NotAKey" is not a recognised shortcut (UnsupportedKey)';

    const { container } = await mountSettings();

    await waitFor(() => {
      expect(rowText(container, 'toggle_playback')).toContain('not a recognised shortcut');
    });
    // The raw stored value is shown, so the user can see what to replace.
    expect(field(container, 'toggle_playback').value).toBe('NotAKey');
  });

  it('clears one binding without touching the other', async () => {
    const { container } = await mountSettings();

    await fireEvent.click(clearButton(container, 'toggle_playback'));
    await waitFor(() => {
      expect(field(container, 'toggle_playback').value).toBe('');
    });

    await fireEvent.click(saveButton(container));
    await waitFor(() => expect(commandsCalled('save_config')).toBe(1));
    expect(persisted.toggle_playback).toBeNull();
    expect(persisted.toggle_sync).toBe('CmdOrCtrl+Alt+S');
  });

  it('registers the persisted bindings after a save', async () => {
    const { container } = await mountSettings();
    const target = field(container, 'toggle_playback');
    const before = commandsCalled('register_shortcuts');

    await fireEvent.focus(target);
    await fireEvent.keyDown(target, { key: 'K', code: 'KeyK', ctrlKey: true, shiftKey: true });
    await fireEvent.blur(target);
    await fireEvent.click(saveButton(container));

    await waitFor(() => expect(commandsCalled('save_config')).toBe(1));
    // The save carried the new binding, and registration re-ran from the config
    // the backend just persisted — the acceptance's "the registered
    // accelerators match the config after a change".
    expect(persisted.toggle_playback).toBe('CmdOrCtrl+Shift+K');
    await waitFor(() => expect(commandsCalled('register_shortcuts')).toBeGreaterThan(before + 1));
    await waitFor(() => {
      expect(field(container, 'toggle_playback').value).toBe('CmdOrCtrl+Shift+K');
    });
  });
});
