import { listen, type Event, type UnlistenFn } from '@tauri-apps/api/event';

// The backend emits `()` (JSON `null`) on completion purely as a signal —
// tokens stay in Rust (issue #299). Listeners must not expect a payload.
export interface AuthListeners {
  onSpotifyComplete: () => void;
  onSpotifyFailed: (payload: string) => void;
  onTeamsComplete: () => void;
  onTeamsFailed: (payload: string) => void;
}

/**
 * An additional event subscribed alongside the four auth events, as
 * `[eventName, handler]`. Used for one-shot events that share the auth
 * listeners' lifetime (issue #376 `spotify-secret-conflict`).
 *
 * `any` mirrors `listen<T>()`'s own payload type: the caller picks the
 * payload shape it dev-logs, and this module never reads it.
 */
export type ExtraListener = [event: string, handler: (event: Event<any>) => void];

export type Registration = Promise<UnlistenFn>;

/**
 * Handles the "the component may unmount before `listen()` resolves"
 * problem (issues #392, #615) once, instead of once per call site.
 *
 * Call `add()` for every registration — before or after `dispose()`, since
 * a registration that settles after disposal is released as it settles — and
 * `dispose()` on unmount. `dispose()` is idempotent, never rejects, and
 * never waits on a registration that is still in flight.
 */
export interface ListenerTeardown {
  add: (registration: Registration) => void;
  dispose: () => Promise<void>;
}

export function useListenerTeardown(): ListenerTeardown {
  // `settled[index]` holds the unlisten of a registration that resolved while
  // still mounted. One that resolves after `dispose()` is released straight
  // from its settle handler and never recorded, so every subscription is
  // released exactly once.
  const settled: Array<UnlistenFn | null> = [];
  const removals: Array<Promise<void>> = [];
  let disposed = false;

  const release = (unlisten: UnlistenFn) => {
    removals.push(
      (async () => {
        try {
          await unlisten();
        } catch (err) {
          console.warn('[useAuthListeners] unlisten failed:', err);
        }
      })()
    );
  };

  return {
    add: (registration) => {
      const index = settled.length;
      settled.push(null);
      registration.then(
        (unlisten) => {
          // Already torn down → the subscription that just appeared is the
          // leak this helper exists to prevent.
          if (disposed) release(unlisten);
          else settled[index] = unlisten;
        },
        (err) => {
          // A failed `listen()` has nothing to release; surface it once.
          console.warn('[useAuthListeners] listen failed:', err);
        }
      );
    },
    dispose: async () => {
      disposed = true;
      // A registration that has already resolved is recorded by its settle
      // handler, which runs as a microtask — yield once so it lands before
      // the sweep. Registrations still in flight are released by that same
      // handler, so the teardown itself can never hang on one.
      await Promise.resolve();
      settled.forEach((unlisten, index) => {
        if (!unlisten) return;
        settled[index] = null;
        release(unlisten);
      });
      await Promise.all(removals);
    }
  };
}

/**
 * Subscribe to the four auth completion/failure events. Returns the combined
 * teardown synchronously; `await` it (or `void` it) on unmount.
 *
 * Handlers are not invoked once the teardown has run, so call sites no longer
 * need a `destroyed` guard inside each callback.
 */
export function useAuthListeners(
  handlers: AuthListeners,
  extraListeners: ExtraListener[] = []
): () => Promise<void> {
  const teardown = useListenerTeardown();
  // Guard read by the event callbacks: the teardown is async, so an event
  // delivered between `dispose()` and the unlisten round-trip must not reach
  // a handler that outlives its component.
  let disposed = false;

  teardown.add(
    listen<null>('spotify-auth-complete', () => {
      if (disposed) return;
      handlers.onSpotifyComplete();
    })
  );
  teardown.add(
    listen<string>('spotify-auth-failed', (e) => {
      if (disposed) return;
      handlers.onSpotifyFailed(e.payload);
    })
  );
  teardown.add(
    listen<null>('teams-auth-complete', () => {
      if (disposed) return;
      handlers.onTeamsComplete();
    })
  );
  teardown.add(
    listen<string>('teams-auth-failed', (e) => {
      if (disposed) return;
      handlers.onTeamsFailed(e.payload);
    })
  );
  for (const [event, handler] of extraListeners) {
    teardown.add(listen(event, handler));
  }

  return () => {
    disposed = true;
    return teardown.dispose();
  };
}
