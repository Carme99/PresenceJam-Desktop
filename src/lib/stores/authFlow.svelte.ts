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
}

/** Store the DeviceCodeResponse from `start_teams_auth_device_code`. */
export function setTeamsDeviceCode(state: TeamsDeviceCodeState) {
  authFlow.teams.userCode = state.userCode;
  authFlow.teams.verificationUrl = state.verificationUrl;
  authFlow.teams.deviceCode = state.deviceCode;
  authFlow.teams.interval = state.interval;
}

export function resetAuthFlow() {
  authFlow.spotify.phase = 'idle';
  authFlow.spotify.error = null;
  authFlow.teams.phase = 'idle';
  authFlow.teams.error = null;
  authFlow.teams.userCode = '';
  authFlow.teams.verificationUrl = '';
  authFlow.teams.deviceCode = '';
  authFlow.teams.interval = 5;
}