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
  'common.checkNow': "I've signed in — check now",
  'common.connected': 'Connected',
  'common.dismiss': 'Dismiss',
  'common.goTo': 'Go to',
  'common.andEnterCode': 'and enter this code',
  'common.launchAtLogin': 'Launch at login',
  'common.loading': 'Loading...',
  'common.notConnected': 'Not connected',
  'common.reconnecting': 'Reconnecting…',
  'common.reconnect': 'Reconnect',
  'common.resetToDefault': 'Reset to default',
  'common.themeToggle': 'Toggle theme',
  'common.waiting': 'Waiting…',
  'common.waitingForSignIn': 'Waiting for sign-in…',
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
  'dashboard.presenceGated': "Status paused while you're busy/in a meeting",
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
    "Start something on Spotify and we'll pipe it through to Teams.",
  'dashboard.syncCrashed': 'Sync stopped unexpectedly. Please restart PresenceJam.',
  'dashboard.credentialCheckFailed': 'Unable to check credentials — please try again.',
  'dashboard.statusNotConfigured': 'Not configured',
  'dashboard.statusNoTrack': 'No track playing',
  'dashboard.live': 'Live',

  // ── logs ──────────────────────────────────────────────────────────
  'logs.title': 'Logs',
  'logs.filterAria': 'Log level filter',
  'logs.level.all': 'All',
  'logs.level.trace': 'Trace',
  'logs.level.debug': 'Debug',
  'logs.level.info': 'Info',
  'logs.level.warning': 'Warning',
  'logs.level.error': 'Error',
  'logs.countOne': '{count} entry',
  'logs.countOther': '{count} entries',
  'logs.popOut': 'Pop out',
  'logs.clear': 'Clear',
  'logs.openFolder': 'Open folder',
  'logs.empty': 'No log entries yet',
  'logs.emptyHint': 'Live entries stream here as the polling loop runs.',

  // ── settings ──────────────────────────────────────────────────────
  'settings.title': 'Settings',
  'settings.popBackIn': 'Pop back in',
  'settings.popOutActionTitle': 'Pop out into its own window',
  'settings.unsavedChanges': 'Unsaved changes',
  'settings.sectionSpotify': 'Spotify',
  'settings.sectionTeams': 'Microsoft Teams',
  'settings.sectionPresence': 'Presence',
  'settings.sectionStatusFormat': 'Status format',
  'settings.sectionPolling': 'Polling',
  'settings.sectionNotifications': 'Notifications',
  'settings.sectionAppearance': 'Appearance',
  'settings.clientId': 'Client ID',
  'settings.clientIdPlaceholder': 'Enter Spotify Client ID',
  'settings.clientSecret': 'Client secret',
  'settings.secretStoredHint':
    "Stored securely in your operating system's keychain. To replace it, run Onboarding again.",
  'settings.secretNotConfigured': 'Not configured.',
  'settings.runOnboarding': 'Run Onboarding',
  'settings.toSetUpSpotify': 'to set up Spotify.',
  'settings.reconnectSpotify': 'Reconnect Spotify',
  'settings.completeAuthInBrowser': 'Complete authentication in the browser.',
  'settings.playbackScopeBanner': 'Playback control needs a one-time reconnect.',
  'settings.teamsAuthHint':
    'Teams authentication uses your Microsoft 365 account. No additional configuration required.',
  'settings.presenceScopeBanner': 'Presence features need a one-time Teams reconnect.',
  'settings.availabilitySyncLabel': 'Show Available while listening',
  'settings.availabilitySyncHint':
    'Off by default. Shows Available (not Busy) in Teams while a track plays, because setPresence only supports the Busy/InACall combination — see the setPresence limitation.',
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
  'settings.saved': 'Settings saved!',
  'settings.failedToSave': 'Failed to save',
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

  // ── reconnect ─────────────────────────────────────────────────────
  'reconnect.title': 'Reconnect',
  'reconnect.description': 'Your session expired. Reconnect below to resume syncing.',
  'reconnect.missingCredentials': 'Missing credentials',
  'reconnect.failed': 'Failed',
  'reconnect.readyToReconnect': 'Ready to reconnect',
  'reconnect.needsReconnect': 'Needs reconnect',
  'reconnect.spotifyOk': 'Spotify reconnected successfully.',
  'reconnect.spotifyNotConfigured':
    'Spotify credentials are not configured on this machine.',
  'reconnect.completeAuthInOpenedBrowser':
    'Complete authentication in the opened browser window.',
  'reconnect.tryAgain': 'Try again',
  'reconnect.reconnectSpotify': 'Reconnect Spotify',
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
    "Pipes what you're playing on Spotify into your Microsoft Teams status — automatically.",
  'about.statusSync': 'Status sync',
  'about.live': 'Live',
  'about.auth': 'Auth',
  'about.authMethod': 'PKCE / Device Code',
  'about.storage': 'Storage',
  'about.osKeychain': 'OS keychain',
  'about.githubRepo': 'GitHub repository',
  'about.releases': 'Releases',
  'about.reportIssue': 'Report an issue',

  // ── update banner ─────────────────────────────────────────────────
  'update.available': 'Update v{version} available',
  'update.stagedQuit': 'v{version} will be installed when you quit PresenceJam',
  'update.downloadFailed': 'Download failed — {error}',
  'update.downloadAndInstall': 'Download & Install',
  'update.downloading': 'Downloading…',
  'update.installOnQuit': 'Install on quit',
  'update.preparing': 'Preparing…',
  'update.dismissAria': 'Dismiss update banner',

  // ── onboarding ────────────────────────────────────────────────────
  'onboarding.stepOf': 'Step {step} of 3',
  'onboarding.step1Title': 'Connect Spotify',
  'onboarding.step1Intro':
    "Paste your Spotify application's Client ID and Client Secret. We'll start the sign-in flow once you click the button.",
  'onboarding.getCredentials': 'Get your Spotify credentials',
  'onboarding.instruction1':
    'Open the Spotify developer dashboard and create an app.',
  'onboarding.instruction2': 'Add presencejam://callback as a redirect URI.',
  'onboarding.instruction3':
    "Copy the Client ID and Client Secret from the app's settings.",
  'onboarding.clientIdPlaceholder': '32-character Spotify Client ID',
  'onboarding.clientSecretPlaceholder': 'Spotify Client Secret',
  'onboarding.connectSpotify': 'Connect Spotify',
  'onboarding.signInWaiting': 'Spotify sign-in is waiting…',
  'onboarding.manualUrlHint':
    'Complete the authorisation in your browser, or paste the redirect URL below.',
  'onboarding.submitCode': 'Submit code',
  'onboarding.connectedToSpotify': 'Connected to Spotify',
  'onboarding.continue': 'Continue →',
  'onboarding.step2Title': 'Sign in with Microsoft',
  'onboarding.step2Intro':
    "We use Microsoft's device-code flow — a one-time code you enter at a Microsoft page. No extra setup required.",
  'onboarding.startMicrosoftSignIn': 'Start Microsoft sign-in',
  'onboarding.connectedToTeams': 'Connected to Microsoft Teams',
  'onboarding.step3Title': 'Finishing touches',
  'onboarding.step3Intro':
    'Choose how your status message should look and whether PresenceJam should launch when you sign in.',
  'onboarding.statusTemplate': 'Status template',
  'onboarding.placeholdersHint': 'Placeholders: {artist}, {track}, {album}, {emoji}',
  'onboarding.pollInterval': 'Default poll interval: {seconds}s',
  'onboarding.settingUp': 'Setting up…',
  'onboarding.finishSetup': 'Finish setup',

  // ── validation / errors ───────────────────────────────────────────
  'validation.clientIdRequired': 'Spotify Client ID is required.',
  'validation.clientIdFormat':
    'Spotify Client ID must be exactly 32 hexadecimal characters.',
  'validation.clientSecretRequired': 'Spotify Client Secret is required.',
  'validation.clientSecretTooShort':
    'Spotify Client Secret appears to be invalid (too short — must be at least 32 characters).',
  'validation.noCodeInUrl':
    'No code found in URL — paste the full redirect URL with ?code=…',
  'validation.connectBothFirst':
    'Please connect both Spotify and Teams before finishing setup.',
  'validation.setupFailed': 'Setup failed: {error}',

  // ── routes / chrome ───────────────────────────────────────────────
  'routes.skipToMainContent': 'Skip to main content',
  'routes.unknownPane': 'Unknown pane: {pane}'
};

export type Dict = { readonly [K in keyof typeof en]: string };
