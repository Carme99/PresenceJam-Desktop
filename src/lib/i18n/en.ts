/**
 * C6 i18n foundation (docs/scope-3.3.md §C6).
 *
 * English source dictionary — the single source of truth for every
 * i18n key. `de.ts` and `fr.ts` are typed against `Dict` so a missing
 * key fails type-check.
 *
 * Interpolation uses `{name}` placeholders, resolved by `t(key, params)`.
 * Strings that legitimately contain literal braces (e.g. the status
 * format placeholder hints `{artist}`, `{track}`) are only passed
 * through `t()` WITHOUT params, so their braces survive verbatim.
 *
 * Known limitation (per scope doc §C6): Rust-side error strings
 * surfaced through `invoke()` rejections and event payloads
 * (`spotify.rs`/`teams.rs`/polling) remain English; localizing them
 * would require passing a locale tag across IPC and is out of scope
 * for the foundation.
 */

export const en = {
  // ── common ────────────────────────────────────────────────────────
  'common.back': 'Back',
  'common.backToDashboard': 'Back to dashboard',
  'common.checkNow': 'Check sign-in status',
  'common.connected': 'Connected',
  'common.dismiss': 'Dismiss',
  'common.openSignInPage': 'Open the Microsoft sign-in page:',
  'common.enterCodeWhenAsked': 'Enter this code when asked:',
  'common.moreActions': 'More actions',
  'common.launchAtLogin': 'Launch at login',
  'common.loading': 'Loading...',
  'common.notConnected': 'Not connected',
  'common.reconnecting': 'Reconnecting…',
  'common.reconnect': 'Reconnect',
  'common.retry': 'Retry',
  'common.bootFailed': 'Could not load app state',
  'common.resetToDefault': 'Reset to default',
  'common.themeToggle': 'Toggle theme',
  'common.waiting': 'Waiting…',
  'common.waitingForSignIn': 'Waiting for sign-in…',
  'common.codeExpiresIn': 'Code expires in {time}',
  'common.codeExpired': 'This code has expired — it can no longer be used.',
  'common.getNewCode': 'Get a new code',
  'common.yes': 'Yes',
  'common.no': 'No',
  'common.tagline': 'Spotify → Teams Status',

  // ── dashboard ─────────────────────────────────────────────────────
  'dashboard.spotifyOff': 'Spotify off',
  'dashboard.teamsOff': 'Teams off',
  'dashboard.syncing': 'Syncing',
  'dashboard.logsDetachedTitle': 'Logs (detached — click to focus)',
  'dashboard.logsTitle': 'Logs',
  'dashboard.logsDetachedAria': 'Logs (detached in separate window)',
  'dashboard.openLogsAria': 'Open logs',
  'dashboard.diagnostics': 'Diagnostics',
  'dashboard.openDiagnosticsAria': 'Open diagnostics',
  'dashboard.settingsDetachedTitle': 'Settings (detached — click to focus)',
  'dashboard.settings': 'Settings',
  'dashboard.settingsDetachedAria': 'Settings (detached in separate window)',
  'dashboard.openSettingsAria': 'Open settings',
  'dashboard.about': 'About',
  'dashboard.aboutAria': 'About PresenceJam',
  'dashboard.pauseSync': 'Pause sync',
  'dashboard.resumeSync': 'Resume sync',
  'dashboard.presenceGated': "Status paused — you're busy, in a call, or presenting",
  'dashboard.setupRequired': 'Setup required',
  'dashboard.setupHint':
    'Connect Spotify and Microsoft Teams so your now-playing tracks can drive your Teams status.',
  'dashboard.continueSetup': 'Continue setup',
  'dashboard.playing': 'Playing',
  'dashboard.paused': 'Paused',
  'dashboard.liveStreamAria': 'Live stream — position unknown',
  'dashboard.yourTeamsStatus': 'Your Teams status',
  'dashboard.nothingPlaying': 'Nothing playing',
  'dashboard.nothingPlayingHint':
    'Play something on Spotify and it will show in your Teams status.',
  'dashboard.syncCrashed': 'Sync stopped unexpectedly. Press resume (▶) to restart it.',
  'dashboard.credentialCheckFailed': "Couldn't check your credentials. Check your connection and try again.",
  'dashboard.syncToggleFailed': "Couldn't start/stop sync. Try again — if it persists, open Diagnostics from the dashboard header.",
  'dashboard.statusNotConfigured': 'Not configured',
  'dashboard.statusNoTrack': 'No track playing',
  'dashboard.live': 'Live',
  'dashboard.refreshStatus': 'Refresh status',
  'dashboard.refreshing': 'Refreshing…',
  'dashboard.refreshFailed': 'Could not refresh status. Please try again.',
  'dashboard.refreshAria': 'Refresh Teams status now',

  // ── logs ──────────────────────────────────────────────────────────
  'logs.title': 'Logs',
  'logs.filterAria': 'Log level filter',
  'logs.level.all': 'All',
  'logs.level.trace': 'Trace',
  'logs.level.debug': 'Debug',
  'logs.level.info': 'Info',
  'logs.level.warning': 'Warning',
  'logs.level.error': 'Error',
  'logs.count_one': '{count} entry',
  'logs.count_other': '{count} entries',
  'logs.showingOf': 'Showing {shown} of {total}',
  'logs.jumpToLatest': 'Jump to latest',
  'logs.popOut': 'Pop out',
  'logs.clear': 'Clear',
  'logs.openFolder': 'Open folder',
  'logs.empty': 'No log entries yet',
  'logs.emptyHint': 'Live entries appear here once sync starts and Spotify is playing.',

  // ── settings ──────────────────────────────────────────────────────
  'settings.title': 'Settings',
  'settings.popBackIn': 'Pop back in',
  'settings.popOutActionTitle': 'Pop out into its own window',
  'settings.unsavedChanges': 'Unsaved changes',
  'settings.sectionSpotify': 'Spotify',
  'settings.sectionTeams': 'Microsoft Teams',
  'settings.sectionPresence': 'Presence',
  'settings.sectionStatusFormat': 'Status format',
  'settings.sectionPolling': 'Sync frequency',
  'settings.sectionNotifications': 'Notifications',
  'settings.sectionAppearance': 'Appearance',
  'settings.clientId': 'Client ID',
  'settings.clientIdPlaceholder': 'Enter Spotify Client ID',
  'settings.clientSecret': 'Client secret',
  'settings.secretStoredHint':
    "Stored securely in your operating system's keychain. To replace it, go back to the dashboard and choose Continue setup.",
  'settings.secretNotConfigured': 'Not configured.',
  'settings.runOnboarding': 'Run Onboarding',
  'settings.toSetUpSpotify': 'to set up Spotify.',
  'settings.reconnectSpotify': 'Reconnect Spotify',
  'settings.completeAuthInBrowser': 'Complete authentication in the browser.',
  'settings.playbackScopeBanner': 'Spotify added playback controls. Click Reconnect next to this message to enable them.',
  'settings.spotifySecretConflict':
    'The client secret in your config file differs from the one in the keychain. Reconnect Spotify to resolve.',
  'settings.teamsAuthHint':
    'Teams authentication uses your Microsoft 365 account. No additional configuration required.',
  'settings.presenceScopeBanner': 'Teams added meeting/call detection. Click Reconnect next to this message to enable it.',
  'settings.availabilitySyncLabel': 'Show Available while listening',
  'settings.availabilitySyncHint':
    'Off by default. When on, Teams shows you as Available (instead of Busy) while music plays. Note: Teams still shows Busy during calls and meetings.',
  'settings.presenceGateLabel': 'Pause status during meetings/calls/DND',
  'settings.presenceGateHint':
    "On by default. Skips writing your Spotify status while Teams says you're busy, in a meeting, in a call, or presenting.",
  'settings.formatTemplate': 'Format template',
  'settings.formatTemplatePlaceholder': '🎵 {artist} - {track} 🎧',
  'settings.livePreview': 'Live preview',
  'settings.placeholdersHint':
    'Available placeholders: {artist}, {track}, {album}, {emoji}',
  'settings.profanityFilterLabel': 'Filter profanity in status',
  'settings.placeholderTextLabel': 'Placeholder text',
  'settings.placeholderTextHint':
    'Use {emoji} for play state (🎵 playing / ⏸ paused). Shown when profanity is detected in track info.',
  'settings.placeholderTextPlaceholder': 'Currently Listening to Spotify',
  'settings.profaneSampleToggle': 'Preview with a profane sample track',
  'settings.defaultIntervalLabel': 'Default interval: {seconds}s',
  'settings.minIntervalLabel': 'Min interval (s)',
  'settings.maxIntervalLabel': 'Max interval (s)',
  'settings.clampHint':
    'Min interval exceeds max interval — max will be saved as {max}s.',
  'settings.notificationsToggle': 'Desktop notification on track change',
  'settings.notificationsHint':
    'Shows a system notification when the track changes. Disabled by default.',
  'settings.themeLabel': 'Theme',
  'settings.themeDark': 'Dark',
  'settings.themeLight': 'Light',
  'settings.languageLabel': 'Language',
  'settings.autostartError': 'Failed to update launch-at-login: {error}',
  'settings.saveChanges': 'Save changes',
  'settings.saving': 'Saving…',
  'settings.saved': 'Settings saved.',
  'settings.failedToSave': "Couldn't save settings — your edits are still here. Try again.",
  'settings.openLogsFolder': 'Open logs folder',
  'settings.previewUnavailable': '(preview unavailable)',

  // ── diagnostics ───────────────────────────────────────────────────
  'diagnostics.title': 'Diagnostics',
  'diagnostics.localOnlyHint': 'Local-only snapshot — safe to attach to a bug report.',
  'diagnostics.copy': 'Copy diagnostics',
  'diagnostics.saveToFile': 'Save to file',
  'diagnostics.copied': 'Diagnostics copied to clipboard.',
  'diagnostics.copyFailed': 'Copy failed — use "Save to file" instead.',
  'diagnostics.savedToDownloads': 'Diagnostics saved to your downloads folder.',
  'diagnostics.saveFailed': 'Save failed — use "Copy diagnostics" instead.',
  'diagnostics.collecting': 'Collecting diagnostics…',
  'diagnostics.collectFailed': 'Failed to collect diagnostics',
  'diagnostics.versions': 'Versions',
  'diagnostics.configuration': 'Configuration',
  'diagnostics.connections': 'Connections',
  'diagnostics.recentLogLines': 'Recent log lines',
  'diagnostics.app': 'PresenceJam',
  'diagnostics.tauri': 'Tauri',
  'diagnostics.os': 'OS',
  'diagnostics.spotifyClientId': 'Spotify client ID',
  'diagnostics.redirectUri': 'Redirect URI',
  'diagnostics.notSet': '(not set)',
  'diagnostics.clientSecretKeychain': 'Client secret in keychain',
  'diagnostics.clearOnPause': 'Clear status on pause',
  'diagnostics.profanityFilter': 'Profanity filter',
  'diagnostics.startMinimized': 'Start minimized',
  'diagnostics.availabilitySync': 'Teams availability sync',
  'diagnostics.presenceGate': 'Presence gate',
  'diagnostics.pollInterval': 'Poll interval (default/min/max)',
  'diagnostics.logging': 'Logging',
  'diagnostics.loggingEnabled': 'enabled ({level})',
  'diagnostics.loggingDisabled': 'disabled',
  'diagnostics.launchAtLogin': 'Launch at login',
  'diagnostics.statusRules': 'Status rules',
  'diagnostics.statusRulesValue': '{quiet}/{quietTotal} quiet hours, {rules}/{rulesTotal} track rules on',
  'diagnostics.spotifyConnected': 'Spotify connected',
  'diagnostics.spotifyTokenExpires': 'Spotify token expires',
  'diagnostics.teamsConnected': 'Teams connected',
  'diagnostics.teamsTokenExpires': 'Teams token expires',
  'diagnostics.expired': '(expired)',
  'diagnostics.keychainSpotifySecret': 'Keychain: Spotify secret present',
  'diagnostics.keychainEncryptionKey': 'Keychain: token encryption key present',
  'diagnostics.tokensNeverIncluded':
    'Token values are never included — expiry timestamps and presence flags only.',
  'diagnostics.noLogLinesYet': 'No log lines available yet.',
  'diagnostics.failedInstallTitle': 'Failed update install',
  'diagnostics.failedInstallVersion': 'Version',
  'diagnostics.failedInstallError': 'Error',
  'diagnostics.failedInstallTimestamp': 'Attempted at',
  'diagnostics.failedInstallDismissFailed': 'Could not dismiss the failed-install record.',

  // ── reconnect ─────────────────────────────────────────────────────
  'reconnect.title': 'Reconnect',
  'reconnect.description': 'Sync needs your attention. Reconnect below to resume.',
  'reconnect.missingCredentials': 'Missing credentials',
  'reconnect.failed': 'Failed',
  'reconnect.readyToReconnect': 'Ready to reconnect',
  'reconnect.spotifyOk': 'Spotify reconnected successfully.',
  'reconnect.spotifyNotConfigured':
    'Spotify credentials are not configured on this machine.',
  'reconnect.completeAuthInOpenedBrowser':
    'Complete authentication in the opened browser window.',
  'reconnect.tryAgain': 'Try again',
  'reconnect.clickBelowSpotify': 'Click below to reconnect your Spotify account.',
  'reconnect.teamsOk': 'Teams reconnected successfully.',
  'reconnect.clickBelowTeams':
    'Click below to reconnect your Microsoft Teams account.',
  'reconnect.missingCredsTitle': 'Missing Spotify credentials?',
  'reconnect.reenterCredsHint':
    "You'll need to re-enter your Client ID and Client Secret.",
  'reconnect.goToFullSetup': 'Go to full setup',
  'reconnect.reconnectTeams': 'Reconnect Teams',

  // ── about ─────────────────────────────────────────────────────────
  'about.version': 'Version {version}',
  'about.description':
    "Shows what you're playing on Spotify in your Microsoft Teams status — automatically.",
  'about.statusSync': 'Status sync',
  'about.live': 'Live',
  'about.auth': 'Sign-in',
  'about.authMethod': 'Spotify + Microsoft',
  'about.storage': 'Storage',
  'about.osKeychain': 'OS keychain',
  'about.githubRepo': 'GitHub repository',
  'about.releases': 'Releases',
  'about.reportIssue': 'Report an issue',

  // ── update banner ─────────────────────────────────────────────────
  'update.available': 'Update v{version} available',
  'update.stagedQuit': 'v{version} will be installed when you quit PresenceJam',
  'update.downloadFailed': 'Download failed — {error}',
  'update.downloadAndInstall': 'Download and install',
  'update.downloading': 'Downloading…',
  'update.installOnQuit': 'Install on quit',
  'update.preparing': 'Preparing…',
  'update.dismissAria': 'Dismiss update banner',
  'update.confirmQuitInstall':
    'Install v{staged} when you quit? Current version: v{current}.',
  'update.confirmQuitInstallUnknown': 'Install v{staged} when you quit?',
  'update.stagedVsCurrent':
    'v{staged} will be installed when you quit (current v{current})',
  'update.staleSkipped':
    'v{staged} was skipped — your current v{current} is newer.',
  'update.staleSkippedUnknown':
    'v{staged} was skipped — it is not newer than your current version.',
  'update.installAnyway': 'Install anyway',

  // ── onboarding ────────────────────────────────────────────────────
  'onboarding.stepOf': 'Step {step} of 3',
  'onboarding.step1Title': 'Connect Spotify',
  'onboarding.step1Intro':
    "Paste your Spotify Client ID and Client Secret below, then choose Connect Spotify — we'll open the Spotify sign-in page.",
  'onboarding.getCredentials': 'Get your Spotify credentials',
  'onboarding.instruction1':
    'Open the Spotify developer dashboard and create an app.',
  'onboarding.instruction2': 'Under Redirect URIs, add presencejam://callback (this tells Spotify where to send you back).',
  'onboarding.instruction3':
    "Copy the Client ID and Client Secret from the app's settings.",
  'onboarding.clientIdPlaceholder': '32-character Spotify Client ID',
  'onboarding.clientSecretPlaceholder': 'Spotify Client Secret',
  'onboarding.connectSpotify': 'Connect Spotify',
  'onboarding.signInWaiting': 'Spotify sign-in is waiting…',
  'onboarding.manualUrlHint':
    'Finish signing in with Spotify in your browser, then paste the full address from the address bar below.',
  'onboarding.manualUrlLabel': 'Spotify redirect URL',
  'onboarding.manualUrlPlaceholder': 'presencejam://callback?code=…',
  'onboarding.submitCode': 'Submit code',
  'onboarding.connectedToSpotify': 'Connected to Spotify',
  'onboarding.continue': 'Continue →',
  'onboarding.step2Title': 'Connect Microsoft Teams',
  'onboarding.step2Intro':
    "We use Microsoft's device-code flow — a one-time code you enter at a Microsoft page. No extra setup required.",
  'onboarding.startMicrosoftSignIn': 'Connect Microsoft Teams',
  'onboarding.connectedToTeams': 'Connected to Microsoft Teams',
  'onboarding.step3Title': 'Finishing touches',
  'onboarding.step3Intro':
    'Choose how your status message should look and whether PresenceJam should launch when you sign in.',
  'onboarding.statusTemplate': 'Status template',
  'onboarding.placeholdersHint': 'Placeholders: {artist}, {track}, {album}, {emoji}',
  'onboarding.pollInterval': 'How often to check Spotify: {seconds}s',
  'onboarding.settingUp': 'Setting up…',
  'onboarding.finishSetup': 'Finish setup',

  // ── validation / errors ───────────────────────────────────────────
  'validation.clientIdRequired': 'Spotify Client ID is required.',
  'validation.clientIdFormat':
    'Spotify Client ID must be exactly 32 hexadecimal characters.',
  'validation.clientSecretRequired': 'Spotify Client Secret is required.',
  'validation.clientSecretTooShort':
    'That Client Secret looks too short — it should be at least 32 characters. Check for a copy-paste slip.',
  'validation.noCodeInUrl':
    "That URL has no sign-in code in it — paste the full address from your browser's address bar after Spotify redirects you.",
  'validation.connectBothFirst':
    'Please connect both Spotify and Teams before finishing setup.',
  'validation.setupFailed': 'Setup failed: {error}',

  // ── routes / chrome ───────────────────────────────────────────────
  'routes.skipToMainContent': 'Skip to main content',
  'routes.unknownPane': 'Unknown pane: {pane}',

  // ── feat/45-features: status rules (#432) + support snapshot (#434) ──
  'rules.sectionTitle': 'Status rules',
  'rules.sectionHint':
    'Quiet hours and track rules suppress the Teams status write, reusing the same presence-gate path — a cleared rule posts automatically mid-track.',
  'rules.quietHoursLabel': 'Quiet hours',
  'rules.noQuietHours': 'No quiet hours defined — status syncs at all hours.',
  'rules.quietStart': 'Quiet hours start',
  'rules.quietEnd': 'Quiet hours end',
  'rules.quietDays': 'Active days (none selected = every day)',
  'rules.day1': 'Mon',
  'rules.day2': 'Tue',
  'rules.day3': 'Wed',
  'rules.day4': 'Thu',
  'rules.day5': 'Fri',
  'rules.day6': 'Sat',
  'rules.day7': 'Sun',
  'rules.addQuietHours': 'Add quiet hours',
  'rules.trackRulesLabel': 'Track rules',
  'rules.noTrackRules': 'No track rules defined — all tracks sync normally.',
  'rules.artistPlaceholder': 'Artist contains…',
  'rules.trackPlaceholder': 'Track title contains…',
  'rules.replacementPlaceholder': 'Post this instead (empty = suppress)',
  'rules.addTrackRule': 'Add track rule',
  'rules.removeRule': 'Remove',
  'rules.ruleEnabled': 'Enabled',
  'logs.copySnapshot': 'Copy snapshot',
  'logs.snapshotCopied': 'Redacted snapshot copied to clipboard.',
  'logs.snapshotCopyFailed': 'Could not copy the snapshot.',
  // 4.6 additions
  'dashboard.availabilityListening': 'Listening (Available)',
  'dashboard.availabilityCleared': 'Availability cleared',
  'settings.saveAndLeave': 'Save and leave',
  'settings.discardChanges': 'Discard changes',
  'settings.stayHere': 'Stay here',
  'settings.notificationsDenied':
    'Notifications are blocked by the system. Allow them in your system settings, then switch this back on.',
  'diagnostics.expiryBuffer': 'Token refresh buffer',
  'diagnostics.teamsRefreshTokenPresent': 'Teams refresh token stored',
  'reconnect.restartSignIn': 'Restart sign-in',
  'update.stagingProgress': 'Preparing update — {percent}%',
  'update.cancelStage': 'Cancel',
  'onboarding.submitting': 'Submitting…'
};

export type Dict = { readonly [K in keyof typeof en]: string };
