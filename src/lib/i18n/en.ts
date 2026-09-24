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
  'common.loading': 'Loading…',
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
    'Available placeholders: {artist}, {track}, {album}, {emoji}, {device}, {playlist} (or {context}), {progress}, {shuffle}, {repeat}. Shuffle and Repeat show 🔀/🔁 only while they are on.',
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
  'settings.notificationsHint':
    'Each class you enable shows a system notification — the first one may ask your OS for permission.',
  'settings.themeLabel': 'Theme',
  'settings.themeDark': 'Dark',
  'settings.themeLight': 'Light',
  'settings.languageLabel': 'Language',
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
  'diagnostics.osRelease': 'Release',
  'diagnostics.installFlavor': 'Install flavor',
  'diagnostics.unknown': 'unknown',
  'diagnostics.spotifyClientId': 'Spotify client ID',
  'diagnostics.redirectUri': 'Redirect URI',
  'diagnostics.notSet': '(not set)',
  'diagnostics.clientSecretKeychain': 'Client secret in keychain',
  'diagnostics.clearOnPause': 'Clear status on pause',
  'diagnostics.profanityFilter': 'Profanity filter',
  'diagnostics.extraProfanityWords': 'Extra profanity words',
  'diagnostics.startMinimized': 'Start minimized',
  'diagnostics.availabilitySync': 'Teams availability sync',
  'diagnostics.presenceGate': 'Presence gate',
  'diagnostics.pollInterval': 'Poll interval (default/min/max)',
  'diagnostics.logging': 'Logging',
  'diagnostics.loggingEnabled': 'enabled ({level})',
  'diagnostics.loggingDisabled': 'disabled',
  'diagnostics.respectManualStatus': 'Respect manual status',
  'diagnostics.gateOutOfOffice': 'Gate while out of office',
  'diagnostics.locale': 'Locale',
  'diagnostics.updateChannel': 'Update channel',
  'diagnostics.configSnoozed': 'Sync snoozed',
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
  'diagnostics.resetTokenStorage': 'Reset local token storage',
  'diagnostics.resetTokenConfirm':
    'This deletes tokens.json and its sidecars plus the stored token encryption key. You will need to sign in again. Continue?',
  'diagnostics.resetTokenDone': 'Token storage reset. Sign in again to resume sync.',
  'diagnostics.resetTokenFailed': 'Reset failed — {error}',
  'diagnostics.syncRunning': 'Sync running',
  'diagnostics.syncSnoozed': 'Snoozed',
  'diagnostics.syncSnoozeMinutes': '{minutes} min left',
  'diagnostics.syncManualStatusBlocks': 'Manual status holding writes back',
  'diagnostics.syncPresenceGateReason': 'Presence-gate reason',
  'diagnostics.syncTransientFailures': 'Consecutive auth failures',
  'diagnostics.syncNetworkFailures': 'Consecutive network failures',

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
  'reconnect.tokenStorageUnusable': 'Your saved sign-in data cannot be read — the stored token encryption key is unusable.',
  'reconnect.resetTokenStorage': 'Reset local token storage',
  'reconnect.resetTokenConfirm':
    'This deletes tokens.json and its sidecars plus the stored token encryption key. You will need to sign in again. Continue?',
  'reconnect.resetTokenDone': 'Token storage reset. Sign in again below to resume.',
  'reconnect.resetTokenFailed': 'Reset failed — {error}',
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
  'onboarding.placeholdersHint':
    'Placeholders: {artist}, {track}, {album}, {emoji}, {device}, {playlist} (or {context}), {progress}, {shuffle}, {repeat}',
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
  'rules.dayEveryDay': 'Every day',
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
  'logs.openFolderError':
    'Could not open the logs folder. It may not exist yet — try restarting the app to create it.',
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
  'onboarding.submitting': 'Submitting…',
  'settings.episodeFormatHint':
    'Podcasts and audiobooks use their own template — 🎙️ {show} - {episode} — so your music template above is not applied to them.',
  'reconnect.keychainUnavailableBadge':
    'Keychain unavailable',
  'reconnect.keychainUnavailableHint':
    'PresenceJam could not read your saved Spotify Client Secret: the system keychain is locked or missing. Unlock it (or install a Secret Service provider such as gnome-keyring), then try again — your secret is still stored, so you do not need to set Spotify up again.',
  'settings.secretKeychainUnavailable':
    'System keychain unavailable — it may be locked or missing. Unlock it (or install a Secret Service provider) to use your saved secret; it is still stored.',
  'diagnostics.quarantineTitle': 'Settings were reset',
  'diagnostics.quarantineBodyNow':
    'PresenceJam could not read your settings file, so every setting was reset to its default.',
  'diagnostics.quarantineBodyEarlier':
    'A previous launch could not read your settings file and reset it to its defaults.',
  'diagnostics.quarantineBackupPresent':
    'The unreadable original was kept next to your settings file as {name}, so the values it held can still be recovered.',
  'diagnostics.quarantineBackupMissing':
    'The unreadable original is still next to your settings file as config.json.',
  'diagnostics.quarantineWhere':
    'Both files live in the PresenceJam folder inside your user configuration folder — the backup sits next to config.json.',
  // 4.6 additions (presence rules #634/#635/#636/#637 + #538 consumption sites)
  'rules.replacementClampHint':
    'Replacement status is capped at {max} characters; the rest is not posted.',
  'rules.presenceLabel': 'Presence while this rule applies',
  'rules.presenceNone': "Don't change my presence",
  'rules.presenceHint':
    'A rule can set Teams availability/activity, but only while “Availability sync” is on; it never overrides a call, a meeting or a status you set by hand.',
  'settings.respectManualStatusLabel': 'Never overwrite a status I set by hand',
  'settings.respectManualStatusHint':
    'Reuses the presence read the gate already performs, so it costs no extra request — its text is respected until you change it or it expires.',
  'settings.gateOutOfOfficeLabel': 'Pause while I am out of office',
  'settings.gateOutOfOfficeHint':
    'Skips the status update while your Teams out-of-office setting is on. A track rule with its own presence action overrides this.',
  // Issue #872: the OS-level presentation gate (full-screen app, slide
  // deck, Windows Focus Assist Quiet Time). OFF by default, matching how
  // `availabilitySync` and `gateWhenOutOfOffice` shipped — 4.7 behaviour
  // is unchanged until the user opts in.
  'settings.gateWhenPresentingLabel': 'Pause while I am presenting',
  'settings.gateWhenPresentingHint':
    'Skips writing your Spotify status while the OS reports a full-screen app, slide deck, or Quiet Time. Linux/macOS never report this signal, so the toggle is a no-op there.',
  // Issue #873: the desktop-idle gate. `0` (the default) keeps 4.7
  // behaviour — the app keeps advertising listening until the user
  // explicitly opts in.
  'settings.idleAwayLabel': 'Pause when my desktop is idle',
  'settings.idleAwayHint':
    'Stops advertising your Spotify status once the desktop has had no keyboard/mouse input for this many seconds. 60–3600; 0 disables. Linux/macOS never report this signal.',
  'settings.extraWordsLabel': 'Custom words to filter',
  'settings.extraWordsHint':
    'One word or phrase per line. Applied with the same boundaries as the built-in list.',
  'settings.extraWordsPlaceholder': 'word or phrase',
  'settings.extraWordsClampHint':
    'Only the first {max} entries of {chars} characters are kept — {kept} will be filtered.',
  'settings.pauseBackoffMaxLabel': 'Paused backoff ceiling (seconds)',
  'settings.pauseBackoffClampHint':
    'Allowed range is {min}–{max} seconds; {effective} will be used.',
  'dashboard.presenceGatedQuietHours': 'Status paused — quiet hours are active',
  'dashboard.presenceGatedTrackRule': 'Status paused — a track rule matched',
  'dashboard.presenceGatedManualStatus':
    'Status paused — you set a status message by hand',
  'dashboard.presenceGatedOutOfOffice': 'Status paused — you are out of office',
  'dashboard.presenceGatedPresenting':
    'Status paused — you are presenting or in a full-screen app',
  'dashboard.presenceGatedQuietTime': 'Status paused — Focus Assist is on',
  'dashboard.presenceGatedIdle': 'Status paused — desktop is idle',
  // 4.7.0 — S6 (tray localization): `config.locale` is the single source of
  // truth, so the picker also drives the tray and the native app menu.
  'settings.languageHint':
    'Also applies to the tray menu and the native application menu.',

  // 4.7.0 — S3 tray/logs hygiene
  'settings.teamsPersistWarning':
    'Signed in, but this device could not save the session — it works until you quit. Reconnect Teams to try saving it again.',

  // 4.7.0 — S4 (rules engine)
  'rules.quietWindowHint':
    'Quiet hours wrap over midnight — 22:00–07:00 runs through the night. Each half belongs to the night it starts on: with only Monday selected, the window covers Monday night into Tuesday morning. An end time of 00:00 means midnight (the end of the day), and a start equal to the end never matches.',
  'rules.pausePollingLabel': 'Stop polling during this window',
  'rules.pausePollingHint':
    'While this window is active Spotify is not queried at all — no status update and no Teams call. Polling resumes by itself when the window ends.',
  'rules.trackRulesOrderHint':
    'Rules are evaluated top to bottom — the first matching one wins. A rule with no weekdays applies every day, an end time of 00:00 means the end of the day, and a start equal to the end never matches. A midnight-crossing window belongs to the night it starts on: with only Monday selected, 22:00–07:00 covers Monday night into Tuesday morning.',
  'rules.ruleStart': 'Rule window start',
  'rules.ruleEnd': 'Rule window end',
  'rules.ruleDays': 'Active days for this rule (none selected = every day)',
  'rules.moveRuleUp': 'Move rule {n} up',
  'rules.moveRuleDown': 'Move rule {n} down',
  'rules.manualStatusLabel': 'Pause and stop status text',
  'rules.manualStatusHint':
    'The text posted as your Teams status while playback is paused and when nothing is playing. The music emoji is added for you; clearing a field restores the default.',
  'rules.pausedStatusPlaceholder': 'Paused',
  'rules.stoppedStatusPlaceholder': 'Nothing playing on Spotify',

  // 4.7.0 — S5 (log rotation + settings export/import)
  'settings.sectionLogging': 'Logging',
  'settings.loggingEnabledLabel': 'Write a log file',
  'settings.logLevelLabel': 'Log level',
  'settings.logMaxSizeLabel': 'Maximum log file size (MB)',
  'settings.logKeepFilesLabel': 'Archived log files to keep',
  'settings.logRotationHint':
    'The size limit and the number of archived files apply the next time PresenceJam starts. The log being written right now is not one of them: the log folder holds at most one more file than the number you set. Turning logging off or changing the level takes effect immediately.',
  'settings.sectionBackup': 'Backup',
  'settings.backupHint':
    'Export writes a copy of these settings that you can keep or move to another machine. Your Spotify client secret stays in the system keychain and is never included — and a file that carries one is refused on import.',
  'settings.backupExport': 'Export settings…',
  'settings.backupImport': 'Import settings…',
  'settings.backupExportDialogTitle': 'Export PresenceJam settings',
  'settings.backupImportDialogTitle': 'Import PresenceJam settings',
  'settings.backupConfirmOverwrite':
    'Importing replaces all of your current settings. The current file is kept beside it as config.json.bak. Continue?',
  'settings.backupExported': 'Settings exported to {path}',
  'settings.backupImported': 'Settings imported from {path}',
  'settings.backupError': 'The backup action could not be completed: {error}',

  // 4.7.0 — S9 (issue #677: the tray snooze / "pause sync for a while")
  'dashboard.snoozeChip': 'Snoozed — {remaining} left (until {time})',
  'dashboard.snoozeResume': 'Resume now',
  'dashboard.snoozeResuming': 'Resuming…',
  'dashboard.snoozeResumeFailed':
    'Could not resume syncing. The snooze is still stored — try again.',

  // 4.7.0 — S7 notifications (#675): one toggle per desktop-notification
  // class, plus the copy for the three classes the always-mounted layout
  // dispatches (track changes keep dispatching from the Dashboard card).
  'settings.notificationsTrackChange': 'Notify me when the track changes',
  'settings.notificationsSyncStopped': 'Notify me when syncing stops on its own',
  'settings.notificationsAuthRequired': 'Notify me when I have to sign in to Teams again',
  'settings.notificationsUpdateStaged': 'Notify me when an update will install on quit',
  'notifications.syncStoppedTitle': 'PresenceJam stopped syncing',
  'notifications.syncStoppedBody':
    'The status sync stopped on its own. Open PresenceJam to restart it.',
  'notifications.authRequiredTitle': 'Teams sign-in required',
  'notifications.authRequiredBody':
    'Your Teams session expired. Sign in again so your status keeps syncing.',
  'notifications.updateStagedTitle': 'Update ready',
  'notifications.updateStagedBody': 'PresenceJam {version} will be installed when you quit.',
  // 4.7.0 — update channel (#678)
  'settings.sectionUpdates': 'Updates',
  'settings.updateChannelLabel': 'Release channel',
  'settings.updateChannelStable': 'Stable',
  'settings.updateChannelBeta': 'Beta',
  'settings.updateChannelHint':
    'Beta builds use the rolling beta feed when available; if it is missing or has no newer version, Beta falls back to the stable release. Beta builds install on quit only.',
  'update.betaOnQuitOnly':
    'Beta channel: updates install when you quit — there is no download-and-relaunch path on Beta.',
  // 4.7.0 — S8 global hotkeys
  'settings.sectionShortcuts': 'Global shortcuts',
  'settings.shortcutsHint':
    'These work while the window is hidden. Click a field and press the combination you want — the field records what you press, not what you type.',
  'settings.shortcutTogglePlayback': 'Toggle playback',
  'settings.shortcutToggleSync': 'Pause or resume sync',
  'settings.shortcutUnbound': 'Not set — click and press a combination',
  'settings.shortcutClear': 'Clear',
  'settings.shortcutRegistered': 'Active',
  'settings.shortcutNotRegistered': 'Not registered on this desktop',
  'settings.shortcutCaptureReleased':
    'Released while recording — the current binding would fire instead of being recorded',
  'settings.shortcutRejected': 'Cannot be used: {reason}',
  'settings.shortcutRegistrationFailed': 'Registration failed on this desktop: {reason}',
  // Issue #968: typed reason codes from the Rust validator. The Settings
  // card maps each `kind` to a dictionary entry so the rejection copy is
  // localized; `shortcutReasonUnknown` renders the backend's free-form text
  // for genuinely foreign refusals (compositor / app-owned combos).
  'settings.shortcutReasonNotAKey': '“{accelerator}” is not a recognized shortcut',
  'settings.shortcutReasonConflict':
    'Conflicts with the {other} shortcut — one accelerator cannot drive both actions',
  // Issue #810: a bare key would be grabbed system-wide. Function keys
  // (F1–F24) and media keys are exempt and bind without a modifier.
  'settings.shortcutReasonNeedsModifier':
    '“{accelerator}” needs at least one modifier — a bare key would be grabbed in every application',
  'settings.shortcutReasonAutostart':
    'Launch-at-login failed: {cause}',
  'settings.shortcutReasonUnknown': '{message}',
  'settings.shortcutReasonX11Unavailable':
    'Global shortcuts need a reachable X11 display on this desktop',
  'settings.shortcutReasonWorkerUnavailable':
    'The global-shortcut worker could not be verified; shortcuts are unavailable',
  // 4.7.0 — S12 hygiene (theme/density)
  'settings.themeSystem': 'System',
  'settings.themeHint':
    '“System” follows your operating system’s appearance; Dark and Light stay pinned.',
  'settings.densityCompactLabel': 'Compact spacing',
  'settings.densityHint': 'Tightens the spacing and type scale. Independent of the theme.',
  // --- 5.0 wave1 i18n-lib ---
  // Key requests routed through this slice's dictionaries (the owning slice
  // cannot edit them).
  'onboarding.pollIntervalClamped':
    'Your saved interval is {stored}s, outside this step’s {min}–{max}s range — {seconds}s will be used.',
  'update.stagingProgressLabel': 'Update preparing',
  'rules.presenceAvailable': 'Available',
  'rules.presenceBusyCall': 'Busy — In a call',
  'rules.presenceBusyConference': 'Busy — In a conference call',
  'rules.presenceAway': 'Away',
  'rules.presenceDndPresenting': 'Do not disturb — Presenting',
  // --- 5.0 wave3 features-presence ---
  'dashboard.manualStatusTitle': 'Manual status',
  'dashboard.manualStatusPlaceholder': 'Set a status your team can see for a while',
  'dashboard.manualStatusExpiryLabel': 'Expires after',
  'dashboard.manualStatusExpiry15': '15 minutes',
  'dashboard.manualStatusExpiry30': '30 minutes',
  'dashboard.manualStatusExpiry60': '1 hour',
  'dashboard.manualStatusExpiry120': '2 hours',
  'dashboard.manualStatusSet': 'Set status',
  'dashboard.manualStatusClear': 'Clear status',
  'dashboard.manualStatusActive': 'Active until {expiry}',
  'dashboard.manualStatusActiveEmpty': 'Active (expires soon)',
  'dashboard.manualStatusRecentTitle': 'Recent statuses',
  'dashboard.manualStatusRecentEmpty': 'No recent statuses yet',
  'dashboard.manualStatusFiltered': 'Status was rewritten by your profanity filter',
  'dashboard.activityTitle': 'Activity',
  'dashboard.activityEmpty': 'No decisions yet — start sync to see what your app chose',
  'dashboard.volumeLabel': 'Volume',
  'dashboard.volumeAria': 'Spotify volume slider',
  'dashboard.seekAria': 'Click to seek',
  'dashboard.seekUnavailableAria': 'This device does not support seeking',

  // --- 5.0 wave3 features-outlook ---
  'rules.importWorkingHours': 'Import Outlook working hours',
  'rules.importWorkingHoursHint': 'Reads your Work hours tab and turns each off-block into a quiet-hours rule. Preview before applying.',
  'rules.importWorkingHoursPreviewTitle': 'Outlook working hours preview',
  'rules.importWorkingHoursApply': 'Apply these rules',
  'rules.importWorkingHoursReplace': 'Replace existing quiet-hours rules',
  'rules.importWorkingHoursCancel': 'Cancel',
  'rules.importWorkingHoursReplaceHint': 'Replaces your existing quiet-hours rules with the imported set. Uncheck to keep both.',
  'rules.importWorkingHoursDaysLabel': 'Outlook reports {days} working on {start}–{end}',
  'rules.importWorkingHoursDaysAllOff': 'Outlook reports no working days — nothing to import',

  // --- 5.0 wave3 features-gating ---

  // --- 5.0 wave3 features-profiles ---
  'rules.testTitle': 'Test these rules',
  'rules.testHint':
    'Type a sample track, pick a minute-of-day + weekday, and see exactly which rule (if any) would fire.',
  'rules.testArtistLabel': 'Artist',
  'rules.testTrackLabel': 'Track title',
  'rules.testAlbumLabel': 'Album (optional)',
  'rules.testShowLabel': 'Show / podcast (optional)',
  'rules.testDeviceLabel': 'Spotify device (optional)',
  'rules.testPlaylistLabel': 'Playlist URI (optional)',
  'rules.testDurationLabel': 'Duration (mm:ss, optional)',
  'rules.testWeekdayLabel': 'Weekday',
  'rules.testMinuteLabel': 'Minute of day (HH:MM)',
  'rules.testRun': 'Run test',
  'rules.testRunning': 'Running…',
  'rules.testSummaryNoMatch': 'No rule would match this track.',
  'rules.testSummaryRuleMatched': 'Rule {index} would fire: {summary}',
  'rules.testStepMatched': 'matched',
  'rules.testStepNotMatched': 'did NOT match',
  'rules.testStepReason': 'reason: {reason}',
  'rules.testStepDisabled': 'rule is disabled',
  'rules.testStepScheduleOutside': 'schedule does not contain this minute-of-day',
  'rules.testStepNegated': 'negate flipped the conditions',
  'rules.matchKindLabel': 'Match style',
  'rules.matchKindSubstring': 'Substring (default)',
  'rules.matchKindExact': 'Exact match',
  'rules.matchKindGlob': 'Glob pattern',
  'rules.albumSubstringLabel': 'Album contains…',
  'rules.showSubstringLabel': 'Show contains…',
  'rules.deviceSubstringLabel': 'Device contains…',
  'rules.playlistUriLabel': 'Playlist URI contains…',
  'rules.minDurationLabel': 'Minimum duration (seconds)',
  'rules.negateLabel': 'Negate (match when conditions do NOT hold)',
  'rules.actionLabel': 'Action',
  'rules.actionSuppress': 'Suppress status',
  'rules.actionReplace': 'Replace status with…',
  'rules.actionSnoozeMinutes': 'Snooze for minutes…',
  'rules.actionProfile': 'Switch to profile…',
  'rules.actionPresence': 'Set presence pair…',
  'rules.actionReplaceStatusPlaceholder': 'Status text',
  'rules.actionSnoozePlaceholder': 'Minutes',
  'rules.actionProfilePlaceholder': 'Profile name',
  'rules.actionAvailabilityPlaceholder': 'Availability',
  'rules.actionActivityPlaceholder': 'Activity',
  'dashboard.gateWhyTitle': 'Why is the status paused?',
  'dashboard.gateWhyShow': 'Show why',
  'dashboard.gateWhyHide': 'Hide why',
  'dashboard.gateWhyEmpty': 'No active gate right now — the status is syncing normally.',

  'profiles.sectionTitle': 'Presence profiles',
  'profiles.sectionHint':
    'Save a named overlay (status format, gates, rules) and switch with the tray, a hotkey or the CLI.',
  'profiles.empty': 'No profiles defined.',
  'profiles.addProfile': 'Add profile',
  'profiles.removeProfile': 'Remove',
  'profiles.activeProfileLabel': 'Active profile',
  'profiles.activeProfileNone': 'None — use the base configuration',
  'profiles.overlayStatusFormatLabel': 'Status format',
  'profiles.overlayClearOnPauseLabel': 'Clear status when paused',
  'profiles.overlayAvailabilitySyncLabel': 'Sync availability with Teams',
  'profiles.overlayGateOutOfOfficeLabel': 'Gate on Out of Office',
  'profiles.overlayGatePresentingLabel': 'Gate on full-screen apps',
  'profiles.overlayIdleAwayLabel': 'Stop advertising after idle (seconds)',
  'profiles.overlayPreferredPresenceLabel': 'Preferred presence',
  'profiles.overlayRulesLabel': 'Rules subset',
  'profiles.overlayRulesHint': 'Replaces the base rules list while this profile is active.',
  'profiles.overlayNotificationsLabel': 'Notifications',
  'profiles.profileNameLabel': 'Profile name',
  'profiles.profileNamePlaceholder': 'e.g. Focus, Workout',
  'profiles.profileNameDuplicate': 'A profile with this name already exists.',
  'profiles.profileNameTooLong': 'Profile names must be 32 characters or fewer.',
  'profiles.profileNameMissing': 'Profile name cannot be empty.',
  'profiles.activeProfileUnknown': 'Unknown profile — using the base configuration.',
  'tray.profilesMenu': 'Presence profiles',
  'tray.profilesMenuNone': 'No profiles defined',
  'tray.profilesMenuActivateBase': 'Use base configuration',
  'cli.profileFlag': 'Switch to a named presence profile and exit.',
  'cli.profileActive': 'Active profile is now "{name}".',
  'cli.profileUnknown': 'No profile named "{name}" — base configuration remains active.',
  'settings.shortcutToggleProfile': 'Cycle presence profiles',

  // --- 5.0 wave3 playback-source ---
  'onboarding.playbackSourceMacNote':
    "You're on macOS — only the Spotify playback source is available here. The system media-session source (Windows SMTC / Linux MPRIS) is not available on macOS, so PresenceJam falls back to Spotify automatically. Switch to Spotify if the wizard ever reports \"no track\" while music is playing in another app.",
};

export type Dict = { readonly [K in keyof typeof en]: string };
