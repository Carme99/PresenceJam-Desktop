import { get, writable } from 'svelte/store';
import {
  isPermissionGranted,
  requestPermission,
  sendNotification
} from '@tauri-apps/plugin-notification';
import { configStore, defaultConfig, saveConfig } from './config';
import { t } from '$lib/i18n';
import type { AppConfig } from '../types';

/**
 * Desktop-notification classes (#675, 4.7.0).
 *
 * Pre-4.7 this store held one boolean — "notify me when the track changes" —
 * mirrored into `localStorage`, with `Dashboard.svelte` owning the only
 * dispatch. The class set is now the four fixed in the config schema, the
 * persisted source of truth is `AppConfig.notifications` (Rust owns the
 * default: every class ON, serde-defaulted so a pre-4.7 config loads
 * unchanged), and the dispatching lives here so that the classes driven by
 * always-mounted events (`sync-stopped`, `teams-reconnect-required`,
 * `update-stage-complete`) are registered by `+layout.svelte`, not by a view
 * that unmounts.
 *
 * `AppConfig['notifications']` is the ts-rs export of Rust's
 * `NotificationsConfig`: adding a class on the Rust side without teaching the
 * frontend about it is a type error here, and `NOTIFICATION_CLASSES` is
 * asserted against it by `tests/notifications.test.ts`.
 */
export type NotificationPreferences = AppConfig['notifications'];
export type NotificationClass = keyof NotificationPreferences;

/** Card/array order — also the order the Settings card renders the toggles. */
export const NOTIFICATION_CLASSES: readonly NotificationClass[] = [
  'track_change',
  'sync_stopped',
  'auth_required',
  'update_staged'
];

/** The pre-4.7 single-boolean opt-in. Migrated once, then removed. */
export const NOTIFICATIONS_STORAGE_KEY = 'notificationsEnabled';

/**
 * In-session cross-window channel. `config.json` is the persisted truth and is
 * read on startup; this key only carries a live toggle to the other webviews
 * (the detached Settings window is the case that matters — #423's pattern,
 * which the pre-4.7 store already used). It is deliberately never read as a
 * startup source of truth, so it cannot resurrect a superseded value.
 */
export const NOTIFICATION_PREFS_MIRROR_KEY = 'presencejam:notification-prefs';

/** C8: at most one track-change notification every 5 s. */
export const TRACK_NOTIFICATION_THROTTLE_MS = 5000;

/**
 * Stable numeric id + group per class, so a platform that supports id reuse
 * (or Apple's threadIdentifier) replaces that class's previous notification in
 * place instead of stacking one per occurrence.
 */
const CLASS_TARGETS: Record<NotificationClass, { id: number; group: string }> = {
  track_change: { id: 1001, group: 'presencejam-track-change' },
  sync_stopped: { id: 1002, group: 'presencejam-sync-stopped' },
  auth_required: { id: 1003, group: 'presencejam-auth-required' },
  update_staged: { id: 1004, group: 'presencejam-update-staged' }
};

/** The track payload `spotify-track-changed` carries (the fields we render). */
export interface NotificationTrack {
  title?: string | null;
  artist?: string | null;
  album?: string | null;
  album_art_url?: string | null;
}

export const notificationPreferences = writable<NotificationPreferences>({
  ...defaultConfig.notifications
});

/** Publish to this webview only — local changes and cross-window convergence alike. */
function setLocal(next: NotificationPreferences): void {
  const current = get(notificationPreferences);
  if (NOTIFICATION_CLASSES.every((cls) => current[cls] === next[cls])) return;
  notificationPreferences.set({ ...next });
}

/**
 * The persisted config is the source of truth: every load/save result
 * re-derives the mirror, so a value the backend clamped or a config another
 * window wrote wins over a stale in-memory opinion.
 */
configStore.subscribe((cfg: AppConfig) => {
  const next = cfg?.notifications;
  // A backend that predates the section sends nothing; the defaults stand.
  if (!next) return;
  setLocal(next);
});

function parseMirror(raw: string | null): NotificationPreferences | null {
  if (!raw) return null;
  try {
    const parsed = JSON.parse(raw) as Partial<NotificationPreferences>;
    if (typeof parsed !== 'object' || parsed === null) return null;
    const next = { ...get(notificationPreferences) };
    for (const cls of NOTIFICATION_CLASSES) {
      if (typeof parsed[cls] === 'boolean') next[cls] = parsed[cls] as boolean;
    }
    return next;
  } catch {
    return null;
  }
}

/**
 * Apply the mirror a sibling webview just wrote. Exported so the `storage`
 * listener below and the regression test drive the same path.
 */
export function convergeFromMirror(mirror: NotificationPreferences | string | null): boolean {
  const next =
    typeof mirror === 'string' || mirror === null ? parseMirror(mirror) : parseMirror(JSON.stringify(mirror));
  if (!next) return false;
  setLocal(next);
  return true;
}

function writeMirror(next: NotificationPreferences): void {
  try {
    window.localStorage.setItem(NOTIFICATION_PREFS_MIRROR_KEY, JSON.stringify(next));
  } catch {
    // Best-effort: a blocked localStorage only costs cross-window convergence.
  }
}

// #423 pattern: `storage` fires in every OTHER same-origin webview on write.
if (typeof window !== 'undefined') {
  window.addEventListener('storage', (e) => {
    if (e.key !== NOTIFICATION_PREFS_MIRROR_KEY) return;
    convergeFromMirror(e.newValue);
  });
}

function readLegacyPreference(): boolean | null {
  try {
    const raw = window.localStorage.getItem(NOTIFICATIONS_STORAGE_KEY);
    if (raw === 'true') return true;
    if (raw === 'false') return false;
    return null;
  } catch {
    // localStorage blocked: nothing to migrate, the config default stands.
    return null;
  }
}

function clearLegacyPreference(): void {
  try {
    window.localStorage.removeItem(NOTIFICATIONS_STORAGE_KEY);
  } catch {
    // Best-effort; a key that survives only re-applies the same value.
  }
}

/**
 * Pre-4.7 migration (#675): the single `notificationsEnabled` boolean becomes
 * `notifications.track_change`, the one class it ever governed. The legacy key
 * is removed in the same breath, so the value is honoured exactly once and
 * config owns it from then on.
 *
 * Never rejects — a failed write leaves the migrated value live in this
 * session (the pre-4.7 store's best-effort rule) and the next launch simply
 * migrates again.
 */
export async function migrateLegacyNotificationPreference(cfg: AppConfig): Promise<AppConfig> {
  const legacy = readLegacyPreference();
  if (legacy === null) return cfg;
  clearLegacyPreference();
  const next: AppConfig = { ...cfg, notifications: { ...cfg.notifications, track_change: legacy } };
  try {
    return await saveConfig(next);
  } catch (e) {
    console.warn('[NOTIFICATIONS] legacy opt-in could not be persisted:', e);
    setLocal(next.notifications);
    configStore.set(next);
    return next;
  }
}

/**
 * The #549 rule, now shared by every class: the OS prompt's answer decides
 * whether the class may notify. Denied (or a plugin that throws, e.g. outside
 * the Tauri runtime) means no notification, never a silent checkbox.
 */
async function ensurePermission(): Promise<boolean> {
  let granted = false;
  try {
    granted = await isPermissionGranted();
  } catch {
    // Fall through to the request path.
  }
  if (!granted) {
    try {
      granted = (await requestPermission()) === 'granted';
    } catch {
      // A refused/there-is-no-prompt answer is a "no", not a crash.
    }
  }
  return granted;
}

/**
 * Send one notification for `cls`, if the class is on and the OS allows it.
 * Returns whether a notification was actually handed to the plugin — the
 * seam the tests assert through (a headless box cannot observe the toast).
 */
async function sendNow(
  cls: NotificationClass,
  title: string,
  body?: string,
  icon?: string
): Promise<boolean> {
  if (!get(notificationPreferences)[cls]) return false;
  if (!(await ensurePermission())) return false;
  const target = CLASS_TARGETS[cls];
  try {
    sendNotification({ title, body, icon: icon || undefined, id: target.id, group: target.group });
    return true;
  } catch (e) {
    console.warn(`[NOTIFICATIONS] sendNotification (${cls}) failed:`, e);
    return false;
  }
}

let lastNotifiedId = '';
let lastNotifiedAt = 0;

/**
 * Track change — the pre-4.7 class, with its behaviour unchanged: one
 * notification every 5 s at most, replace-in-place id, and an identical
 * title+artist never notifies twice. A throttled track does NOT claim
 * `lastNotifiedId`, so once the window elapses the genuinely-current track can
 * still notify. The bookkeeping is claimed before the permission check, as it
 * was in Dashboard, so a denied permission cannot retry on every track.
 */
export async function notifyTrackChange(track: NotificationTrack): Promise<boolean> {
  if (!track?.title) return false;
  if (!get(notificationPreferences).track_change) return false;
  const id = `${track.title}::${track.artist}`;
  if (id === lastNotifiedId) return false;
  const now = Date.now();
  if (now - lastNotifiedAt < TRACK_NOTIFICATION_THROTTLE_MS) return false;
  lastNotifiedId = id;
  lastNotifiedAt = now;
  const body = `${track.artist ?? ''} — ${track.album ?? ''}`.trim();
  return sendNow('track_change', String(track.title), body, track.album_art_url ?? undefined);
}

/**
 * The poller stopped on its own (S1 made `sync-stopped` fire exactly once per
 * stop on both the explicit and the self-exit path). One occurrence, one
 * notification — there is no timer in this class.
 */
export async function notifySyncStopped(): Promise<boolean> {
  return sendNow(
    'sync_stopped',
    t('notifications.syncStoppedTitle'),
    t('notifications.syncStoppedBody')
  );
}

/** A stored Teams session is no longer usable; a sign-in is required. */
export async function notifyAuthRequired(): Promise<boolean> {
  return sendNow(
    'auth_required',
    t('notifications.authRequiredTitle'),
    t('notifications.authRequiredBody')
  );
}

/** An update finished staging and will install on quit. */
export async function notifyUpdateStaged(version: string): Promise<boolean> {
  return sendNow(
    'update_staged',
    t('notifications.updateStagedTitle'),
    t('notifications.updateStagedBody', { version })
  );
}

/**
 * Flip one class. Enabling requests OS permission first (the #549 rule: the
 * prompt's answer decides the value), then persists through `saveConfig` so
 * `config.json` — not a localStorage key — is what survives a relaunch, and
 * writes the in-session mirror so an already-open sibling window converges.
 *
 * Returns whether the class ended up in the requested state.
 */
export async function setNotificationPreference(
  cls: NotificationClass,
  enabled: boolean
): Promise<boolean> {
  if (enabled && !(await ensurePermission())) return false;
  const next: NotificationPreferences = { ...get(notificationPreferences), [cls]: enabled };
  setLocal(next);
  writeMirror(next);
  const cfg = get(configStore);
  try {
    await saveConfig({ ...cfg, notifications: next });
  } catch (e) {
    // In-session the choice still stands; keep the config store coherent so a
    // later unrelated save cannot resurrect the old flag.
    console.warn('[NOTIFICATIONS] could not persist the notification preference:', e);
    configStore.set({ ...cfg, notifications: next });
  }
  return true;
}
