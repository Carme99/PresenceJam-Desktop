import { listen, type UnlistenFn } from '@tauri-apps/api/event';

// The backend emits `()` (JSON `null`) on completion purely as a signal —
// tokens stay in Rust (issue #299). Listeners must not expect a payload.
export interface AuthListeners {
  onSpotifyComplete: () => void;
  onSpotifyFailed: (payload: string) => void;
  onTeamsComplete: () => void;
  onTeamsFailed: (payload: string) => void;
}

export async function useAuthListeners(handlers: AuthListeners): Promise<UnlistenFn> {
  const unlistens: UnlistenFn[] = await Promise.all([
    listen<null>('spotify-auth-complete', () => handlers.onSpotifyComplete()),
    listen<string>('spotify-auth-failed', (e) => handlers.onSpotifyFailed(e.payload)),
    listen<null>('teams-auth-complete', () => handlers.onTeamsComplete()),
    listen<string>('teams-auth-failed', (e) => handlers.onTeamsFailed(e.payload)),
  ]);
  return () => Promise.all(unlistens.map((fn) => fn()));
}