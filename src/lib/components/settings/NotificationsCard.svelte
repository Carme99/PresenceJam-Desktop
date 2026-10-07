<script lang="ts">
  import { t, type TKey } from '$lib/i18n';
  import {
    NOTIFICATION_CLASSES,
    notificationPreferences,
    setNotificationPreference,
    type NotificationClass
  } from '$lib/stores/notifications';
  import SettingsCard from './SettingsCard.svelte';

  // Keys are `TKey`, so a class added on the Rust side cannot be rendered
  // with a missing dictionary entry.
  const NOTIFICATION_LABELS: Record<NotificationClass, TKey> = {
    track_change: 'settings.notificationsTrackChange',
    sync_stopped: 'settings.notificationsSyncStopped',
    auth_required: 'settings.notificationsAuthRequired',
    update_staged: 'settings.notificationsUpdateStaged'
  };

  // #675: one toggle per desktop-notification class. The store is the shared
  // state (persisted to `config.json` through `saveConfig`), so a toggle here
  // reaches the always-mounted main window. #549 still holds: the OS prompt's
  // answer decides whether a class may notify, and a denied permission must
  // not leave a checked toggle behind.
  let notificationsMessage = $state('');

  async function toggleNotificationClass(cls: NotificationClass, e: Event) {
    const target = e.currentTarget as HTMLInputElement;
    const applied = await setNotificationPreference(cls, target.checked);
    if (!applied) {
      // The store kept the class off, so reset the DOM property this click
      // already flipped.
      target.checked = false;
      notificationsMessage = t('settings.notificationsDenied');
      return;
    }
    notificationsMessage = '';
  }
</script>

<SettingsCard title={t('settings.sectionNotifications')}>
  {#each NOTIFICATION_CLASSES as cls (cls)}
    <div class="toggle-row">
      <label for={`notifications-${cls}`}>{t(NOTIFICATION_LABELS[cls])}</label>
      <input
        id={`notifications-${cls}`}
        data-no-draft
        type="checkbox"
        checked={$notificationPreferences[cls]}
        onchange={(e) => toggleNotificationClass(cls, e)}
      />
    </div>
  {/each}
  <p class="hint">{t('settings.notificationsHint')}</p>
  {#if notificationsMessage}
    <p class="error-message" role="alert">{notificationsMessage}</p>
  {/if}
</SettingsCard>

<style>
  .toggle-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--sp-3);
    padding: var(--sp-2) 0;
  }
  .toggle-row label {
    font-size: var(--fs-base);
    color: var(--fg);
  }
</style>
