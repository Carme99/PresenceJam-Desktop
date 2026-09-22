/**
 * #735 — the device-code countdown must not be a live region, and
 * #952 — Reconnect must render the shared device-code block.
 *
 * The code stays valid for ~15 minutes and the countdown is recomputed once a
 * second for that whole window, so a polite live region on it queued a fresh
 * "Code expires in m:ss" announcement every second and buried the two things
 * the user acts on (the code arriving, and the code expiring). Both panes now
 * render `DeviceCodeBox`, so mounting Reconnect exercises the shared block:
 * the arrival announcement (`aria-live` on the code pill) and the expiry
 * announcement (`role="alert"`) survive; the countdown is silent.
 *
 * The countdown is asserted through the real 1 s ticker: the test advances
 * fake timers in 1 s steps and keeps every live region's text, so the
 * pre-fix markup (a live region whose text changed 30 times) fails here.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, cleanup, waitFor } from '@testing-library/svelte';
import { tick } from 'svelte';
import { get } from 'svelte/store';
import type { Mock } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';

type Listener = { event: string; fn: (e: { payload: unknown }) => void };
const listeners: Listener[] = [];

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/event', () => ({
  // Mirror the real signature: listen(eventName, handler) → unlisten. The
  // handler is kept so the test can emit the event the backend would send.
  listen: vi.fn(async (event: string, fn: (e: { payload: unknown }) => void) => {
    const entry = { event, fn };
    listeners.push(entry);
    return () => {
      const i = listeners.indexOf(entry);
      if (i >= 0) listeners.splice(i, 1);
    };
  })
}));
vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: vi.fn(() => ({ label: 'main' }))
}));
vi.mock('@tauri-apps/api/app', () => ({ getVersion: vi.fn(async () => '4.6.0') }));
// The layout mounts UpdatePrompt; a null answer keeps the banner out of the way.
vi.mock('@tauri-apps/plugin-updater', () => ({ check: vi.fn(async () => null) }));
vi.mock('@tauri-apps/plugin-notification', () => ({
  isPermissionGranted: vi.fn(async () => true),
  requestPermission: vi.fn(async () => 'granted'),
  sendNotification: vi.fn()
}));
// The detach store reaches for WebviewWindow at import time; the layout only
// reads the pane flags. `svelte/store` is imported inside the factory because
// a hoisted `vi.mock` factory cannot use static imports.
vi.mock('$lib/stores/detach', async () => {
  const { writable } = await import('svelte/store');
  return {
    detachedPanes: writable({ logs: false, settings: false }),
    focusDetached: vi.fn(async () => {}),
    popOut: vi.fn(async () => {}),
    popIn: vi.fn(async () => {}),
    // The layout adopts the real window set on mount (#601).
    reconcileDetachedPanes: vi.fn(async () => {})
  };
});

// Static imports: the vi.mock calls above hoist, so the Tauri mocks are in
// place before the component and its stores load.
import { invoke } from '@tauri-apps/api/core';
import Reconnect from '$lib/components/Reconnect.svelte';
import Layout from '../src/routes/+layout.svelte';
import { defaultConfig, configStore } from '$lib/stores/config';
import {
  authFlow,
  resetAuthFlow,
  setTeamsDeviceCode,
  setTeamsPhase,
  teamsPollMutex,
  releaseTeamsPoll
} from '$lib/stores/authFlow.svelte';
import { currentView } from '$lib/stores/app';
import { i18n } from '$lib/i18n';
import { presence, INITIAL_PRESENCE } from '$lib/stores/presence';

const invokeMock = invoke as unknown as Mock;

const CLIENT_ID = 'a'.repeat(32);
const USER_CODE = 'ABCD-1234';
const VERIFICATION_URL = 'https://microsoft.com/devicelogin';
/** A device code is valid for roughly fifteen minutes. */
const CODE_TTL_MS = 15 * 60 * 1000;

/** The text of every element whose change would be read out. */
function liveRegions(container: HTMLElement): string[] {
  return Array.from(
    container.querySelectorAll('[aria-live], [role="status"], [role="alert"]')
  ).map((el) => el.textContent?.trim() ?? '');
}

/** Settle the mocked IPC and Svelte's microtask flush scheduler. */
async function settle(rounds = 24) {
  for (let i = 0; i < rounds; i++) await Promise.resolve();
}

function mockBackend() {
  invokeMock.mockImplementation(async (cmd: string) => {
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
        // Teams has no session: that is the branch that renders the code box.
        return {
          is_syncing: false,
          current_track: null,
          spotify_connected: true,
          teams_connected: false
        };
      default:
        return undefined;
    }
  });
}

/** Mount Reconnect mid-Teams-sign-in, with a live device code. */
async function renderWaitingTeams(expiresInMs = CODE_TTL_MS) {
  vi.useFakeTimers();
  mockBackend();
  setTeamsDeviceCode({
    userCode: USER_CODE,
    verificationUrl: VERIFICATION_URL,
    deviceCode: 'device-code',
    interval: 5,
    expiresAt: Date.now() + expiresInMs
  });
  setTeamsPhase('waiting');
  const rendered = render(Reconnect);
  await settle();
  return rendered;
}

function countdownText(container: HTMLElement): string | undefined {
  return Array.from(container.querySelectorAll('p'))
    .find((p) => (p.textContent ?? '').includes('expires in'))
    ?.textContent?.trim();
}

beforeEach(async () => {
  listeners.length = 0;
  // `isTauriRuntime` is computed at component init; the layout only binds
  // process-wide listeners inside the Tauri runtime.
  Object.defineProperty(window, '__TAURI_INTERNALS__', { value: {}, configurable: true });
  invokeMock.mockReset();
  releaseTeamsPoll();
  resetAuthFlow();
  currentView.set('dashboard');
  configStore.set(structuredClone(defaultConfig));
  presence.set({ ...INITIAL_PRESENCE });
  await i18n.set('en');
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe('device-code block (#735, #952)', () => {
  it('#735 keeps the ticking countdown out of every live region', async () => {
    const { container } = await renderWaitingTeams();

    const before = countdownText(container);
    expect(before).toBe('Code expires in 15:00');

    // Sample every live region once per second for 30 s of the countdown's
    // real ticker, keeping each distinct reading.
    const readings = new Set<string>([liveRegions(container).join(' | ')]);
    for (let second = 0; second < 30; second++) {
      await vi.advanceTimersByTimeAsync(1000);
      readings.add(liveRegions(container).join(' | '));
    }
    await settle(4);

    // The countdown the user sees did advance (so the ticker ran) ...
    expect(countdownText(container)).toBe('Code expires in 14:30');
    // ... and no live region's text changed once in that window.
    expect([...readings]).toHaveLength(1);
    // The code itself is still announced, exactly once, on arrival.
    expect(liveRegions(container)).toContain(USER_CODE);
  });

  it('#735 announces the expiry exactly once, and offers a fresh code', async () => {
    const { container } = await renderWaitingTeams(1000);

    await vi.advanceTimersByTimeAsync(2000);
    await settle(4);

    const announcements = liveRegions(container).filter((text) =>
      text.includes('expired')
    );
    expect(announcements).toHaveLength(1);
    // The dead code is not left on screen as if it still worked.
    expect(countdownText(container)).toBeUndefined();
  });

  it('#952 renders the code as the shared monospace, select-all pill', async () => {
    const { container } = await renderWaitingTeams();

    const code = container.querySelector('.device-code-box .code-display');
    expect(code?.textContent?.trim()).toBe(USER_CODE);
    // Announced on arrival — the one live region the countdown must not add to.
    expect(code?.getAttribute('aria-live')).toBe('polite');

    // The verification URL is the accent pill, not a bare link.
    const url = container.querySelector('.device-code-box a.verification-url');
    expect(url?.getAttribute('href')).toBe(VERIFICATION_URL);

    // Same actions, same weight, in every pane: secondary, never `.btn-full`.
    const actions = Array.from(container.querySelectorAll('.device-code-box button'));
    expect(actions.map((b) => b.textContent?.trim())).toEqual(['Check sign-in status']);
    expect(actions.every((b) => b.classList.contains('btn-secondary'))).toBe(true);
    expect(actions.some((b) => b.classList.contains('btn-full'))).toBe(false);
  });
});

/**
 * #814 — the layout's `teams-reconnect-required` handler must not strand a
 * displayed device code.
 *
 * A second poller emit landing while the first flow's long-blocking
 * `poll_teams_auth` invoke still held the shared mutex used to replace the
 * on-screen code and re-open the browser, then drop the new poll on the
 * mutex — the displayed code was never polled, and Check-now (same mutex)
 * could not rescue it either.
 *
 * Two rules now cover it, pinned here through the mounted layout shell:
 *   - while the stored flow is still `waiting` on a live (unexpired) code,
 *     the emit routes back to that code instead of starting a fresh flow;
 *   - when a fresh code is nevertheless issued while the mutex is held (two
 *     emits racing each other's device-code request), the newest code is
 *     parked and re-driven once the in-flight poll settles.
 *
 * Fails pre-fix: the parked `poll_teams_auth` never happens (exactly one
 * poll reaches the backend), and the first rule's emit issues a fresh
 * `start_teams_auth_device_code` call.
 */
describe('teams-reconnect-required handler (#814)', () => {
  const CODE_A = 'code-A';
  const CODE_B = 'code-B';

  function deviceCodeResponse(userCode: string, deviceCode: string) {
    return {
      user_code: userCode,
      verification_url: VERIFICATION_URL,
      device_code: deviceCode,
      interval: 5,
      expires_in: 900
    };
  }

  /** Mount the always-mounted shell and wait for the reconnect listener. */
  async function mountLayoutShell() {
    const shell = render(Layout);
    await waitFor(() => {
      expect(listeners.filter((l) => l.event === 'teams-reconnect-required').length).toBe(1);
    });
    return shell;
  }

  /** Deliver a Tauri event to every mounted listener, then flush the mocked IPC. */
  async function emit(event: string, payload: unknown) {
    for (const l of [...listeners]) {
      if (l.event === event) l.fn({ payload });
    }
    await tick();
    await settle();
  }

  it('routes back to the on-screen code while its flow is still waiting', async () => {
    setTeamsDeviceCode({
      userCode: USER_CODE,
      verificationUrl: VERIFICATION_URL,
      deviceCode: CODE_A,
      interval: 5,
      expiresAt: Date.now() + CODE_TTL_MS
    });
    setTeamsPhase('waiting');
    mockBackend();
    await mountLayoutShell();
    invokeMock.mockClear();

    await emit('teams-reconnect-required', {});

    const commands = invokeMock.mock.calls.map((call) => call[0]);
    expect(commands).not.toContain('start_teams_auth_device_code');
    expect(commands).not.toContain('open_external_url');
    expect(authFlow.teams.deviceCode).toBe(CODE_A);
    expect(authFlow.teams.userCode).toBe(USER_CODE);
    expect(get(currentView)).toBe('settings');
  });

  it('polls the newest code once the in-flight poll settles (mutex held)', async () => {
    let releaseStartA!: () => void;
    let releaseStartB!: () => void;
    let releasePollA!: () => void;
    const startA = new Promise<void>((resolve) => (releaseStartA = resolve));
    const startB = new Promise<void>((resolve) => (releaseStartB = resolve));
    const pollA = new Promise<void>((resolve) => (releasePollA = resolve));
    const polls: Array<{ deviceCode: string; interval: number }> = [];
    let starts = 0;
    invokeMock.mockImplementation(
      async (cmd: string, args?: { deviceCode?: string; interval?: number }) => {
        switch (cmd) {
          case 'load_config':
            return structuredClone(defaultConfig);
          case 'check_for_update':
            return null;
          case 'start_teams_auth_device_code':
            if (starts++ === 0) {
              await startA;
              return deviceCodeResponse('AAAA-1111', CODE_A);
            }
            await startB;
            return deviceCodeResponse('BBBB-2222', CODE_B);
          case 'poll_teams_auth':
            polls.push({ deviceCode: args?.deviceCode ?? '', interval: args?.interval ?? 0 });
            if (polls.length === 1) await pollA;
            return undefined;
          default:
            return undefined;
        }
      }
    );

    await mountLayoutShell();
    vi.useFakeTimers();

    // Two poller emits race each other's device-code request: neither sees a
    // waiting flow yet, so both issue a fresh code.
    await emit('teams-reconnect-required', {});
    await emit('teams-reconnect-required', {});

    releaseStartA();
    await settle();
    // The first code is stored and its poll is now in flight.
    expect(authFlow.teams.deviceCode).toBe(CODE_A);
    expect(polls.map((poll) => poll.deviceCode)).toEqual([CODE_A]);

    releaseStartB();
    await settle();
    // The second emit replaced the on-screen code while the first poll still
    // holds the mutex — the follow-up poll is parked, not dropped.
    expect(authFlow.teams.deviceCode).toBe(CODE_B);
    expect(polls.map((poll) => poll.deviceCode)).toEqual([CODE_A]);

    // The first poll settles (its result is superseded); the parked code is
    // re-driven without another emit, and the mutex is free afterwards.
    releasePollA();
    await settle();
    await vi.advanceTimersByTimeAsync(1000);
    await settle();

    expect(polls.map((poll) => poll.deviceCode)).toEqual([CODE_A, CODE_B]);
    expect(authFlow.teams.deviceCode).toBe(CODE_B);
    expect(teamsPollMutex.inFlight).toBe(false);
  });
});

/**
 * #747 — a spinner must never keep rotating for a user who asked the OS for
 * reduced motion. The sign-in wait is the longest wait in the app, so the
 * rotation is precisely where that preference matters, and the rule that stops
 * it is now global (the `.spinner` rule itself moved to app.css for #751).
 *
 * Source scan in the style of tests/hygiene.test.ts: the contract is "every
 * keyframe animation the stylesheet declares is neutralised under
 * prefers-reduced-motion". Fails pre-fix (no such rule) and fails again if the
 * animation is reintroduced without it.
 */
describe('reduced motion (#747)', () => {
  it('stops the spinner in the global reduced-motion block', () => {
    const css = readFileSync('src/app.css', 'utf8');

    expect(css).toMatch(/animation:\s*spin /);
    expect(css).toMatch(
      /@media \(prefers-reduced-motion: reduce\) \{[\s\S]*?\.spinner\s*\{\s*animation: none;/
    );
  });
});

/**
 * #751 — the pane primitives are declared once, in app.css, and no component
 * brings a copy back. The promoted set is the one the issue names.
 */
const PROMOTED_SELECTORS = ['hint', 'error-message', 'sr-only', 'spinner', 'card', 'btn-full'];
const DEVICE_CODE_SELECTORS = ['device-code-box', 'verification-url', 'code-display'];

/** Every `.svelte` source under a directory, recursively. */
function svelteSources(dir: string): string[] {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const path = `${dir}/${entry.name}`;
    if (entry.isDirectory()) return svelteSources(path);
    return entry.name.endsWith('.svelte') ? [path] : [];
  });
}

const COMPONENT_SOURCES = [...svelteSources('src/lib/components'), ...svelteSources('src/routes')];

/** Selectors a source declares as a rule of its own (`^\s*\.name {` / `,`). */
function declaredSelectors(source: string, selectors: string[]): string[] {
  return selectors.filter((selector) =>
    new RegExp(`^\\s*\\.${selector}\\s*[,{]`, 'm').test(source)
  );
}

describe('pane primitives (#751)', () => {
  it('covers every component and route source', () => {
    // A scan that silently matched nothing would pass forever.
    expect(COMPONENT_SOURCES.length).toBeGreaterThan(10);
  });

  it.each(PROMOTED_SELECTORS)('declares .%s exactly once, in app.css', (selector) => {
    const css = readFileSync('src/app.css', 'utf8');
    const declarations = css.match(new RegExp(`^\\.${selector}\\s*[,{]`, 'gm')) ?? [];

    expect(declarations).toHaveLength(1);
  });

  it('leaves no component redeclaring a promoted selector', () => {
    const offenders = COMPONENT_SOURCES.flatMap((path) => {
      const redeclared = declaredSelectors(readFileSync(path, 'utf8'), PROMOTED_SELECTORS);
      return redeclared.map((selector) => `${path}: .${selector}`);
    });

    expect(offenders).toEqual([]);
  });

  it('keeps the device-code block in exactly one component', () => {
    const offenders = COMPONENT_SOURCES.flatMap((path) => {
      const declared = declaredSelectors(readFileSync(path, 'utf8'), DEVICE_CODE_SELECTORS);
      // Owned by DeviceCodeBox alone (#952); Onboarding, Settings and Reconnect
      // render it rather than restyling it.
      return declared
        .filter(() => !path.endsWith('DeviceCodeBox.svelte'))
        .map((selector) => `${path}: .${selector}`);
    });

    expect(offenders).toEqual([]);
    // And the owner still declares all three (a rename would silently pass).
    expect(
      declaredSelectors(
        readFileSync('src/lib/components/DeviceCodeBox.svelte', 'utf8'),
        DEVICE_CODE_SELECTORS
      )
    ).toEqual(DEVICE_CODE_SELECTORS);
  });
});
