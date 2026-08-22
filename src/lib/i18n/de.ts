/**
 * C6 i18n foundation (docs/scope-3.3.md §C6) — German dictionary.
 * Typed against `Dict`, so every English key must be present.
 */

import type { Dict } from './en';

export const de: Dict = {
  // ── common ────────────────────────────────────────────────────────
  'common.back': 'Zurück',
  'common.backToDashboard': 'Zurück zum Dashboard',
  'common.checkNow': 'Ich habe mich angemeldet — jetzt prüfen',
  'common.connected': 'Verbunden',
  'common.dismiss': 'Ausblenden',
  'common.goTo': 'Wechseln Sie zu',
  'common.andEnterCode': 'und geben Sie diesen Code ein',
  'common.launchAtLogin': 'Bei der Anmeldung starten',
  'common.loading': 'Wird geladen...',
  'common.notConnected': 'Nicht verbunden',
  'common.reconnecting': 'Neuverbindung…',
  'common.reconnect': 'Erneut verbinden',
  'common.resetToDefault': 'Auf Standard zurücksetzen',
  'common.themeToggle': 'Design umschalten',
  'common.waiting': 'Warten…',
  'common.waitingForSignIn': 'Warten auf Anmeldung…',
  'common.yes': 'Ja',
  'common.no': 'Nein',
  'common.tagline': 'Spotify → Teams-Status',

  // ── dashboard ─────────────────────────────────────────────────────
  'dashboard.spotifyOff': 'Spotify aus',
  'dashboard.teamsOff': 'Teams aus',
  'dashboard.syncing': 'Synchronisiert',
  'dashboard.logsDetachedTitle': 'Protokolle (abgekoppelt — klicken zum Fokussieren)',
  'dashboard.logsTitle': 'Protokolle',
  'dashboard.logsDetachedAria': 'Protokolle (in separatem Fenster abgekoppelt)',
  'dashboard.openLogsAria': 'Protokolle öffnen',
  'dashboard.diagnostics': 'Diagnose',
  'dashboard.openDiagnosticsAria': 'Diagnose öffnen',
  'dashboard.settingsDetachedTitle': 'Einstellungen (abgekoppelt — klicken zum Fokussieren)',
  'dashboard.settings': 'Einstellungen',
  'dashboard.settingsDetachedAria': 'Einstellungen (in separatem Fenster abgekoppelt)',
  'dashboard.openSettingsAria': 'Einstellungen öffnen',
  'dashboard.about': 'Über',
  'dashboard.aboutAria': 'Über PresenceJam',
  'dashboard.pauseSync': 'Synchronisierung pausieren',
  'dashboard.resumeSync': 'Synchronisierung fortsetzen',
  'dashboard.presenceGated': 'Status pausiert, während Sie beschäftigt/in einer Besprechung sind',
  'dashboard.setupRequired': 'Einrichtung erforderlich',
  'dashboard.setupHint':
    'Verbinden Sie Spotify und Microsoft Teams, damit Ihre aktuell gespielten Titel Ihren Teams-Status steuern können.',
  'dashboard.continueSetup': 'Einrichtung fortsetzen',
  'dashboard.playing': 'Spielt',
  'dashboard.paused': 'Pausiert',
  'dashboard.liveStreamAria': 'Live-Stream — Position unbekannt',
  'dashboard.yourTeamsStatus': 'Ihr Teams-Status',
  'dashboard.nothingPlaying': 'Keine Wiedergabe',
  'dashboard.nothingPlayingHint':
    'Starten Sie etwas auf Spotify, und wir leiten es an Teams weiter.',
  'dashboard.syncCrashed':
    'Die Synchronisierung wurde unerwartet beendet. Bitte starten Sie PresenceJam neu.',
  'dashboard.credentialCheckFailed':
    'Anmeldedaten konnten nicht geprüft werden — bitte versuchen Sie es erneut.',
  'dashboard.statusNotConfigured': 'Nicht konfiguriert',
  'dashboard.statusNoTrack': 'Kein Titel wird abgespielt',
  'dashboard.live': 'Live',

  // ── logs ──────────────────────────────────────────────────────────
  'logs.title': 'Protokolle',
  'logs.filterAria': 'Protokollebenen-Filter',
  'logs.level.all': 'Alle',
  'logs.level.trace': 'Trace',
  'logs.level.debug': 'Debug',
  'logs.level.info': 'Info',
  'logs.level.warning': 'Warnung',
  'logs.level.error': 'Fehler',
  'logs.countOne': '{count} Eintrag',
  'logs.countOther': '{count} Einträge',
  'logs.popOut': 'Abkoppeln',
  'logs.clear': 'Leeren',
  'logs.openFolder': 'Ordner öffnen',
  'logs.empty': 'Noch keine Protokolleinträge',
  'logs.emptyHint':
    'Live-Einträge erscheinen hier, während die Abfrageschleife läuft.',

  // ── settings ──────────────────────────────────────────────────────
  'settings.title': 'Einstellungen',
  'settings.popBackIn': 'Wieder andocken',
  'settings.popOutActionTitle': 'In ein eigenes Fenster abkoppeln',
  'settings.unsavedChanges': 'Ungespeicherte Änderungen',
  'settings.sectionSpotify': 'Spotify',
  'settings.sectionTeams': 'Microsoft Teams',
  'settings.sectionPresence': 'Präsenz',
  'settings.sectionStatusFormat': 'Statusformat',
  'settings.sectionPolling': 'Abrufintervall',
  'settings.sectionNotifications': 'Benachrichtigungen',
  'settings.sectionAppearance': 'Erscheinungsbild',
  'settings.clientId': 'Client-ID',
  'settings.clientIdPlaceholder': 'Spotify-Client-ID eingeben',
  'settings.clientSecret': 'Client-Secret',
  'settings.secretStoredHint':
    'Wird sicher im Schlüsselbund Ihres Betriebssystems gespeichert. Zum Ersetzen führen Sie das Onboarding erneut aus.',
  'settings.secretNotConfigured': 'Nicht konfiguriert.',
  'settings.runOnboarding': 'Onboarding starten',
  'settings.toSetUpSpotify': 'um Spotify einzurichten.',
  'settings.reconnectSpotify': 'Spotify neu verbinden',
  'settings.completeAuthInBrowser':
    'Schließen Sie die Authentifizierung im Browser ab.',
  'settings.playbackScopeBanner':
    'Die Wiedergabesteuerung erfordert eine einmalige Neuverbindung.',
  'settings.teamsAuthHint':
    'Die Teams-Authentifizierung verwendet Ihr Microsoft-365-Konto. Keine zusätzliche Konfiguration erforderlich.',
  'settings.presenceScopeBanner':
    'Die Präsenzfunktionen erfordern eine einmalige Teams-Neuverbindung.',
  'settings.availabilitySyncLabel':
    '„Verfügbar“ beim Hören anzeigen',
  'settings.availabilitySyncHint':
    'Standardmäßig aus. Zeigt in Teams „Verfügbar“ (statt „Beschäftigt“), während ein Titel spielt, da setPresence nur die Kombination Busy/InACall unterstützt — siehe setPresence-Einschränkung.',
  'settings.presenceGateLabel':
    'Status bei Besprechungen/Anrufen/Nicht-stören pausieren',
  'settings.presenceGateHint':
    'Standardmäßig an. Überspringt das Schreiben Ihres Spotify-Status, während Teams meldet, dass Sie beschäftigt, in einer Besprechung, in einem Anruf oder beim Präsentieren sind.',
  'settings.formatTemplate': 'Formatvorlage',
  'settings.formatTemplatePlaceholder': '🎵 {artist} - {track} 🎧',
  'settings.livePreview': 'Live-Vorschau',
  'settings.placeholdersHint':
    'Verfügbare Platzhalter: {artist}, {track}, {album}, {emoji}',
  'settings.profanityFilterLabel': 'Vulgärsprache im Status filtern',
  'settings.placeholderTextLabel': 'Platzhaltertext',
  'settings.placeholderTextHint':
    'Verwenden Sie {emoji} für den Wiedergabestatus (🎵 spielt / ⏸ pausiert). Wird angezeigt, wenn vulgäre Sprache in Titelinformationen erkannt wird.',
  'settings.placeholderTextPlaceholder': 'Hört gerade Spotify',
  'settings.defaultIntervalLabel': 'Standardintervall: {seconds}s',
  'settings.minIntervalLabel': 'Mindestintervall (s)',
  'settings.maxIntervalLabel': 'Maximalintervall (s)',
  'settings.clampHint':
    'Das Mindestintervall überschreitet das Maximalintervall — das Maximum wird als {max}s gespeichert.',
  'settings.notificationsToggle':
    'Desktop-Benachrichtigung bei Titelwechsel',
  'settings.notificationsHint':
    'Zeigt eine Systembenachrichtigung, wenn der Titel wechselt. Standardmäßig deaktiviert.',
  'settings.themeLabel': 'Design',
  'settings.themeDark': 'Dunkel',
  'settings.themeLight': 'Hell',
  'settings.languageLabel': 'Sprache',
  'settings.autostartError':
    'Aktualisieren von „Bei der Anmeldung starten“ fehlgeschlagen: {error}',
  'settings.saveChanges': 'Änderungen speichern',
  'settings.saving': 'Speichern…',
  'settings.saved': 'Einstellungen gespeichert!',
  'settings.failedToSave': 'Speichern fehlgeschlagen',
  'settings.openLogsFolder': 'Protokollordner öffnen',
  'settings.previewUnavailable': '(Vorschau nicht verfügbar)',

  // ── diagnostics ───────────────────────────────────────────────────
  'diagnostics.title': 'Diagnose',
  'diagnostics.localOnlyHint':
    'Rein lokale Momentaufnahme — kann gefahrlos einem Fehlerbericht beigelegt werden.',
  'diagnostics.copy': 'Diagnose kopieren',
  'diagnostics.saveToFile': 'In Datei speichern',
  'diagnostics.copied': 'Diagnose in die Zwischenablage kopiert.',
  'diagnostics.copyFailed':
    'Kopieren fehlgeschlagen — verwenden Sie stattdessen „In Datei speichern“.',
  'diagnostics.savedToDownloads':
    'Diagnose in Ihrem Downloads-Ordner gespeichert.',
  'diagnostics.saveFailed':
    'Speichern fehlgeschlagen — verwenden Sie stattdessen „Diagnose kopieren“.',
  'diagnostics.collecting': 'Diagnosedaten werden gesammelt…',
  'diagnostics.collectFailed':
    'Sammeln der Diagnosedaten fehlgeschlagen',
  'diagnostics.versions': 'Versionen',
  'diagnostics.configuration': 'Konfiguration',
  'diagnostics.connections': 'Verbindungen',
  'diagnostics.recentLogLines': 'Letzte Protokollzeilen',
  'diagnostics.app': 'PresenceJam',
  'diagnostics.tauri': 'Tauri',
  'diagnostics.os': 'Betriebssystem',
  'diagnostics.spotifyClientId': 'Spotify-Client-ID',
  'diagnostics.redirectUri': 'Redirect-URI',
  'diagnostics.notSet': '(nicht gesetzt)',
  'diagnostics.clientSecretKeychain': 'Client-Secret im Schlüsselbund',
  'diagnostics.clearOnPause': 'Status bei Pause löschen',
  'diagnostics.profanityFilter': 'Vulgärsprachen-Filter',
  'diagnostics.startMinimized': 'Minimiert starten',
  'diagnostics.availabilitySync': 'Teams-Verfügbarkeits-Sync',
  'diagnostics.presenceGate': 'Präsenz-Sperre',
  'diagnostics.pollInterval': 'Abrufintervall (Standard/min/max)',
  'diagnostics.logging': 'Protokollierung',
  'diagnostics.loggingEnabled': 'aktiviert ({level})',
  'diagnostics.loggingDisabled': 'deaktiviert',
  'diagnostics.launchAtLogin': 'Bei der Anmeldung starten',
  'diagnostics.spotifyConnected': 'Spotify verbunden',
  'diagnostics.spotifyTokenExpires': 'Spotify-Token läuft ab',
  'diagnostics.teamsConnected': 'Teams verbunden',
  'diagnostics.teamsTokenExpires': 'Teams-Token läuft ab',
  'diagnostics.expired': '(abgelaufen)',
  'diagnostics.keychainSpotifySecret':
    'Schlüsselbund: Spotify-Secret vorhanden',
  'diagnostics.keychainEncryptionKey':
    'Schlüsselbund: Token-Verschlüsselungsschlüssel vorhanden',
  'diagnostics.tokensNeverIncluded':
    'Token-Werte sind nie enthalten — nur Ablaufzeitstempel und Präsenzflags.',
  'diagnostics.noLogLinesYet': 'Noch keine Protokollzeilen verfügbar.',

  // ── reconnect ─────────────────────────────────────────────────────
  'reconnect.title': 'Erneut verbinden',
  'reconnect.description':
    'Ihre Sitzung ist abgelaufen. Verbinden Sie sich unten neu, um die Synchronisierung fortzusetzen.',
  'reconnect.missingCredentials': 'Fehlende Anmeldedaten',
  'reconnect.failed': 'Fehlgeschlagen',
  'reconnect.readyToReconnect': 'Bereit zur Neuverbindung',
  'reconnect.needsReconnect': 'Neuverbindung erforderlich',
  'reconnect.spotifyOk': 'Spotify erfolgreich neu verbunden.',
  'reconnect.spotifyNotConfigured':
    'Spotify-Anmeldedaten sind auf diesem Rechner nicht konfiguriert.',
  'reconnect.completeAuthInOpenedBrowser':
    'Schließen Sie die Authentifizierung im geöffneten Browserfenster ab.',
  'reconnect.tryAgain': 'Erneut versuchen',
  'reconnect.clickBelowSpotify':
    'Klicken Sie unten, um Ihr Spotify-Konto neu zu verbinden.',
  'reconnect.teamsOk': 'Teams erfolgreich neu verbunden.',
  'reconnect.clickBelowTeams':
    'Klicken Sie unten, um Ihr Microsoft-Teams-Konto neu zu verbinden.',
  'reconnect.missingCredsTitle': 'Spotify-Anmeldedaten vergessen?',
  'reconnect.reenterCredsHint':
    'Sie müssen Ihre Client-ID und Ihr Client-Secret erneut eingeben.',
  'reconnect.goToFullSetup': 'Zur vollständigen Einrichtung',
  'reconnect.reconnectSpotify': 'Spotify neu verbinden',
  'reconnect.reconnectTeams': 'Teams neu verbinden',

  // ── about ─────────────────────────────────────────────────────────
  'about.version': 'Version {version}',
  'about.description':
    'Leitet weiter, was Sie auf Spotify hören, automatisch in Ihren Microsoft-Teams-Status.',
  'about.statusSync': 'Status-Sync',
  'about.live': 'Live',
  'about.auth': 'Authentifizierung',
  'about.authMethod': 'PKCE / Gerätecode',
  'about.storage': 'Speicher',
  'about.osKeychain': 'Betriebssystem-Schlüsselbund',
  'about.githubRepo': 'GitHub-Repository',
  'about.releases': 'Veröffentlichungen',
  'about.reportIssue': 'Problem melden',

  // ── update banner ─────────────────────────────────────────────────
  'update.available': 'Update v{version} verfügbar',
  'update.stagedQuit':
    'v{version} wird installiert, wenn Sie PresenceJam beenden',
  'update.downloadFailed': 'Download fehlgeschlagen — {error}',
  'update.downloadAndInstall': 'Herunterladen & installieren',
  'update.downloading': 'Wird heruntergeladen…',
  'update.installOnQuit': 'Beim Beenden installieren',
  'update.preparing': 'Wird vorbereitet…',
  'update.dismissAria': 'Update-Banner ausblenden',

  // ── onboarding ────────────────────────────────────────────────────
  'onboarding.stepOf': 'Schritt {step} von 3',
  'onboarding.step1Title': 'Spotify verbinden',
  'onboarding.step1Intro':
    'Fügen Sie die Client-ID und das Client-Secret Ihrer Spotify-Anwendung ein. Nach dem Klick auf die Schaltfläche starten wir den Anmeldevorgang.',
  'onboarding.getCredentials': 'Ihre Spotify-Anmeldedaten abrufen',
  'onboarding.instruction1':
    'Öffnen Sie das Spotify-Entwickler-Dashboard und erstellen Sie eine App.',
  'onboarding.instruction2':
    'Fügen Sie presencejam://callback als Redirect-URI hinzu.',
  'onboarding.instruction3':
    'Kopieren Sie die Client-ID und das Client-Secret aus den App-Einstellungen.',
  'onboarding.clientIdPlaceholder': '32-stellige Spotify-Client-ID',
  'onboarding.clientSecretPlaceholder': 'Spotify-Client-Secret',
  'onboarding.connectSpotify': 'Spotify verbinden',
  'onboarding.signInWaiting': 'Spotify-Anmeldung wartet…',
  'onboarding.manualUrlHint':
    'Schließen Sie die Autorisierung in Ihrem Browser ab, oder fügen Sie die Weiterleitungs-URL unten ein.',
  'onboarding.submitCode': 'Code übermitteln',
  'onboarding.connectedToSpotify': 'Mit Spotify verbunden',
  'onboarding.continue': 'Weiter →',
  'onboarding.step2Title': 'Mit Microsoft anmelden',
  'onboarding.step2Intro':
    'Wir verwenden den Gerätecode-Flow von Microsoft — einen einmaligen Code, den Sie auf einer Microsoft-Seite eingeben. Keine zusätzliche Einrichtung nötig.',
  'onboarding.startMicrosoftSignIn': 'Microsoft-Anmeldung starten',
  'onboarding.connectedToTeams': 'Mit Microsoft Teams verbunden',
  'onboarding.step3Title': 'Letzte Schritte',
  'onboarding.step3Intro':
    'Wählen Sie, wie Ihre Statusmeldung aussehen soll und ob PresenceJam bei Ihrer Anmeldung gestartet werden soll.',
  'onboarding.statusTemplate': 'Statusvorlage',
  'onboarding.placeholdersHint':
    'Platzhalter: {artist}, {track}, {album}, {emoji}',
  'onboarding.pollInterval': 'Standard-Abrufintervall: {seconds}s',
  'onboarding.settingUp': 'Einrichtung läuft…',
  'onboarding.finishSetup': 'Einrichtung abschließen',

  // ── validation / errors ───────────────────────────────────────────
  'validation.clientIdRequired': 'Die Spotify-Client-ID ist erforderlich.',
  'validation.clientIdFormat':
    'Die Spotify-Client-ID muss genau 32 hexadezimale Zeichen lang sein.',
  'validation.clientSecretRequired':
    'Das Spotify-Client-Secret ist erforderlich.',
  'validation.clientSecretTooShort':
    'Das Spotify-Client-Secret scheint ungültig zu sein (zu kurz — mindestens 32 Zeichen).',
  'validation.noCodeInUrl':
    'Kein Code in der URL gefunden — fügen Sie die vollständige Weiterleitungs-URL mit ?code=… ein.',
  'validation.connectBothFirst':
    'Bitte verbinden Sie sowohl Spotify als auch Teams, bevor Sie die Einrichtung abschließen.',
  'validation.setupFailed': 'Einrichtung fehlgeschlagen: {error}',

  // ── routes / chrome ───────────────────────────────────────────────
  'routes.skipToMainContent': 'Zum Hauptinhalt springen',
  'routes.unknownPane': 'Unbekannter Bereich: {pane}'
};
