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
import { get } from 'svelte/store';
import { tick } from 'svelte';
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
import type { ShortcutReason } from '$lib/types';

const invokeMock = invoke as unknown as Mock;

/** One slot as the backend reports it (mirrors the Rust `SlotRegistration`). */
type SlotStatus = { accelerator: string | null; registered: boolean; error: string | null };
type Status = { toggle_playback: SlotStatus; toggle_sync: SlotStatus };

/** The bindings the backend has persisted — what registration reads from. */
let persisted: ShortcutBindings;
/** Accelerators this fake desktop refuses to grab, keyed to the reason
 * string the plugin answered with. Rust wraps the plugin text as
 * `ShortcutReason::Unknown { message }` (issue #968), so the harness still
 * produces a free-form string here and lets the wrap happen Rust-side;
 * the rejected slot will surface as `Unknown { message }`.
 */
let refusals: Record<string, string>;
/** Accelerators the backend's validator rejects, keyed to the typed
 * `ShortcutReason` Rust throws (issue #968). The harness emits the same
 * tagged-enum JSON the Rust command serializes, so the frontend's
 * `normalizeReason` parses it back to a typed reason.
 */
let rejections: Record<string, ShortcutReason>;

/** A configured install with the given bindings. */
function configWith(bindings: ShortcutBindings) {
  const cfg = structuredClone(defaultConfig);
  cfg.spotify.client_id = 'test-client-id';
  return setShortcutBindings(cfg, { ...bindings });
}

/** The status a registration pass would report for the persisted bindings. */
function registrationStatus(): Status {
  const slot = (accelerator: string | null): SlotStatus => {
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

  /**
   * Issue #968: the rejection is a typed `Conflict { other_slot }` from
   * Rust, not a free-form string the validator built in English. The card
   * maps it to the localized `settings.shortcutReasonConflict` template;
   * the test asserts the *template* (not the raw English the old code
   * spliced into German / French copy), and pins the save path that refused
   * the combo so the test cannot be made green by a card that swallowed
   * the rejection.
   */
  it('names the reason a combination cannot be used and refuses to save it', async () => {
    rejections['CmdOrCtrl+Shift+K'] = { kind: 'Conflict', other_slot: 'toggle_sync' };
    const { container } = await mountSettings();
    const target = field(container, 'toggle_playback');

    await fireEvent.focus(target);
    await fireEvent.keyDown(target, { key: 'K', code: 'KeyK', ctrlKey: true, shiftKey: true });

    await waitFor(() => {
      expect(rowText(container, 'toggle_playback')).toContain(
        t('settings.shortcutReasonConflict', { other: 'toggle_sync' })
      );
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

  /**
   * Issue #968: the planner surfaces a stored-but-unparseable binding
   * through the typed `NotAKey { accelerator }` reason (the same code path
   * the live validator uses). The Settings card mounts by re-validating
   * every slot through the `validate_shortcut` IPC, so the typed reason
   * reaches the row on mount and the card renders the localized
   * `settings.shortcutReasonNotAKey` template (which interpolates the
   * offending accelerator verbatim). A regression here would render the
   * raw stored string instead of the localized template.
   */
  it('names an unparsable stored binding at mount through the localized key', async () => {
    persisted = { toggle_playback: 'NotAKey', toggle_sync: 'CmdOrCtrl+Alt+S' };
    // The Settings card's onMount re-validates each slot via
    // `validate_shortcut`, which the harness answers from the typed
    // `rejections` map. Setting the typed reason here is what the
    // production Settings card observes.
    rejections['NotAKey'] = { kind: 'NotAKey', accelerator: 'NotAKey' };

    const { container } = await mountSettings();

    await waitFor(() => {
      expect(rowText(container, 'toggle_playback')).toContain(
        t('settings.shortcutReasonNotAKey', { accelerator: 'NotAKey' })
      );
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

  /**
   * Issue #968 (acceptance #1): "a rejected accelerator renders fully
   * localized copy in every shipped locale". The validator returns a typed
   * `NotAKey { accelerator }` from Rust; the Settings card routes it
   * through `shortcutReasonLabel` so a German card shows the German
   * `settings.shortcutReasonNotAKey` template, not `"foo bar baz" is not a
   * recognised shortcut`. The offensive accelerator is rendered via the
   * `{accelerator}` placeholder, so the substituted text reaches the
   * screen and a regression to the raw text breaks the assertion.
   */
  it('renders a NotAKey rejection through the localized key, not the raw accelerator (#968)', async () => {
    // Pre-seed the typed reason the harness's `validate_shortcut` IPC
    // throws. The accelerator the field records (Ctrl+Alt+X) is keyed in
    // the rejection map, so the live IPC throws the typed reason and the
    // row renders the localized label.
    rejections['CmdOrCtrl+Alt+X'] = { kind: 'NotAKey', accelerator: 'CmdOrCtrl+Alt+X' };
    const { container } = await mountSettings();
    const target = field(container, 'toggle_playback');

    // Focus + keypress drives `setShortcutBinding`, which calls
    // `validateShortcut` and stores the typed reason the IPC throws.
    await fireEvent.focus(target);
    await fireEvent.keyDown(target, {
      key: 'X',
      code: 'KeyX',
      ctrlKey: true,
      altKey: true
    });
    await tick();

    await waitFor(() => {
      const row = rowText(container, 'toggle_playback');
      const localized = t('settings.shortcutReasonNotAKey', {
        accelerator: 'CmdOrCtrl+Alt+X'
      });
      // The localized template (including the `{accelerator}` interpolation)
      // reaches the row.
      expect(row).toContain(localized);
      expect(row).toContain('CmdOrCtrl+Alt+X');
      // The legacy English copy must not appear spliced into a German /
      // French card.
      expect(row).not.toContain('is not a recognised shortcut');
      expect(row).not.toContain('(NotAKey)');
    });

    await fireEvent.click(saveButton(container));
    // The rejected combination must not reach the backend's save path.
    expect(commandsCalled('save_config')).toBe(0);
  });

  /**
   * Issue #968 (acceptance #2): "unrecognised reasons still fall back to
   * the backend text rather than rendering a raw code". A slot whose
   * error is something `normalizeReason` does not recognise (e.g. a future
   * variant Rust adds before the frontend ships the matching template) is
   * downgraded to `Unknown { message }`, which the Settings card renders
   * through `settings.shortcutReasonUnknown` — not as a JSON object dump
   * like `{"kind":"Future"}`.
   */
  it('falls back to Unknown for a reason the frontend does not recognise (#968)', async () => {
    // Capture the default impl so the mount's onMount chain still
    // resolves (load_config, get_sync_status, …) — only the
    // `register_shortcuts` response is replaced with a payload whose
    // `kind` is not in the typed enum.
    const defaultImpl = invokeMock.getMockImplementation();
    invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === 'register_shortcuts') {
        return {
          toggle_playback: {
            accelerator: 'CmdOrCtrl+Alt+P',
            registered: false,
            error: { kind: 'Future', payload: 42 }
          },
          toggle_sync: {
            accelerator: 'CmdOrCtrl+Alt+S',
            registered: true,
            error: null
          }
        };
      }
      if (defaultImpl) return defaultImpl(cmd, args);
      return [];
    });
    const { container } = await mountSettings();

    await waitFor(() => {
      const row = rowText(container, 'toggle_playback');
      // The unrecognised shape is serialized through the unknown template —
      // a JSON object dump must not appear in the rendered card.
      expect(row).not.toContain('Future');
      expect(row).not.toContain('"payload"');
      expect(row).not.toContain('undefined');
      // The Unknown path renders something the user can act on, not an
      // empty card.
      expect(row.length).toBeGreaterThan(0);
    });
  });

  /**
   * Issue #968 (autostart acceptance): the autostart toggle is the other
   * place the Settings card used to splice a plugin/io error verbatim
   * into localized copy. With the typed reason, a denial surfaces through
   * `settings.shortcutReasonAutostart` and the original `cause` text is
   * the parameter, not the body of the localized wrapper.
   */
  it('renders an autostart rejection through the localized key with the cause (#968)', async () => {
    // Capture the default impl so the mount's onMount chain still
    // resolves (load_config, get_sync_status, …) — only
    // `set_autostart_enabled` throws the typed reason here.
    const defaultImpl = invokeMock.getMockImplementation();
    invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === 'set_autostart_enabled') {
        throw { kind: 'Autostart', cause: 'permission denied' };
      }
      if (defaultImpl) return defaultImpl(cmd, args);
      return [];
    });

    const { container } = await mountSettings();

    // The autostart toggle is the labeled `#autostart` checkbox. The
    // onchange handler invokes `set_autostart_enabled`, which throws the
    // typed reason here.
    const toggle = container.querySelector('#autostart') as HTMLInputElement | null;
    expect(toggle).not.toBeNull();
    // jsdom: fireEvent.click on a checkbox toggles `checked` and fires
    // both `click` and `change`; the production handler reads
    // `e.currentTarget.checked` on the change event.
    toggle!.checked = true;
    await fireEvent.change(toggle!);
    await tick();

    await waitFor(() => {
      const body = container.textContent ?? '';
      const localized = t('settings.shortcutReasonAutostart', {
        cause: 'permission denied'
      });
      expect(body).toContain(localized);
      // The `cause` placeholder must reach the screen.
      expect(body).toContain('permission denied');
      // The old prefix is gone — the wrapper is not the legacy English
      // copy.
      expect(body).not.toContain('Failed to update launch-at-login');
    });
  });

  /**
   * Issue #811: the card follows the reported OS state — a successful toggle
   * converges the shared store (via `load_config`) so a later whole-document
   * save (even a bare language change) carries the toggled flag instead of
   * silently reverting the OS entry; a rejection reverts the checkbox and
   * reports the typed reason without touching the store.
   */
  it('converges the store on a successful toggle and reverts on rejection (#811)', async () => {
    const defaultImpl = invokeMock.getMockImplementation();
    // The backend's `update_config` merges the patch and echoes the converged
    // document; the mock mirrors it by reading the patch's autostart flag
    // (pure — no store write here, the `updateConfig()` helper adopts the
    // return value into the store itself, so a component that never calls it
    // leaves the store stale and this test fails pre-fix).
    let patched: boolean | null = null;
    invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === 'set_autostart_enabled') return undefined;
      if (cmd === 'update_config') {
        const patch = (args as { patch?: { autostart?: boolean } } | undefined)?.patch;
        patched = patch?.autostart ?? null;
        const converged = structuredClone(get(configStore));
        if (patched !== null) converged.autostart = patched;
        return converged;
      }
      if (defaultImpl) return defaultImpl(cmd, args);
      return [];
    });
    const { container } = await mountSettings();
    const toggle = container.querySelector('#autostart') as HTMLInputElement | null;
    expect(toggle).not.toBeNull();
    expect(toggle!.checked).toBe(false);

    toggle!.checked = true;
    await fireEvent.change(toggle!);
    await tick();

    await waitFor(() => {
      expect(patched).toBe(true);
    });
    await waitFor(() => {
      expect(get(configStore).autostart).toBe(true);
    });

    // A rejection reverts the checkbox, not the store, and surfaces the
    // typed reason.
    invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === 'set_autostart_enabled') {
        throw { kind: 'Autostart', cause: 'permission denied' };
      }
      if (defaultImpl) return defaultImpl(cmd, args);
      return [];
    });

    toggle!.checked = false;
    await fireEvent.change(toggle!);
    await tick();

    await waitFor(() => {
      const body = container.textContent ?? '';
      expect(body).toContain('permission denied');
    });
    expect(toggle!.checked).toBe(true);
  });
});
