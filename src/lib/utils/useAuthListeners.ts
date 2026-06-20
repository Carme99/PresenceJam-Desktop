import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { SpotifyTokens, TeamsTokens } from '$lib/types';

export interface AuthListeners {
  onSpotifyComplete: (payload: SpotifyTokens | null) => void;
  onSpotifyFailed: (payload: string) => void;
  onTeamsComplete: (payload: TeamsTokens | null) => void;
  onTeamsFailed: (payload: string) => void;
}

export async function useAuthListeners(handlers: AuthListeners): Promise<UnlistenFn> {
  const unlistens: UnlistenFn[] = await Promise.all([
    listen<SpotifyTokens | null>('spotify-auth-complete', (e) => handlers.onSpotifyComplete(e.payload)),
    listen<string>('spotify-auth-failed', (e) => handlers.onSpotifyFailed(e.payload)),
    listen<TeamsTokens | null>('teams-auth-complete', (e) => handlers.onTeamsComplete(e.payload)),
    listen<string>('teams-auth-failed', (e) => handlers.onTeamsFailed(e.payload)),
  ]);
  return () => Promise.all(unlistens.map((fn) => fn()));
}