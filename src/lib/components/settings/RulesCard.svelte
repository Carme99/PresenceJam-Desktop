<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { tick } from 'svelte';
  import { t, WEEKDAY_KEYS, type TKey } from '$lib/i18n';
  import type { AppConfig } from '$lib/types';
  import type { TrackRuleAction } from '$lib/types-generated/TrackRuleAction';
  import SettingsCard from './SettingsCard.svelte';

  interface Props {
    /** Bindable slice: the card edits `status_rules.*` in place. */
    statusRules: AppConfig['status_rules'];
    /** Manual-status pair that lives on `teams.*` but renders on this card (S4 #672). */
    pausedStatusFormat: string;
    stoppedStatusFormat: string;
    /** Whether the parent draft has unsaved edits — gates the Undo affordance. */
    isDirty: boolean;
    /** Reset callback: restore shipped defaults (rules + manual-status pair). */
    onreset: () => void;
    /** Dirty-mark callback: the parent's `markDirty` (Save gate + parked-nav store). */
    onchange: () => void;
  }

  let {
    statusRules = $bindable(),
    pausedStatusFormat = $bindable(),
    stoppedStatusFormat = $bindable(),
    isDirty,
    onreset,
    onchange
  }: Props = $props();
  // Issue #432: status-rule rows reuse the card's form rhythm — a
  // wrapping flex row for quiet-hours entries, column variant for the
  // four-field track rules.
  const MAX_RULE_STATUS_CHARS = 128;

  /**
   * The five availability/activity pairs Graph `presence: setPresence`
   * accepts. Mirrors `config.rs::PRESENCE_COMBINATIONS` — the closed set the
   * backend normalizes against — and deliberately omits the two the docs say
   * have no effect.
   *
   * #955: the wire pair is the value; the visible text is a dictionary key, so
   * the dropdown is translated like the rest of the card.
   */
  const PRESENCE_OPTIONS: readonly {
    availability: string;
    activity: string;
    labelKey: TKey;
  }[] = [
    { availability: 'Available', activity: 'Available', labelKey: 'rules.presenceAvailable' },
    { availability: 'Busy', activity: 'InACall', labelKey: 'rules.presenceBusyCall' },
    {
      availability: 'Busy',
      activity: 'InAConferenceCall',
      labelKey: 'rules.presenceBusyConference'
    },
    { availability: 'Away', activity: 'Away', labelKey: 'rules.presenceAway' },
    {
      availability: 'DoNotDisturb',
      activity: 'Presenting',
      labelKey: 'rules.presenceDndPresenting'
    }
  ];

  type PresenceFields = { presence_availability: string; presence_activity: string };

  /** `"Availability|Activity"` for the row's `<select>`, `''` when unset. */
  function presenceValue(availability: string, activity: string): string {
    return availability && activity ? `${availability}|${activity}` : '';
  }

  /** Write a selected pair back, or clear BOTH fields for "don't change". */
  function applyPresenceValue(target: PresenceFields, value: string) {
    const [availability, activity] = value.split('|');
    target.presence_availability = availability ?? '';
    target.presence_activity = activity ?? '';
  }

  // Issue #432: format minutes-since-midnight as HH:MM for time inputs.
  function minutesToTime(m: number): string {
    const h = Math.floor(m / 60) % 24;
    const mm = m % 60;
    return `${String(h).padStart(2, '0')}:${String(mm).padStart(2, '0')}`;
  }
  function timeToMinutes(value: string, fallback: number): number {
    const match = /^(\d{1,2}):(\d{2})$/.exec(value.trim());
    if (!match) return fallback;
    const h = Math.min(23, Math.max(0, Number(match[1])));
    const mm = Math.min(59, Math.max(0, Number(match[2])));
    return h * 60 + mm;
  }

  // S4 (issue #672): a track rule's window END spans 0..=1440, where 1440 is
  // the end of the day (the config default). `<input type="time">` can only
  // express 00:00–23:59, and a picked 00:00 is midnight — the same instant as
  // 1440 — so it is stored as 1440 instead of 0, which would be an empty
  // window that never matches.
  function endMinutesFromTime(value: string, fallback: number): number {
    const minutes = timeToMinutes(value, fallback % 1440);
    return minutes === 0 ? 1440 : minutes;
  }

  // S4 (issue #672): array order is priority (the first matching rule wins), so
  // the card needs a way to reorder the rules.
  function moveRule(
    rules: AppConfig['status_rules']['track_rules'],
    index: number,
    delta: number
  ): void {
    const target = index + delta;
    if (target < 0 || target >= rules.length) return;
    const [moved] = rules.splice(index, 1);
    rules.splice(target, 0, moved);
    onchange();
  }

  // #981: removing a quiet-hours row or a track rule is reversible. The
  // pending removal holds the entry object itself (spliced out of the proxy
  // array, not copied) and the index it came from, so Undo re-inserts the very
  // same values at the very same position instead of an empty row.
  type QuietHoursEntry = AppConfig['status_rules']['quiet_hours'][number];
  type TrackRule = AppConfig['status_rules']['track_rules'][number];
  type PendingRemoval =
    | { list: 'quiet_hours'; index: number; entry: QuietHoursEntry }
    | { list: 'track_rules'; index: number; entry: TrackRule };
  // Only the most recent removal is undoable; removing again replaces it.
  let pendingRemoval = $state<PendingRemoval | null>(null);

  function removeQuietHours(index: number): void {
    const list = statusRules.quiet_hours;
    const entry = list[index];
    if (entry === undefined) return;
    pendingRemoval = { list: 'quiet_hours', index, entry };
    list.splice(index, 1);
    onchange();
  }

  function removeTrackRule(index: number): void {
    const list = statusRules.track_rules;
    const entry = list[index];
    if (entry === undefined) return;
    pendingRemoval = { list: 'track_rules', index, entry };
    list.splice(index, 1);
    onchange();
  }

  /** #981: put the entry back where it was, then move focus onto it. */
  async function undoRemove(): Promise<void> {
    const pending = pendingRemoval;
    if (pending === null) return;
    if (pending.list === 'quiet_hours') {
      statusRules.quiet_hours.splice(pending.index, 0, pending.entry);
    } else {
      statusRules.track_rules.splice(pending.index, 0, pending.entry);
    }
    pendingRemoval = null;
    onchange();
    // The restored row is the same DOM position it held before, so focus goes
    // there rather than back to the top of the form: keyboard and screen-reader
    // users land on the row they just recovered instead of losing their place.
    await tick();
    document
      .querySelector<HTMLElement>(
        `[data-rule-list="${pending.list}"][data-rule-index="${pending.index}"]`
      )
      ?.querySelector<HTMLElement>('input, select, button')
      ?.focus();
  }

  /** #981: the removal is committed — nothing left to undo. */
  export function commitRemoval(): void {
    pendingRemoval = null;
  }

  /** #981: the draft was abandoned — its pending removal goes with it. */
  export function clearRemoval(): void {
    pendingRemoval = null;
  }

  // -----------------------------------------------------------------
  // Issue #876: Outlook "Work hours" import.
  // -----------------------------------------------------------------

  interface WorkingHoursImportEntry {
    enabled: boolean;
    start_minutes: number;
    end_minutes: number;
    days: number[];
    replacement_status: string;
    presence_availability: string;
    presence_activity: string;
    pause_polling: boolean;
  }

  interface WorkingHoursImportWorking {
    start_minutes: number;
    end_minutes: number;
    days: number[];
    time_zone_offset_minutes: number;
  }

  interface WorkingHoursImportPreview {
    entries: WorkingHoursImportEntry[];
    working: WorkingHoursImportWorking | null;
    message: string | null;
  }

  let workingHoursPreview = $state<WorkingHoursImportPreview | null>(null);
  let workingHoursReplaceExisting = $state(true);
  let workingHoursBusy = $state(false);
  let workingHoursError = $state('');

  // Issue #868: dry-run tester state.
  interface RuleTestSynthetic {
    artist: string;
    track: string;
    album: string;
    show: string;
    device: string;
    playlist_uri: string;
    duration_ms: number;
    weekday: number;
  }
  interface RuleTestStep {
    index: number;
    enabled: boolean;
    matched: boolean;
    reason: string | null;
    negated: boolean;
    action: unknown;
    match_kind: 'substring' | 'exact' | 'glob';
  }
  interface RuleTestResult {
    matched_index: number | null;
    summary: string;
    reason_chain: RuleTestStep[];
    synthetic_track: RuleTestSynthetic;
    now_minutes: number;
    weekday: number;
  }
  let ruleTest = $state<RuleTestSynthetic>({
    artist: '',
    track: '',
    album: '',
    show: '',
    device: '',
    playlist_uri: '',
    duration_ms: 0,
    weekday: 1
  });
  let ruleTestDurationInput = $state('');
  let ruleTestMinuteInput = $state('12:00');
  let ruleTestRunning = $state(false);
  let ruleTestResult = $state<RuleTestResult | null>(null);
  let ruleTestError = $state('');

  function parseDurationToMs(value: string): number {
    const trimmed = value.trim();
    if (!trimmed) return 0;
    const parts = trimmed.split(':');
    if (parts.length === 2) {
      const minutes = parseInt(parts[0], 10);
      const seconds = parseInt(parts[1], 10);
      if (Number.isFinite(minutes) && Number.isFinite(seconds) && minutes >= 0 && seconds >= 0) {
        return (minutes * 60 + seconds) * 1000;
      }
    }
    const single = parseInt(trimmed, 10);
    if (Number.isFinite(single) && single >= 0) {
      return single * 1000;
    }
    return 0;
  }

  function parseMinuteInputToNumber(value: string): number {
    if (!value) return 0;
    const parts = value.split(':');
    if (parts.length !== 2) return 0;
    const hours = parseInt(parts[0], 10);
    const minutes = parseInt(parts[1], 10);
    if (!Number.isFinite(hours) || !Number.isFinite(minutes)) return 0;
    return Math.max(0, Math.min(1439, hours * 60 + minutes));
  }

  // Issue #868: factory for the rule `action` discriminated union.
  function actionForKind(kind: string): TrackRuleAction {
    switch (kind) {
      case 'replace':
        return { kind: 'replace', status: '' };
      case 'snoozeminutes':
        return { kind: 'snoozeminutes', value: 30 };
      case 'profile':
        return { kind: 'profile', id: '' };
      case 'presence':
        return { kind: 'presence', availability: '', activity: '' };
      default:
        return { kind: 'suppress' };
    }
  }

  async function runRuleTest() {
    ruleTestError = '';
    ruleTestRunning = true;
    try {
      const now_minutes = parseMinuteInputToNumber(ruleTestMinuteInput);
      const synthetic = {
        ...ruleTest,
        duration_ms: parseDurationToMs(ruleTestDurationInput)
      };
      const result = (await invoke('explain_rules', {
        nowMinutes: now_minutes,
        weekday: ruleTest.weekday,
        syntheticTrack: synthetic
      })) as RuleTestResult;
      ruleTestResult = result;
    } catch (e) {
      ruleTestError = typeof e === 'string' ? e : t('common.retry');
    } finally {
      ruleTestRunning = false;
    }
  }

  async function requestWorkingHoursPreview() {
    workingHoursError = '';
    workingHoursBusy = true;
    try {
      const preview = (await invoke('import_working_hours')) as WorkingHoursImportPreview;
      workingHoursPreview = preview;
      if (preview.message) {
        workingHoursError = preview.message;
      }
    } catch (e) {
      workingHoursError = typeof e === 'string' ? e : t('common.retry');
    } finally {
      workingHoursBusy = false;
    }
  }

  function cancelWorkingHoursPreview() {
    workingHoursPreview = null;
    workingHoursError = '';
    workingHoursReplaceExisting = true;
  }

  function applyWorkingHoursPreview() {
    const preview = workingHoursPreview;
    if (!preview) return;
    const existing = statusRules.quiet_hours;
    const incoming = preview.entries.map((entry) => ({
      enabled: entry.enabled,
      start_minutes: entry.start_minutes,
      end_minutes: entry.end_minutes,
      days: [...entry.days].sort((a, b) => a - b),
      replacement_status: entry.replacement_status ?? '',
      presence_availability: entry.presence_availability ?? '',
      presence_activity: entry.presence_activity ?? '',
      pause_polling: entry.pause_polling ?? false
    }));
    statusRules.quiet_hours = workingHoursReplaceExisting
      ? incoming
      : [...existing, ...incoming];
    workingHoursPreview = null;
    workingHoursReplaceExisting = true;
    onchange();
  }

  /** Issue #876: one short, translated line per preview entry. */
  function previewEntryLine(entry: WorkingHoursImportEntry, _idx: number): string {
    const days =
      entry.days.length === 0
        ? t('rules.dayEveryDay')
        : entry.days.map((d) => t(WEEKDAY_KEYS[d])).join(', ');
    return `${days} · ${minutesToTime(entry.start_minutes)}–${minutesToTime(entry.end_minutes)}`;
  }

  /** Issue #876: the "Outlook reports ..." hint line above the entry list. */
  function formatWorkingHoursSummary(working: WorkingHoursImportWorking): string {
    if (working.days.length === 0) return t('rules.importWorkingHoursDaysAllOff');
    const days = working.days.map((d) => t(WEEKDAY_KEYS[d])).join(', ');
    return t('rules.importWorkingHoursDaysLabel', {
      days,
      start: minutesToTime(working.start_minutes),
      end: minutesToTime(working.end_minutes)
    });
  }
</script>

<SettingsCard title={t('rules.sectionTitle')} resetLabel={t('common.resetToDefault')} {onreset}>
  {#snippet actions()}
    <!-- #981: the card header is where a removal is announced and taken
         back. It lives here, not on each row, because a row that was just
         removed is by definition not on screen to host its own control. -->
    {#if pendingRemoval !== null && isDirty}
      <button type="button" class="btn-link btn-link-tap" onclick={undoRemove}>
        {t('rules.undoRemove')}
      </button>
    {/if}
  {/snippet}
  <p class="hint">{t('rules.sectionHint')}</p>
  {#if statusRules == null}
    <p class="hint">{t('rules.noQuietHours')}</p>
  {:else}
    <div class="form-group">
      <span class="form-label">{t('rules.quietHoursLabel')}</span>
      <p class="hint">{t('rules.quietWindowHint')}</p>
      {#if statusRules.quiet_hours.length === 0}
        <p class="hint">{t('rules.noQuietHours')}</p>
      {/if}
      {#each statusRules.quiet_hours as entry, i}
        <!-- #746: the ordinal is appended so two rows are not announced under
             the same group name; the label keys carry no `{n}` placeholder. -->
        <div
          class="rule-row rule-col"
          role="group"
          aria-label={`${t('rules.quietHoursLabel')} ${i + 1}`}
          data-rule-list="quiet_hours"
          data-rule-index={i}
        >
          <div class="rule-row">
            <input
              type="checkbox"
              bind:checked={entry.enabled}
              aria-label={t('rules.ruleEnabled')}
            />
            <input
              type="time"
              value={minutesToTime(entry.start_minutes)}
              onchange={(e) => {
                entry.start_minutes = timeToMinutes(
                  (e.currentTarget as HTMLInputElement).value,
                  entry.start_minutes
                );
              }}
              aria-label={t('rules.quietStart')}
            />
            <span aria-hidden="true">–</span>
            <!-- S4 (issue #672): the same `00:00`-means-midnight mapping the
                 track-rule window uses, so the picker can never save a
                 silently inert `00:00–00:00` quiet window. -->
            <input
              type="time"
              value={minutesToTime(entry.end_minutes)}
              onchange={(e) => {
                entry.end_minutes = endMinutesFromTime(
                  (e.currentTarget as HTMLInputElement).value,
                  entry.end_minutes
                );
              }}
              aria-label={t('rules.quietEnd')}
            />
            <button type="button" class="btn-link btn-link-tap" onclick={() => removeQuietHours(i)}>
              {t('rules.removeRule')}
            </button>
          </div>
          <div class="rule-row days-row" role="group" aria-label={t('rules.quietDays')}>
            {#each [1, 2, 3, 4, 5, 6, 7] as day}
              <label class="rule-check day-check">
                <input
                  type="checkbox"
                  checked={entry.days.includes(day)}
                  onchange={(e) => {
                    const on = (e.currentTarget as HTMLInputElement).checked;
                    entry.days = on
                      ? [...entry.days, day].sort()
                      : entry.days.filter((d) => d !== day);
                  }}
                />
                <span>{t(WEEKDAY_KEYS[day])}</span>
              </label>
            {/each}
          </div>
          <div class="rule-row">
            <label class="rule-check">
              <input type="checkbox" bind:checked={entry.pause_polling} />
              <span>{t('rules.pausePollingLabel')}</span>
            </label>
          </div>
          <p class="hint">{t('rules.pausePollingHint')}</p>
          <div class="rule-row">
            <!-- Issue #538: the quiet-hours replacement status was config-only
                 until 4.6 — this is its editor. Finding #634: the same row
                 carries the rule's Teams presence action. -->
            <input
              type="text"
              bind:value={entry.replacement_status}
              maxlength={MAX_RULE_STATUS_CHARS}
              placeholder={t('rules.replacementPlaceholder')}
              aria-label={t('rules.replacementPlaceholder')}
            />
            <select
              value={presenceValue(entry.presence_availability, entry.presence_activity)}
              onchange={(e) =>
                applyPresenceValue(entry, (e.currentTarget as HTMLSelectElement).value)}
              aria-label={t('rules.presenceLabel')}
            >
              <option value="">{t('rules.presenceNone')}</option>
              {#each PRESENCE_OPTIONS as option}
                <option value={`${option.availability}|${option.activity}`}
                  >{t(option.labelKey)}</option
                >
              {/each}
            </select>
          </div>
          {#if entry.replacement_status.length >= MAX_RULE_STATUS_CHARS}
            <p class="clamp-hint" role="status">
              {t('rules.replacementClampHint', { max: MAX_RULE_STATUS_CHARS })}
            </p>
          {/if}
          <p class="hint">{t('rules.presenceHint')}</p>
        </div>
      {/each}
      <button
        type="button"
        class="btn-secondary"
        onclick={() => {
          statusRules.quiet_hours.push({
            enabled: true,
            start_minutes: 1320,
            end_minutes: 420,
            days: [],
            replacement_status: '',
            presence_availability: '',
            presence_activity: '',
            pause_polling: false
          });
          onchange();
        }}>{t('rules.addQuietHours')}</button
      >
      <!-- Issue #876: Outlook "Work hours" import. The button fires a
           `preview_working_hours` IPC call (issue #876 reads Graph
           `mailboxSettings.workingHours`, returns a list of
           `QuietHoursEntry` candidates). The preview renders below so
           the user can see and reject the new rules before they
           overwrite the existing quiet_hours table. -->
      <button
        type="button"
        class="btn-link"
        onclick={requestWorkingHoursPreview}
        disabled={workingHoursBusy}>{t('rules.importWorkingHours')}</button
      >
      <p class="hint">{t('rules.importWorkingHoursHint')}</p>
      {#if workingHoursError}
        <p class="hint error" role="status">{workingHoursError}</p>
      {/if}
      {#if workingHoursPreview && workingHoursPreview.entries.length > 0}
        <div
          class="rule-col import-preview"
          role="group"
          aria-label={t('rules.importWorkingHoursPreviewTitle')}
        >
          <span class="form-label">{t('rules.importWorkingHoursPreviewTitle')}</span>
          {#if workingHoursPreview.working}
            <p class="hint">{formatWorkingHoursSummary(workingHoursPreview.working)}</p>
          {/if}
          <ol class="hint preview-list">
            {#each workingHoursPreview.entries as entry, idx}
              <li>
                {previewEntryLine(entry, idx)}
              </li>
            {/each}
          </ol>
          <label class="rule-check">
            <input type="checkbox" bind:checked={workingHoursReplaceExisting} />
            <span>{t('rules.importWorkingHoursReplace')}</span>
          </label>
          <p class="hint">{t('rules.importWorkingHoursReplaceHint')}</p>
          <div class="rule-row">
            <button
              type="button"
              class="btn-secondary"
              onclick={applyWorkingHoursPreview}>{t('rules.importWorkingHoursApply')}</button
            >
            <button
              type="button"
              class="btn-link"
              onclick={cancelWorkingHoursPreview}>{t('rules.importWorkingHoursCancel')}</button
            >
          </div>
        </div>
      {/if}
    </div>
    <div class="form-group">
      <span class="form-label">{t('rules.trackRulesLabel')}</span>
      <p class="hint">{t('rules.trackRulesOrderHint')}</p>
      {#if statusRules.track_rules.length === 0}
        <p class="hint">{t('rules.noTrackRules')}</p>
      {/if}
      {#each statusRules.track_rules as rule, j}
        <!-- #746: same ordinal as the Move up/down buttons below. -->
        <div
          class="rule-row rule-col"
          role="group"
          aria-label={`${t('rules.trackRulesLabel')} ${j + 1}`}
          data-rule-list="track_rules"
          data-rule-index={j}
        >
          <div class="rule-row">
            <label class="rule-check">
              <input type="checkbox" bind:checked={rule.enabled} />
              <span>{t('rules.ruleEnabled')}</span>
            </label>
            <!-- #741: `.btn-link` is `padding: 0`, so these two arrows were
                 a ~14x21px hit area 8px apart — under the WCAG 2.5.8 24x24
                 target. `btn-link-tap` widens the target and keeps the
                 glyph and its aria-label. -->
            <button
              type="button"
              class="btn-link btn-link-tap"
              disabled={j === 0}
              aria-label={t('rules.moveRuleUp', { n: j + 1 })}
              onclick={() => moveRule(statusRules.track_rules, j, -1)}>↑</button
            >
            <button
              type="button"
              class="btn-link btn-link-tap"
              disabled={j === statusRules.track_rules.length - 1}
              aria-label={t('rules.moveRuleDown', { n: j + 1 })}
              onclick={() => moveRule(statusRules.track_rules, j, 1)}>↓</button
            >
            <button
              type="button"
              class="btn-link btn-link-tap"
              onclick={() => removeTrackRule(j)}>{t('rules.removeRule')}</button
            >
          </div>
          <div class="rule-row">
            <input
              type="text"
              bind:value={rule.artist_substring}
              placeholder={t('rules.artistPlaceholder')}
              aria-label={t('rules.artistPlaceholder')}
            />
            <input
              type="text"
              bind:value={rule.track_substring}
              placeholder={t('rules.trackPlaceholder')}
              aria-label={t('rules.trackPlaceholder')}
            />
          </div>
          <div class="rule-row">
            <input
              type="text"
              bind:value={rule.replacement_status}
              maxlength={MAX_RULE_STATUS_CHARS}
              placeholder={t('rules.replacementPlaceholder')}
              aria-label={t('rules.replacementPlaceholder')}
            />
            <select
              value={presenceValue(rule.presence_availability, rule.presence_activity)}
              onchange={(e) =>
                applyPresenceValue(rule, (e.currentTarget as HTMLSelectElement).value)}
              aria-label={t('rules.presenceLabel')}
            >
              <option value="">{t('rules.presenceNone')}</option>
              {#each PRESENCE_OPTIONS as option}
                <option value={`${option.availability}|${option.activity}`}
                  >{t(option.labelKey)}</option
                >
              {/each}
            </select>
          </div>
          {#if rule.replacement_status.length >= MAX_RULE_STATUS_CHARS}
            <p class="clamp-hint" role="status">
              {t('rules.replacementClampHint', { max: MAX_RULE_STATUS_CHARS })}
            </p>
          {/if}
          <!-- S4 (issue #672): the rule's own window, reusing the quiet-hours
               time inputs and weekday picker verbatim. -->
          <div class="rule-row">
            <input
              type="time"
              value={minutesToTime(rule.start_minutes)}
              onchange={(e) => {
                rule.start_minutes = timeToMinutes(
                  (e.currentTarget as HTMLInputElement).value,
                  rule.start_minutes
                );
              }}
              aria-label={t('rules.ruleStart')}
            />
            <span aria-hidden="true">–</span>
            <input
              type="time"
              value={minutesToTime(rule.end_minutes)}
              onchange={(e) => {
                rule.end_minutes = endMinutesFromTime(
                  (e.currentTarget as HTMLInputElement).value,
                  rule.end_minutes
                );
              }}
              aria-label={t('rules.ruleEnd')}
            />
          </div>
          <div class="rule-row days-row" role="group" aria-label={t('rules.ruleDays')}>
            {#each [1, 2, 3, 4, 5, 6, 7] as day}
              <label class="rule-check day-check">
                <input
                  type="checkbox"
                  checked={rule.days.includes(day)}
                  onchange={(e) => {
                    const on = (e.currentTarget as HTMLInputElement).checked;
                    rule.days = on
                      ? [...rule.days, day].sort()
                      : rule.days.filter((d) => d !== day);
                  }}
                />
                <span>{t(WEEKDAY_KEYS[day])}</span>
              </label>
            {/each}
          </div>
          <p class="hint">{t('rules.presenceHint')}</p>
          <!-- Issue #868: the new match-kind / album / show / device /
               playlist-uri / duration / negate / action surfaces.
            -->
          <div class="rule-row">
            <select
              aria-label={t('rules.matchKindLabel')}
              value={rule.match_kind ?? 'substring'}
              onchange={(e) => {
                rule.match_kind = (e.currentTarget as HTMLSelectElement).value as
                  | 'substring'
                  | 'exact'
                  | 'glob';
              }}
            >
              <option value="substring">{t('rules.matchKindSubstring')}</option>
              <option value="exact">{t('rules.matchKindExact')}</option>
              <option value="glob">{t('rules.matchKindGlob')}</option>
            </select>
            <input
              type="number"
              min="0"
              placeholder={t('rules.minDurationLabel')}
              aria-label={t('rules.minDurationLabel')}
              value={rule.min_duration_seconds ?? 0}
              onchange={(e) => {
                rule.min_duration_seconds = Math.max(
                  0,
                  parseInt((e.currentTarget as HTMLInputElement).value, 10) || 0
                );
              }}
            />
          </div>
          <div class="rule-row">
            <input
              type="text"
              placeholder={t('rules.albumSubstringLabel')}
              aria-label={t('rules.albumSubstringLabel')}
              bind:value={rule.album_substring}
            />
            <input
              type="text"
              placeholder={t('rules.showSubstringLabel')}
              aria-label={t('rules.showSubstringLabel')}
              bind:value={rule.show_substring}
            />
          </div>
          <div class="rule-row">
            <input
              type="text"
              placeholder={t('rules.deviceSubstringLabel')}
              aria-label={t('rules.deviceSubstringLabel')}
              bind:value={rule.device_substring}
            />
            <input
              type="text"
              placeholder={t('rules.playlistUriLabel')}
              aria-label={t('rules.playlistUriLabel')}
              bind:value={rule.playlist_uri}
            />
          </div>
          <div class="rule-row">
            <label class="rule-check">
              <input type="checkbox" bind:checked={rule.negate} />
              <span>{t('rules.negateLabel')}</span>
            </label>
          </div>
          <div class="rule-row">
            <select
              aria-label={t('rules.actionLabel')}
              value={(rule.action as { kind: string })?.kind ?? 'suppress'}
              onchange={(e) => {
                const next = (e.currentTarget as HTMLSelectElement).value;
                rule.action = actionForKind(next);
              }}
            >
              <option value="suppress">{t('rules.actionSuppress')}</option>
              <option value="replace">{t('rules.actionReplace')}</option>
              <option value="snoozeminutes">{t('rules.actionSnoozeMinutes')}</option>
              <option value="profile">{t('rules.actionProfile')}</option>
              <option value="presence">{t('rules.actionPresence')}</option>
            </select>
          </div>
          {#if (rule.action as { kind: string })?.kind === 'replace'}
            <div class="rule-row">
              <input
                type="text"
                placeholder={t('rules.actionReplaceStatusPlaceholder')}
                aria-label={t('rules.actionReplaceStatusPlaceholder')}
                value={(rule.action as { status?: string }).status ?? ''}
                oninput={(e) => {
                  rule.action = {
                    kind: 'replace',
                    status: (e.currentTarget as HTMLInputElement).value
                  };
                }}
              />
            </div>
          {/if}
          {#if (rule.action as { kind: string })?.kind === 'snoozeminutes'}
            <div class="rule-row">
              <input
                type="number"
                min="1"
                placeholder={t('rules.actionSnoozePlaceholder')}
                aria-label={t('rules.actionSnoozePlaceholder')}
                value={(rule.action as { value?: number }).value ?? 30}
                oninput={(e) => {
                  rule.action = {
                    kind: 'snoozeminutes',
                    value: Math.max(
                      1,
                      parseInt((e.currentTarget as HTMLInputElement).value, 10) || 30
                    )
                  };
                }}
              />
            </div>
          {/if}
          {#if (rule.action as { kind: string })?.kind === 'profile'}
            <div class="rule-row">
              <input
                type="text"
                placeholder={t('rules.actionProfilePlaceholder')}
                aria-label={t('rules.actionProfilePlaceholder')}
                value={(rule.action as { id?: string }).id ?? ''}
                oninput={(e) => {
                  rule.action = {
                    kind: 'profile',
                    id: (e.currentTarget as HTMLInputElement).value
                  };
                }}
              />
            </div>
          {/if}
          {#if (rule.action as { kind: string })?.kind === 'presence'}
            <div class="rule-row">
              <input
                type="text"
                placeholder={t('rules.actionAvailabilityPlaceholder')}
                aria-label={t('rules.actionAvailabilityPlaceholder')}
                value={(rule.action as { availability?: string }).availability ?? ''}
                oninput={(e) => {
                  const prev = rule.action as { availability?: string; activity?: string };
                  rule.action = {
                    kind: 'presence',
                    availability: (e.currentTarget as HTMLInputElement).value,
                    activity: prev.activity ?? ''
                  };
                }}
              />
              <input
                type="text"
                placeholder={t('rules.actionActivityPlaceholder')}
                aria-label={t('rules.actionActivityPlaceholder')}
                value={(rule.action as { activity?: string }).activity ?? ''}
                oninput={(e) => {
                  const prev = rule.action as { availability?: string; activity?: string };
                  rule.action = {
                    kind: 'presence',
                    availability: prev.availability ?? '',
                    activity: (e.currentTarget as HTMLInputElement).value
                  };
                }}
              />
            </div>
          {/if}
        </div>
      {/each}
      <button
        type="button"
        class="btn-secondary"
        onclick={() => {
          statusRules.track_rules.push({
            enabled: false,
            artist_substring: '',
            track_substring: '',
            match_kind: 'substring',
            album_substring: '',
            show_substring: '',
            device_substring: '',
            playlist_uri: '',
            min_duration_seconds: 0,
            negate: false,
            replacement_status: '',
            presence_availability: '',
            presence_activity: '',
            action: { kind: 'suppress' },
            days: [],
            start_minutes: 0,
            end_minutes: 1440
          });
          onchange();
        }}>{t('rules.addTrackRule')}</button
      >
      <!-- Issue #868: dry-run tester for the live track-rule walker. The
           user types a synthetic track / minute / weekday and we invoke
           `explain_rules`, which runs the same matcher the live
           `process_track` path uses. -->
      <div class="rule-test" role="group" aria-label={t('rules.testTitle')}>
        <span class="form-label">{t('rules.testTitle')}</span>
        <p class="hint">{t('rules.testHint')}</p>
        <div class="rule-row">
          <input
            type="text"
            placeholder={t('rules.testArtistLabel')}
            aria-label={t('rules.testArtistLabel')}
            bind:value={ruleTest.artist}
          />
          <input
            type="text"
            placeholder={t('rules.testTrackLabel')}
            aria-label={t('rules.testTrackLabel')}
            bind:value={ruleTest.track}
          />
        </div>
        <div class="rule-row">
          <input
            type="text"
            placeholder={t('rules.testAlbumLabel')}
            aria-label={t('rules.testAlbumLabel')}
            bind:value={ruleTest.album}
          />
          <input
            type="text"
            placeholder={t('rules.testShowLabel')}
            aria-label={t('rules.testShowLabel')}
            bind:value={ruleTest.show}
          />
        </div>
        <div class="rule-row">
          <input
            type="text"
            placeholder={t('rules.testDeviceLabel')}
            aria-label={t('rules.testDeviceLabel')}
            bind:value={ruleTest.device}
          />
          <input
            type="text"
            placeholder={t('rules.testPlaylistLabel')}
            aria-label={t('rules.testPlaylistLabel')}
            bind:value={ruleTest.playlist_uri}
          />
        </div>
        <div class="rule-row">
          <input
            type="text"
            placeholder={t('rules.testDurationLabel')}
            aria-label={t('rules.testDurationLabel')}
            bind:value={ruleTestDurationInput}
          />
          <select aria-label={t('rules.testWeekdayLabel')} bind:value={ruleTest.weekday}>
            {#each [1, 2, 3, 4, 5, 6, 7] as day}
              <option value={day}>{t(WEEKDAY_KEYS[day])}</option>
            {/each}
          </select>
          <input
            type="time"
            aria-label={t('rules.testMinuteLabel')}
            bind:value={ruleTestMinuteInput}
          />
        </div>
        <div class="rule-row">
          <button
            type="button"
            class="btn-secondary"
            onclick={runRuleTest}
            disabled={ruleTestRunning}
            >{ruleTestRunning ? t('rules.testRunning') : t('rules.testRun')}</button
          >
        </div>
        {#if ruleTestError}
          <p class="hint error" role="status">{ruleTestError}</p>
        {/if}
        {#if ruleTestResult}
          <p class="hint" role="status">
            {#if ruleTestResult.matched_index !== null && ruleTestResult.matched_index !== undefined}
              {t('rules.testSummaryRuleMatched', {
                index: ruleTestResult.matched_index + 1,
                summary: ruleTestResult.summary
              })}
            {:else}
              {t('rules.testSummaryNoMatch')}
            {/if}
          </p>
          <ol class="rule-test-chain">
            {#each ruleTestResult.reason_chain as step, stepIdx}
              <li class:matched={step.matched}>
                <strong>{`#${stepIdx + 1}`}</strong>
                {' — '}
                {step.matched ? t('rules.testStepMatched') : t('rules.testStepNotMatched')}
                {#if step.reason}
                  <span class="reason">{t('rules.testStepReason', { reason: step.reason })}</span>
                {/if}
                {#if step.negated}
                  <span class="reason">{t('rules.testStepNegated')}</span>
                {/if}
              </li>
            {/each}
          </ol>
        {/if}
      </div>
    </div>
    <div class="form-group">
      <span class="form-label">{t('rules.manualStatusLabel')}</span>
      <p class="hint">{t('rules.manualStatusHint')}</p>
      <div class="rule-row">
        <input
          type="text"
          bind:value={pausedStatusFormat}
          maxlength={MAX_RULE_STATUS_CHARS}
          placeholder={t('rules.pausedStatusPlaceholder')}
          aria-label={t('rules.pausedStatusPlaceholder')}
        />
        <input
          type="text"
          bind:value={stoppedStatusFormat}
          maxlength={MAX_RULE_STATUS_CHARS}
          placeholder={t('rules.stoppedStatusPlaceholder')}
          aria-label={t('rules.stoppedStatusPlaceholder')}
        />
      </div>
    </div>
  {/if}
</SettingsCard>

<style>
  .form-group {
    display: flex;
    flex-direction: column;
    gap: var(--sp-2);
  }
  .form-group .form-label {
    font-size: var(--fs-sm);
    font-weight: 600;
    color: var(--fg);
  }
  /* Issue #432: status-rule rows reuse the card's form rhythm — a
  wrapping flex row for quiet-hours entries, column variant for the
  four-field track rules. */
  .rule-row {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--sp-2);
  }
  .rule-row input[type='text'],
  .rule-row input[type='time'] {
    flex: 1 1 120px;
    min-width: 0;
  }
  .rule-col {
    flex-direction: column;
    align-items: stretch;
  }
  .rule-check {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    font-size: var(--fs-sm);
    color: var(--fg);
  }
  /* #750 slice 2: `.clamp-hint` lives in app.css now — this copy deleted. */
</style>
