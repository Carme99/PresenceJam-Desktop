export type AuthPhase = 'idle' | 'waiting' | 'error' | 'done';

export const authFlow = $state({
  spotify: { phase: 'idle' as AuthPhase, error: null as string | null },
  teams: {
    phase: 'idle' as AuthPhase,
    error: null as string | null,
    // Device-code flow state, lifted from Onboarding/Settings so the
    // always-mounted layout listener (issue #157) can drive the flow and
    // any view can render the code/verification URI from the store.
    userCode: '',
    verificationUrl: '',
    deviceCode: '',
    interval: 5,
    // Epoch-ms at which the device code stops being valid, derived from
    // the backend's `expires_in` at receipt (issue #429). Null when the
    // flow was started by a caller that did not provide expiry (e.g. the
    // always-mounted layout listener), in which case no countdown renders.
    expiresAt: null as number | null,
  },
});

export function setSpotifyPhase(phase: AuthPhase, error: string | null = null) {
  authFlow.spotify.phase = phase;
  authFlow.spotify.error = error;
}

export function setTeamsPhase(phase: AuthPhase, error: string | null = null) {
  authFlow.teams.phase = phase;
  authFlow.teams.error = error;
}

export interface TeamsDeviceCodeState {
  userCode: string;
  verificationUrl: string;
  deviceCode: string;
  interval: number;
  // Optional so older call sites (e.g. +layout.svelte) keep compiling;
  // callers with the DeviceCodeResponse pass
  // `expiresAtFromResponse(response)`. Issue #429.
  expiresAt?: number | null;
}

/**
 * Derive the epoch-ms expiry for `setTeamsDeviceCode` from a device-code
 * response's `expires_in` (seconds). The parameter is structural (not the
 * generated `DeviceCodeResponse` type) so call sites compile whether or
 * not codegen has run. Returns null when the backend omitted expiry.
 */
export function expiresAtFromResponse(response: { expires_in?: number | null }): number | null {
  return typeof response.expires_in === 'number' ? Date.now() + response.expires_in * 1000 : null;
}

/** Format a remaining-time span as `m:ss` for the expiry countdown. */
export function formatCountdownMs(ms: number): string {
  const total = Math.max(0, Math.ceil(ms / 1000));
  const minutes = Math.floor(total / 60);
  const seconds = total % 60;
  return `${minutes}:${String(seconds).padStart(2, '0')}`;
}

/** Store the DeviceCodeResponse from `start_teams_auth_device_code`. */
export function setTeamsDeviceCode(state: TeamsDeviceCodeState) {
  authFlow.teams.userCode = state.userCode;
  authFlow.teams.verificationUrl = state.verificationUrl;
  authFlow.teams.deviceCode = state.deviceCode;
  authFlow.teams.interval = state.interval;
  authFlow.teams.expiresAt = state.expiresAt ?? null;
}

export function resetAuthFlow() {
  resetSpotifyAuthFlow();
  resetTeamsAuthFlow();
}

/** Clear only the Spotify flow; never touches Teams state. */
export function resetSpotifyAuthFlow() {
  authFlow.spotify.phase = 'idle';
  authFlow.spotify.error = null;
}

/** Clear only the Teams flow; never touches Spotify state. */
export function resetTeamsAuthFlow() {
  authFlow.teams.phase = 'idle';
  authFlow.teams.error = null;
  authFlow.teams.userCode = '';
  authFlow.teams.verificationUrl = '';
  authFlow.teams.deviceCode = '';
  authFlow.teams.interval = 5;
  authFlow.teams.expiresAt = null;
}

/** Shared `poll_teams_auth` mutex (issue #396). The device-code poll is a
 * long-blocking invoke issued from four call sites (Onboarding, Settings,
 * Reconnect, +layout); only one may run at a time. Call sites use
 * `tryAcquireTeamsPoll()` + `finally { releaseTeamsPoll(); }`. */
export const teamsPollMutex = $state({ inFlight: false });

/** Acquire the shared poll mutex. Returns false when a poll is already running. */
export function tryAcquireTeamsPoll(): boolean {
  if (teamsPollMutex.inFlight) return false;
  teamsPollMutex.inFlight = true;
  return true;
}

/** Release the shared poll mutex. Always call in a `finally` block. */
export function releaseTeamsPoll() {
  teamsPollMutex.inFlight = false;
}

/**
 * Issue #410: only `https:` URLs are safe to render as verification-URL
 * anchors. The URL arrives from the backend device-code response; a
 * non-HTTP(S) scheme (e.g. `javascript:`) must render as inert text.
 */
export function isSafeHttpUrl(url: string): boolean {
  try {
    return new URL(url).protocol === 'https:';
  } catch {
    return false;
  }
}