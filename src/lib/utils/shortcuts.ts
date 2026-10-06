import { invoke } from '@tauri-apps/api/core';
import { t } from '$lib/i18n';
import { SHORTCUT_SLOTS, shortcutBindingsOf, type ShortcutSlot } from '$lib/stores/config';
import type { AppConfig, ShortcutReason } from '$lib/types';

/**
 * Issue #968: a `ShortcutReason` from the IPC boundary, defensive against a
 * payload the backend did not shape (a partial chunk, an old build, a
 * future variant). Anything that does not match the typed enum becomes
 * `Unknown { message: '<unrecognized reason>' }`, so the Settings card
 * renders *something* rather than crashing.
 */
export function normalizeShortcutReason(raw: unknown): ShortcutReason | null {
  if (raw === null || raw === undefined) return null;
  if (typeof raw !== 'object') {
    return { kind: 'Unknown', message: String(raw).slice(0, 180) };
  }
  const obj = raw as Record<string, unknown>;
  if (obj.kind === 'NotAKey' && typeof obj.accelerator === 'string') {
    return { kind: 'NotAKey', accelerator: obj.accelerator };
  }
  if (obj.kind === 'Conflict' && typeof obj.other_slot === 'string') {
    return { kind: 'Conflict', other_slot: obj.other_slot };
  }
  // Issue #810: the modifier-less rejection is a typed variant, not a
  // foreign string — map it like the other known kinds so the card renders
  // the localized `settings.shortcutReasonNeedsModifier` template.
  if (obj.kind === 'NeedsModifier' && typeof obj.accelerator === 'string') {
    return { kind: 'NeedsModifier', accelerator: obj.accelerator };
  }
  if (obj.kind === 'Autostart' && typeof obj.cause === 'string') {
    return { kind: 'Autostart', cause: obj.cause };
  }
  if (obj.kind === 'X11Unavailable') {
    return { kind: 'X11Unavailable' };
  }
  if (obj.kind === 'WorkerUnavailable') {
    return { kind: 'WorkerUnavailable' };
  }
  if (obj.kind === 'Unknown' && typeof obj.message === 'string') {
    return { kind: 'Unknown', message: obj.message };
  }
  console.warn('[SETTINGS] normalizeReason: unrecognized ShortcutReason payload:', raw);
  return { kind: 'Unknown', message: 'Unrecognized reason' };
}

/**
 * Maps a typed `ShortcutReason` to its localized copy. Mirrors
 * `Dashboard.svelte::gatedReasonLabel`: each known `kind` resolves to a
 * dictionary entry the translator owns, so a German card shows German
 * copy instead of the validator's English.
 */
export function shortcutReasonLabel(reason: ShortcutReason): string {
  switch (reason.kind) {
    case 'NotAKey':
      return t('settings.shortcutReasonNotAKey', { accelerator: reason.accelerator });
    case 'Conflict':
      return t('settings.shortcutReasonConflict', { other: reason.other_slot });
    // Issue #810: a bare key would be grabbed system-wide. The backend
    // names the accelerator; the card renders the localized template.
    case 'NeedsModifier':
      return t('settings.shortcutReasonNeedsModifier', { accelerator: reason.accelerator });
    case 'Autostart':
      return t('settings.shortcutReasonAutostart', { cause: reason.cause });
    case 'X11Unavailable':
      return t('settings.shortcutReasonX11Unavailable');
    case 'WorkerUnavailable':
      return t('settings.shortcutReasonWorkerUnavailable');
    case 'Unknown':
      return t('settings.shortcutReasonUnknown', { message: reason.message });
  }
}

/**
 * Asks the backend whether a combination may bind this slot, so an unparsable,
 * modifier-less or conflicting one is named inline instead of only failing
 * at registration. The other row's *pending* value goes along, and a cleared
 * row is sent as an explicit empty string, never `null`.
 *
 * Returns the rejection reason, or `null` when the slot is bindable/cleared.
 */
export async function validateShortcutBinding(
  config: AppConfig,
  slot: ShortcutSlot
): Promise<ShortcutReason | null> {
  const bindings = shortcutBindingsOf(config);
  const accelerator = bindings[slot];
  if (accelerator === null || accelerator.trim() === '') return null;
  const otherSlot = slot === 'toggle_playback' ? 'toggle_sync' : 'toggle_playback';
  try {
    await invoke('validate_shortcut', {
      accelerator,
      action: slot,
      other: bindings[otherSlot] ?? ''
    });
    return null;
  } catch (e) {
    // Issue #968: the validator rejects with a typed `ShortcutReason`.
    // Capture it as the structured value, not a string of JS error text.
    return normalizeShortcutReason(e);
  }
}
