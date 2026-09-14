import { listen } from '@tauri-apps/api/event';

// The backend emits `()` (JSON `null`) on completion purely as a signal —
// tokens stay in Rust (issue #299). Listeners must not expect a payload.
export interface AuthListeners {
  onSpotifyComplete: () => void;
  onSpotifyFailed: (payload: string) => void;
  onTeamsComplete: () => void;
  onTeamsFailed: (payload: string) => void;
}

// Issue #419: the combined teardown is genuinely async (it awaits every
// underlying unsubscribe), so it is typed `() => Promise<void>` — NOT
// `UnlistenFn` (which is sync `() => void`). Call sites must `await` (or
// explicitly `void`) the teardown instead of calling it as a sync function.
export async function useAuthListeners(handlers: AuthListeners): Promise<() => Promise<void>> {
  const unlistens = await Promise.all([
    listen<null>('spotify-auth-complete', () => handlers.onSpotifyComplete()),
    listen<string>('spotify-auth-failed', (e) => handlers.onSpotifyFailed(e.payload)),
    listen<null>('teams-auth-complete', () => handlers.onTeamsComplete()),
    listen<string>('teams-auth-failed', (e) => handlers.onTeamsFailed(e.payload)),
  ]);
  return async () => {
    await Promise.all(
      unlistens.map(async (fn) => {
        try {
          await fn();
        } catch (err) {
          console.warn('[useAuthListeners] unlisten failed:', err);
        }
      }),
    );
  };
}
