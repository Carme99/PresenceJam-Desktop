export type AuthPhase = 'idle' | 'waiting' | 'error' | 'done';

export const authFlow = $state({
  spotify: { phase: 'idle' as AuthPhase, error: null as string | null },
  teams: { phase: 'idle' as AuthPhase, error: null as string | null },
});

export function setSpotifyPhase(phase: AuthPhase, error: string | null = null) {
  authFlow.spotify.phase = phase;
  authFlow.spotify.error = error;
}

export function setTeamsPhase(phase: AuthPhase, error: string | null = null) {
  authFlow.teams.phase = phase;
  authFlow.teams.error = error;
}

export function resetAuthFlow() {
  authFlow.spotify.phase = 'idle';
  authFlow.spotify.error = null;
  authFlow.teams.phase = 'idle';
  authFlow.teams.error = null;
}