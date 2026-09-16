import { writable } from 'svelte/store';

/**
 * Presence state shared by every Dashboard mount (#547, finding UiCore#1).
 *
 * `+page.svelte` destroys the view component on every view switch, so
 * anything held in component `$state` dies with it. The Teams status
 * preview, the presence-gate chip and the availability chip are fed by
 * Tauri events that only fire on a *change*: `presence-updated` is emitted
 * only on a successful Graph write and is skipped entirely for a
 * byte-identical status inside the keepalive window (#384), and
 * `SyncStatus` carries no last-status field. Visiting Settings/Logs/
 * Diagnostics and returning therefore rendered the live track card next to
 * a stale "Not configured" preview with the gate chip missing, even though
 * the write was still being suppressed.
 *
 * Holding the values here makes the events authoritative for the process
 * lifetime rather than for one mount, so hydration on mount falls out of
 * reading the store and no re-request to the backend is needed.
 *
 * This is the component-side half of the finding. The backend half —
 * promoting the poller's `last_posted_status`/availability-arm locals in
 * `src-tauri/src/polling/poll_once.rs` into `AppState` and onto
 * `SyncStatus` so a freshly-opened window can ask for them over IPC — is a
 * Rust change owned by another slice; `SyncStatus` has no such field today,
 * so `get_sync_status` cannot seed these values.
 */
export interface PresenceState {
  /** Raw status text last confirmed written to Teams; `null` when unknown. */
  postedStatus: string | null;
  /** True while the write is suppressed (busy, in a call, presenting, quiet hours). */
  gated: boolean;
  /** True while the availability sync reports the user as listening. */
  availabilityListening: boolean;
}

export const INITIAL_PRESENCE: PresenceState = {
  postedStatus: null,
  gated: false,
  availabilityListening: false
};

export const presence = writable<PresenceState>({ ...INITIAL_PRESENCE });

/**
 * `presence-updated`: a status reached Teams. A real write also means the
 * gate is no longer suppressing, so the chip clears here (issue #3.0-P2).
 */
export function markStatusPosted(status: string): void {
  presence.update((s) => ({ ...s, postedStatus: status, gated: false }));
}

/** `presence-gated`: the write is being suppressed. */
export function markPresenceGated(): void {
  presence.update((s) => ({ ...s, gated: true }));
}

/** `presence-cleared`: the track stopped and the Teams status was removed. */
export function markPresenceCleared(): void {
  presence.update((s) => ({ ...s, postedStatus: null, gated: false }));
}

/** `presence-availability-updated`, from the payload's structured flag. */
export function setAvailabilityListening(listening: boolean): void {
  presence.update((s) =>
    s.availabilityListening === listening ? s : { ...s, availabilityListening: listening }
  );
}

