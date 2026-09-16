import { writable } from 'svelte/store';
import type { SyncStatus } from '$lib/types';

/**
 * Presence state shared by every Dashboard mount (#547 finding UiCore#1;
 * #670 / finding D2).
 *
 * `+page.svelte` destroys the view component on every view switch, so
 * anything held in component `$state` dies with it. This store is therefore
 * written by the always-mounted `routes/+layout.svelte` listeners — the
 * *component* state moved here in #547, but the listeners were still
 * registered by Dashboard itself, so a status, gate, pause or stop that
 * landed while Settings/Logs/Diagnostics was on screen was dropped along with
 * the component that would have rendered it.
 *
 * `hydrate()` closes the other half: a fresh window, or a session whose
 * events all predate this mount, seeds the same fields from `get_sync_status`
 * (which carries the poller's session bookkeeping). Events stay authoritative
 * for the process lifetime — hydration only fills what no event delivered.
 */
export interface PresenceState {
  /** Raw status text last confirmed written to Teams; `null` when unknown. */
  postedStatus: string | null;
  /** True while Teams shows the paused placeholder for a paused track. */
  paused: boolean;
  /** The paused placeholder the poller last reported; `null` when unknown. */
  pausedStatus: string | null;
  /** True while the write is suppressed (busy, in a call, presenting, quiet hours). */
  gated: boolean;
  /** Why the write is suppressed; `''` = unknown, the chip uses generic copy. */
  gatedReason: string;
  /** True while the availability sync reports the user as listening. */
  availabilityListening: boolean;
  /**
   * The poller reported a genuine stop (the track ended and the Teams status
   * was removed). A pause sets `paused` instead — a pause is not a stop, so
   * only this flag may drop the track card (finding D2).
   */
  stopped: boolean;
  /**
   * Mirror of `SyncStatus.is_syncing`: the poller's run state, owned here for
   * the same reason as the fields above. The Dashboard that renders it is
   * destroyed on every view switch, so `sync-started` / `sync-stopped` are
   * handled by the always-mounted layout. That also keeps a self-terminating
   * poller honest (finding D5, #669): one that exits on its own now
   * announces it, and the announcement survives an unmounted Dashboard.
   */
  syncing: boolean;
}

export const INITIAL_PRESENCE: PresenceState = {
  postedStatus: null,
  paused: false,
  pausedStatus: null,
  gated: false,
  gatedReason: '',
  availabilityListening: false,
  stopped: false,
  syncing: false
};

export const presence = writable<PresenceState>({ ...INITIAL_PRESENCE });

/**
 * Record the poller's run state (`sync-started` / `sync-stopped`, or
 * `SyncStatus.is_syncing` when a view hydrates).
 */
export function setSyncing(value: boolean): void {
  presence.update((s) => (s.syncing === value ? s : { ...s, syncing: value }));
}

/**
 * `presence-updated`: a status reached Teams. A real write also means the gate
 * is no longer suppressing, so the chip clears here (issue #3.0-P2), and the
 * freshly posted status supersedes any paused or stopped state.
 */
export function markStatusPosted(status: string): void {
  presence.update((s) => ({
    ...s,
    postedStatus: status,
    paused: false,
    pausedStatus: null,
    gated: false,
    gatedReason: '',
    stopped: false
  }));
}

/** `presence-gated`: the write is being suppressed, with the poller's reason. */
export function markPresenceGated(reason: string): void {
  presence.update((s) => ({ ...s, gated: true, gatedReason: reason, stopped: false }));
}

/**
 * `presence-paused` (finding D7, #669): the track is paused, so Teams shows
 * the paused placeholder instead of the playing status. The track card stays
 * up — `paused` drives its paused state, `stopped` is untouched.
 */
export function markPresencePaused(status: string): void {
  presence.update((s) => ({
    ...s,
    postedStatus: null,
    paused: true,
    pausedStatus: status,
    stopped: false
  }));
}

/**
 * `presence-cleared`: the track stopped and the Teams status was removed.
 * This is the only transition that drops the track card.
 */
export function markPresenceCleared(): void {
  presence.update((s) => ({
    ...s,
    postedStatus: null,
    paused: false,
    pausedStatus: null,
    gated: false,
    gatedReason: '',
    stopped: true
  }));
}

/**
 * `playback-state-changed` (finding D6, #669): the poller observed the playing
 * state flip for the track it is already tracking — the pause/resume signal
 * the poller otherwise never reports. `track_key` is deliberately not
 * consumed: the card this drives is the Dashboard's own hydrated track.
 */
export function setPlaybackState(isPlaying: boolean): void {
  presence.update((s) => {
    if (isPlaying) {
      if (!s.paused && !s.stopped) return s;
      return { ...s, paused: false, pausedStatus: null, stopped: false };
    }
    if (s.paused) return s;
    return { ...s, paused: true };
  });
}

/** `presence-availability-updated`, from the payload's structured flag. */
export function setAvailabilityListening(listening: boolean): void {
  presence.update((s) =>
    s.availabilityListening === listening ? s : { ...s, availabilityListening: listening }
  );
}

/**
 * Seed the store from the backend's session state (`get_sync_status`).
 *
 * `last_posted_status` / `presence_gated` / `presence_paused` are answers only
 * the backend holds for a view that never saw the events, so where it has an
 * answer it wins. `null` means "nothing posted this session" — not "cleared" —
 * so the fields the events already reported are kept rather than wiped on
 * every mount. `stopped` and `availabilityListening` are event-only state and
 * are left untouched. `is_syncing` is seeded too: a mount is the one moment
 * the poller's run state is asked for rather than announced.
 */
export function hydrate(status: SyncStatus): void {
  presence.update((s) => ({
    ...s,
    postedStatus: status.presence_paused ? null : (status.last_posted_status ?? s.postedStatus),
    paused: status.presence_paused,
    pausedStatus: status.presence_paused
      ? (status.last_posted_status ?? s.pausedStatus)
      : null,
    gated: status.presence_gated,
    gatedReason: status.presence_gated ? s.gatedReason : '',
    syncing: status.is_syncing
  }));
}
