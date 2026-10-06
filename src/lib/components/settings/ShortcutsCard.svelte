<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { onDestroy } from 'svelte';
  import { t, type TKey } from '$lib/i18n';
  import {
    SHORTCUT_SLOTS,
    shortcutBindingsOf,
    type ShortcutSlot
  } from '$lib/stores/config';
  import type { AppConfig, ShortcutReason, ShortcutsStatus, SlotRegistration } from '$lib/types';
  import {
    normalizeShortcutReason,
    shortcutReasonLabel,
    validateShortcutBinding
  } from '$lib/utils/shortcuts';
  import SettingsCard from './SettingsCard.svelte';

  interface Props {
    /** Bindable slice: the card edits `shortcuts.*` in place. */
    shortcuts: AppConfig['shortcuts'];
    /** Dirty-mark callback: the parent's `markDirty` (Save gate + parked-nav store). */
    onchange: () => void;
  }

  let { shortcuts = $bindable(), onchange }: Props = $props();

  const SHORTCUT_LABEL_KEYS: Record<ShortcutSlot, TKey> = {
    toggle_playback: 'settings.shortcutTogglePlayback',
    toggle_sync: 'settings.shortcutToggleSync'
  };

  const NO_REGISTRATION: SlotRegistration = { accelerator: null, registered: false, error: null };
  let shortcutStatus = $state<ShortcutsStatus>({
    toggle_playback: { ...NO_REGISTRATION },
    toggle_sync: { ...NO_REGISTRATION }
  });
  /** The backend's reason for the last rejected edit, per slot (issue #968). */
  let shortcutErrors = $state<Record<ShortcutSlot, ShortcutReason | null>>({
    toggle_playback: null,
    toggle_sync: null
  });
  /**
   * The slot whose field is recording a combination. Its grab is released for
   * as long as it records: the OS delivers the key to the grab, not to the
   * input, so re-recording a live binding would fire its action instead of
   * being captured.
   */
  let capturingSlot = $state<ShortcutSlot | null>(null);
  let lastPublishedShortcuts: AppConfig['shortcuts'] | null = null;

  // The two rows' pending values, read off the bindable draft (issue #676).
  let shortcutBindings = $derived(shortcutBindingsOf({ shortcuts } as AppConfig));

  /**
   * Key tokens the plugin's parser accepts, keyed by DOM `KeyboardEvent.code`.
   * Anything neither listed here nor a letter/digit/F-key is not captured at
   * all, so the field can never store an accelerator the backend cannot parse.
   */
  const SHORTCUT_KEY_TOKENS: Record<string, string> = {
    Space: 'Space',
    Enter: 'Enter',
    Tab: 'Tab',
    Escape: 'Escape',
    ArrowUp: 'ArrowUp',
    ArrowDown: 'ArrowDown',
    ArrowLeft: 'ArrowLeft',
    ArrowRight: 'ArrowRight',
    Backspace: 'Backspace',
    Delete: 'Delete',
    Insert: 'Insert',
    Home: 'Home',
    End: 'End',
    PageUp: 'PageUp',
    PageDown: 'PageDown',
    Minus: 'Minus',
    Equal: 'Equal',
    Comma: 'Comma',
    Period: 'Period',
    Slash: 'Slash',
    Semicolon: 'Semicolon',
    Quote: 'Quote',
    BracketLeft: 'BracketLeft',
    BracketRight: 'BracketRight',
    Backslash: 'Backslash',
    Backquote: 'Backquote',
    MediaPlayPause: 'MediaPlayPause',
    MediaStop: 'MediaStop'
  };

  /** The plugin accelerator a key press describes, or `null` when unusable. */
  function acceleratorFromEvent(e: KeyboardEvent): string | null {
    const code = e.code;
    let key = SHORTCUT_KEY_TOKENS[code] ?? null;
    if (key === null && /^Key[A-Z]$/.test(code)) key = code.slice(3);
    if (key === null && /^Digit[0-9]$/.test(code)) key = code.slice(5);
    if (key === null && /^F([1-9]|1[0-9]|2[0-4])$/.test(code)) key = code;
    if (key === null) return null;

    // The platform's primary modifier normalises to `CmdOrCtrl` — the spelling
    // the defaults use and the plugin resolves per platform — so the binding
    // still means the same key when the config moves to another machine. The
    // secondary modifier keeps its own name.
    const isMac = /mac/i.test(navigator.userAgent ?? '');
    const modifiers: string[] = [];
    if (isMac ? e.metaKey : e.ctrlKey) modifiers.push('CmdOrCtrl');
    if (isMac ? e.ctrlKey : e.metaKey) modifiers.push(isMac ? 'Ctrl' : 'Cmd');
    if (e.altKey) modifiers.push('Alt');
    if (e.shiftKey) modifiers.push('Shift');
    return [...modifiers, key].join('+');
  }

  /** Writes one binding into the config and re-checks the pair. */
  function setShortcutBinding(slot: ShortcutSlot, accelerator: string | null) {
    const bindings = shortcutBindingsOf({ shortcuts } as AppConfig);
    bindings[slot] = accelerator;
    const next = { ...bindings };
    lastPublishedShortcuts = next;
    shortcuts = next;
    onchange();
    const otherSlot = slot === 'toggle_playback' ? 'toggle_sync' : 'toggle_playback';
    void validateShortcut(slot);
    void validateShortcut(otherSlot);
  }

  /** Records the pressed combination, or leaves the field as it was. */
  function onShortcutKeydown(e: KeyboardEvent, slot: ShortcutSlot) {
    // A modifier-only press never completes a combination: keep waiting.
    if (['Control', 'Meta', 'Alt', 'Shift', 'CapsLock'].includes(e.key)) return;
    // Issue #810: a bare Escape/Enter cancels the capture instead of binding.
    if (
      (e.code === 'Escape' || e.code === 'Enter') &&
      !e.ctrlKey &&
      !e.metaKey &&
      !e.altKey &&
      !e.shiftKey
    ) {
      e.preventDefault();
      e.currentTarget instanceof HTMLInputElement
        ? e.currentTarget.blur()
        : void endShortcutCapture(slot);
      return;
    }
    e.preventDefault();
    const accelerator = acceleratorFromEvent(e);
    if (accelerator === null) return;
    setShortcutBinding(slot, accelerator);
  }

  /** Asks the backend whether a combination may bind this slot. */
  async function validateShortcut(slot: ShortcutSlot): Promise<boolean> {
    const reason = await validateShortcutBinding({ shortcuts } as AppConfig, slot);
    shortcutErrors[slot] = reason;
    return reason === null;
  }

  /**
   * One slot's status from an unvalidated IPC payload. A malformed or missing
   * slot becomes "not registered" — never a claim that a binding is live, and
   * never a crash while rendering.
   */
  function registrationFrom(raw: unknown, slot: ShortcutSlot): SlotRegistration {
    if (raw === null || typeof raw !== 'object') return { ...NO_REGISTRATION };
    const table = raw as Record<string, unknown>;
    const entry = table[slot];
    if (entry === undefined || entry === null || typeof entry !== 'object') {
      return { ...NO_REGISTRATION };
    }
    if (!('accelerator' in entry) || !('registered' in entry) || !('error' in entry)) {
      return { ...NO_REGISTRATION };
    }
    const error = normalizeShortcutReason(entry.error);
    return {
      accelerator: typeof entry.accelerator === 'string' ? entry.accelerator : null,
      registered: entry.registered === true && error === null,
      error
    };
  }

  /** Both slots' status from one IPC payload. */
  function statusFrom(raw: unknown): ShortcutsStatus {
    return {
      toggle_playback: registrationFrom(raw, 'toggle_playback'),
      toggle_sync: registrationFrom(raw, 'toggle_sync')
    };
  }

  /** Re-registers from the persisted config and adopts the reported status. */
  async function refreshShortcutStatus() {
    try {
      shortcutStatus = statusFrom(await invoke<unknown>('register_shortcuts'));
    } catch (e) {
      console.warn('[SETTINGS] register_shortcuts failed:', e);
    }
  }

  async function beginShortcutCapture(slot: ShortcutSlot) {
    capturingSlot = slot;
    try {
      shortcutStatus = statusFrom(await invoke<unknown>('unregister_shortcuts'));
    } catch (e) {
      console.warn('[SETTINGS] unregister_shortcuts failed:', e);
    }
  }

  async function endShortcutCapture(slot: ShortcutSlot) {
    if (capturingSlot !== slot) return;
    capturingSlot = null;
    await refreshShortcutStatus();
  }

  /** The card's pending rejection, if any — read by the save path. */
  export function pendingRejection(): { slot: ShortcutSlot; reason: ShortcutReason } | null {
    const rejected = SHORTCUT_SLOTS.find((slot) => shortcutErrors[slot] !== null);
    if (rejected === undefined) return null;
    const reason = shortcutErrors[rejected];
    return reason === null ? null : { slot: rejected, reason };
  }

  /** Re-registers after a save persisted the bindings. */
  export function refreshAfterSave(): void {
    void refreshShortcutStatus();
  }

  // 4.7.0 (issue #676): name any stored binding the backend will not accept,
  // and re-register from the config on screen. The parent replaces the
  // `shortcuts` object wholesale on load/import/revert, which can land after
  // this card mounts — so the pass tracks object identity, not mount, and
  // re-runs whenever a new document arrives. Local edits via
  // setShortcutBinding already replace `shortcuts` too, but they validate
  // the pair directly; re-registering here as well would fire a redundant
  // register_shortcuts round-trip per keystroke, so the effect skips the
  // object this card itself just published.
  $effect(() => {
    if (shortcuts === lastPublishedShortcuts) return;
    lastPublishedShortcuts = null;
    (async () => {
      for (const slot of SHORTCUT_SLOTS) void validateShortcut(slot);
      await refreshShortcutStatus();
    })();
  });

  // 4.7.0 (issue #676): the grabs are released while a field records a
  // combination. Navigating away mid-recording must not leave them released.
  onDestroy(() => {
    if (capturingSlot) void refreshShortcutStatus();
  });
</script>

<!-- 4.7.0 (issue #676): global shortcuts. The field records what is
     pressed — the grab is released while it records, otherwise the key
     would fire the binding instead of being captured. -->
<SettingsCard title={t('settings.sectionShortcuts')}>
  <p class="hint">{t('settings.shortcutsHint')}</p>
  {#each SHORTCUT_SLOTS as slot (slot)}
    <div class="form-group">
      <label for={`shortcut-${slot}`}>{t(SHORTCUT_LABEL_KEYS[slot])}</label>
      <div class="shortcut-row">
        <input
          id={`shortcut-${slot}`}
          type="text"
          readonly
          value={shortcutBindings[slot] ?? ''}
          placeholder={t('settings.shortcutUnbound')}
          onfocus={() => beginShortcutCapture(slot)}
          onblur={() => endShortcutCapture(slot)}
          onkeydown={(e) => onShortcutKeydown(e, slot)}
        />
        <button type="button" class="btn-link" onclick={() => setShortcutBinding(slot, null)}>
          {t('settings.shortcutClear')}
        </button>
      </div>
      {#if shortcutErrors[slot]}
        <p class="error-message" role="alert">
          {t('settings.shortcutRejected', {
            reason: shortcutReasonLabel(shortcutErrors[slot]!)
          })}
        </p>
      {:else if shortcutStatus[slot].error}
        <p class="error-message" role="alert">
          {t('settings.shortcutRegistrationFailed', {
            reason: shortcutReasonLabel(shortcutStatus[slot].error!)
          })}
        </p>
      {:else if shortcutStatus[slot].registered}
        <p class="hint" role="status">{t('settings.shortcutRegistered')}</p>
      {:else if capturingSlot === slot}
        <p class="hint" role="status">{t('settings.shortcutCaptureReleased')}</p>
      {:else}
        <p class="hint" role="status">{t('settings.shortcutNotRegistered')}</p>
      {/if}
    </div>
  {/each}
</SettingsCard>

<style>
  .form-group {
    display: flex;
    flex-direction: column;
    gap: var(--sp-2);
  }
  .form-group label {
    font-size: var(--fs-sm);
    font-weight: 600;
    color: var(--fg);
  }
  /* 4.7.0 (issue #676): a shortcut row pairs the capture field with its Clear
     action. The field is read-only on purpose — a combination is recorded, not
     typed — so it is rendered monospaced like the other machine-readable
     values in this pane. */
  .shortcut-row {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
  }
  .shortcut-row input[type='text'] {
    flex: 1 1 auto;
    min-width: 0;
    font-family: var(--font-mono);
    font-size: var(--fs-sm);
    cursor: pointer;
  }
  .shortcut-row .btn-link {
    flex: 0 0 auto;
    white-space: nowrap;
  }
</style>
