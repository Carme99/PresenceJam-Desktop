/**
 * C6 i18n foundation (docs/scope-3.3.md §C6) — German dictionary.
 * Typed against `Dict`, so every English key must be present.
 */

import type { Dict } from './en';

export const de: Dict = {
  // ── common ────────────────────────────────────────────────────────
  'common.back': 'Zurück',
  'common.backToDashboard': 'Zurück zum Dashboard',
  'common.checkNow': 'Anmeldestatus prüfen',
  'common.connected': 'Verbunden',
  'common.dismiss': 'Ausblenden',
  'common.goTo': 'Wechseln Sie zu',
  'common.andEnterCode': 'und geben Sie diesen Code ein',
  'common.openSignInPage': 'Öffnen Sie die Microsoft-Anmeldeseite:',
  'common.enterCodeWhenAsked': 'Geben Sie diesen Code ein, wenn Sie dazu aufgefordert werden:',
  'common.moreActions': 'Weitere Aktionen',
  'common.launchAtLogin': 'Bei der Anmeldung starten',
  'common.loading': 'Wird geladen...',
  'common.notConnected': 'Nicht verbunden',
  'common.reconnecting': 'Neuverbindung…',
  'common.reconnect': 'Erneut verbinden',
  'common.retry': 'Wiederholen',
  'common.bootFailed': 'App-Status konnte nicht geladen werden.',
  'common.resetToDefault': 'Auf Standard zurücksetzen',
  'common.themeToggle': 'Design umschalten',
  'common.waiting': 'Warten…',
  'common.waitingForSignIn': 'Warten auf Anmeldung…',
  // Best-effort translation (no native review yet) — see wave/slice-e PR body.
  'common.codeExpiresIn': 'Code läuft in {time} ab',
  'common.codeExpired': 'Dieser Code ist abgelaufen — er kann nicht mehr verwendet werden.',
  'common.getNewCode': 'Neuen Code anfordern',
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
  'dashboard.presenceGated': 'Status pausiert — Sie sind beschäftigt, in einem Anruf oder präsentieren',
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
    'Spielen Sie etwas auf Spotify ab — es erscheint in Ihrem Teams-Status.',
  'dashboard.syncCrashed':
    'Die Synchronisierung wurde unerwartet beendet. Drücken Sie Fortsetzen (▶), um sie neu zu starten.',
  'dashboard.credentialCheckFailed':
    'Ihre Anmeldedaten konnten nicht geprüft werden. Prüfen Sie Ihre Verbindung und versuchen Sie es erneut.',
  'dashboard.syncToggleFailed':
    'Die Synchronisierung konnte nicht gestartet/gestoppt werden. Versuchen Sie es erneut — bleibt das Problem, öffnen Sie die Diagnose über die Dashboard-Kopfzeile.',
  'dashboard.statusNotConfigured': 'Nicht konfiguriert',
  'dashboard.statusNoTrack': 'Kein Titel wird abgespielt',
  'dashboard.live': 'Live',
  'dashboard.refreshStatus': 'Status aktualisieren',
  'dashboard.refreshing': 'Wird aktualisiert…',
  'dashboard.refreshFailed':
    'Status konnte nicht aktualisiert werden. Bitte versuchen Sie es erneut.',
  'dashboard.refreshAria': 'Teams-Status jetzt aktualisieren',

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
  'logs.showingOf': '{shown} von {total} angezeigt',
  'logs.jumpToLatest': 'Zum Neuesten springen',
  'logs.popOut': 'Abkoppeln',
  'logs.clear': 'Leeren',
  'logs.openFolder': 'Ordner öffnen',
  'logs.empty': 'Noch keine Protokolleinträge',
  'logs.emptyHint':
    'Live-Einträge erscheinen hier, sobald die Synchronisierung läuft und Spotify spielt.',

  // ── settings ──────────────────────────────────────────────────────
  'settings.title': 'Einstellungen',
  'settings.popBackIn': 'Wieder andocken',
  'settings.popOutActionTitle': 'In ein eigenes Fenster abkoppeln',
  'settings.unsavedChanges': 'Ungespeicherte Änderungen',
  'settings.sectionSpotify': 'Spotify',
  'settings.sectionTeams': 'Microsoft Teams',
  'settings.sectionPresence': 'Präsenz',
  'settings.sectionStatusFormat': 'Statusformat',
  'settings.sectionPolling': 'Sync-Häufigkeit',
  'settings.sectionNotifications': 'Benachrichtigungen',
  'settings.sectionAppearance': 'Erscheinungsbild',
  'settings.clientId': 'Client-ID',
  'settings.clientIdPlaceholder': 'Spotify-Client-ID eingeben',
  'settings.clientSecret': 'Client-Secret',
  'settings.secretStoredHint':
    'Wird sicher im Schlüsselbund Ihres Betriebssystems gespeichert. Zum Ersetzen kehren Sie zum Dashboard zurück und wählen Sie „Einrichtung fortsetzen“.',
  'settings.secretNotConfigured': 'Nicht konfiguriert.',
  'settings.runOnboarding': 'Onboarding starten',
  'settings.toSetUpSpotify': 'um Spotify einzurichten.',
  'settings.reconnectSpotify': 'Spotify neu verbinden',
  'settings.completeAuthInBrowser':
    'Schließen Sie die Authentifizierung im Browser ab.',
  'settings.playbackScopeBanner':
    'Spotify hat Wiedergabesteuerungen hinzugefügt. Klicken Sie neben dieser Meldung auf „Erneut verbinden“, um sie zu aktivieren.',
  'settings.spotifySecretConflict':
    'Das Client-Secret in der Konfigurationsdatei weicht vom Schlüsselbund-Eintrag ab. Spotify neu verbinden, um das Problem zu beheben.',
  'settings.teamsAuthHint':
    'Die Teams-Authentifizierung verwendet Ihr Microsoft-365-Konto. Keine zusätzliche Konfiguration erforderlich.',
  'settings.presenceScopeBanner':
    'Teams hat Besprechungs-/Anruferkennung hinzugefügt. Klicken Sie neben dieser Meldung auf „Erneut verbinden“, um sie zu aktivieren.',
  'settings.availabilitySyncLabel':
    '„Verfügbar“ beim Hören anzeigen',
  'settings.availabilitySyncHint':
    'Standardmäßig aus. Wenn aktiviert, zeigt Teams „Verfügbar“ (statt „Beschäftigt“), während Musik spielt. Hinweis: Bei Anrufen und Besprechungen zeigt Teams weiterhin „Beschäftigt“.',
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
  'settings.profaneSampleToggle': 'Vorschau mit vulgärem Beispieltitel',
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
  'settings.saved': 'Einstellungen gespeichert.',
  'settings.failedToSave': 'Einstellungen konnten nicht gespeichert werden — Ihre Eingaben sind erhalten. Versuchen Sie es erneut.',
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
  'diagnostics.failedInstallTitle': 'Fehlgeschlagene Update-Installation',
  'diagnostics.failedInstallVersion': 'Version',
  'diagnostics.failedInstallError': 'Fehler',
  'diagnostics.failedInstallTimestamp': 'Versuch um',
  'diagnostics.failedInstallDismissFailed':
    'Der Eintrag zur fehlgeschlagenen Installation konnte nicht verworfen werden.',

  // ── reconnect ─────────────────────────────────────────────────────
  'reconnect.title': 'Erneut verbinden',
  'reconnect.description':
    'Die Synchronisierung braucht Ihre Aufmerksamkeit. Verbinden Sie sich unten neu, um fortzufahren.',
  'reconnect.missingCredentials': 'Fehlende Anmeldedaten',
  'reconnect.failed': 'Fehlgeschlagen',
  'reconnect.readyToReconnect': 'Bereit zur Neuverbindung',
  'reconnect.needsReconnect': 'Bereit zur Neuverbindung',
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
    'Zeigt, was Sie auf Spotify hören, automatisch in Ihrem Microsoft-Teams-Status.',
  'about.statusSync': 'Status-Sync',
  'about.live': 'Live',
  'about.auth': 'Anmeldung',
  'about.authMethod': 'Spotify + Microsoft',
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
  'update.downloadAndInstall': 'Herunterladen und installieren',
  'update.downloading': 'Wird heruntergeladen…',
  'update.installOnQuit': 'Beim Beenden installieren',
  'update.preparing': 'Wird vorbereitet…',
  'update.dismissAria': 'Update-Banner ausblenden',
  'update.confirmQuitInstall':
    'v{staged} beim Beenden installieren? Aktuelle Version: v{current}.',
  'update.confirmQuitInstallUnknown': 'v{staged} beim Beenden installieren?',
  'update.stagedVsCurrent':
    'v{staged} wird beim Beenden installiert (aktuell v{current})',
  'update.staleSkipped':
    'v{staged} wurde übersprungen — Ihre aktuelle v{current} ist neuer.',
  'update.staleSkippedUnknown':
    'v{staged} wurde übersprungen — es ist nicht neuer als Ihre aktuelle Version.',
  'update.installAnyway': 'Trotzdem installieren',

  // ── onboarding ────────────────────────────────────────────────────
  'onboarding.stepOf': 'Schritt {step} von 3',
  'onboarding.step1Title': 'Spotify verbinden',
  'onboarding.step1Intro':
    'Fügen Sie unten Ihre Spotify-Client-ID und Ihr Client-Secret ein und wählen Sie dann „Spotify verbinden“ — wir öffnen die Spotify-Anmeldeseite.',
  'onboarding.getCredentials': 'Ihre Spotify-Anmeldedaten abrufen',
  'onboarding.instruction1':
    'Öffnen Sie das Spotify-Entwickler-Dashboard und erstellen Sie eine App.',
  'onboarding.instruction2':
    'Fügen Sie unter „Redirect URIs“ presencejam://callback hinzu (damit weiß Spotify, wohin Sie zurückkehren).',
  'onboarding.instruction3':
    'Kopieren Sie die Client-ID und das Client-Secret aus den App-Einstellungen.',
  'onboarding.clientIdPlaceholder': '32-stellige Spotify-Client-ID',
  'onboarding.clientSecretPlaceholder': 'Spotify-Client-Secret',
  'onboarding.connectSpotify': 'Spotify verbinden',
  'onboarding.signInWaiting': 'Spotify-Anmeldung wartet…',
  'onboarding.manualUrlHint':
    'Schließen Sie die Anmeldung bei Spotify in Ihrem Browser ab und fügen Sie unten die vollständige Adresse aus der Adressleiste ein.',
  'onboarding.manualUrlLabel': 'Spotify-Weiterleitungs-URL',
  'onboarding.submitCode': 'Code übermitteln',
  'onboarding.connectedToSpotify': 'Mit Spotify verbunden',
  'onboarding.continue': 'Weiter →',
  'onboarding.step2Title': 'Microsoft Teams verbinden',
  'onboarding.step2Intro':
    'Wir verwenden den Gerätecode-Flow von Microsoft — einen einmaligen Code, den Sie auf einer Microsoft-Seite eingeben. Keine zusätzliche Einrichtung nötig.',
  'onboarding.startMicrosoftSignIn': 'Microsoft Teams verbinden',
  'onboarding.connectedToTeams': 'Mit Microsoft Teams verbunden',
  'onboarding.step3Title': 'Letzte Schritte',
  'onboarding.step3Intro':
    'Wählen Sie, wie Ihre Statusmeldung aussehen soll und ob PresenceJam bei Ihrer Anmeldung gestartet werden soll.',
  'onboarding.statusTemplate': 'Statusvorlage',
  'onboarding.placeholdersHint':
    'Platzhalter: {artist}, {track}, {album}, {emoji}',
  'onboarding.pollInterval': 'Wie oft Spotify geprüft wird: {seconds}s',
  'onboarding.settingUp': 'Einrichtung läuft…',
  'onboarding.finishSetup': 'Einrichtung abschließen',

  // ── validation / errors ───────────────────────────────────────────
  'validation.clientIdRequired': 'Die Spotify-Client-ID ist erforderlich.',
  'validation.clientIdFormat':
    'Die Spotify-Client-ID muss genau 32 hexadezimale Zeichen lang sein.',
  'validation.clientSecretRequired':
    'Das Spotify-Client-Secret ist erforderlich.',
  'validation.clientSecretTooShort':
    'Dieses Client-Secret sieht zu kurz aus — es sollte mindestens 32 Zeichen haben. Prüfen Sie, ob beim Kopieren etwas fehlt.',
  'validation.noCodeInUrl':
    'Diese URL enthält keinen Anmeldecode — fügen Sie die vollständige Adresse aus der Adressleiste Ihres Browsers ein, nachdem Spotify Sie weitergeleitet hat.',
  'validation.connectBothFirst':
    'Bitte verbinden Sie sowohl Spotify als auch Teams, bevor Sie die Einrichtung abschließen.',
  'validation.setupFailed': 'Einrichtung fehlgeschlagen: {error}',

  // ── routes / chrome ───────────────────────────────────────────────
  'routes.skipToMainContent': 'Zum Hauptinhalt springen',
  'routes.unknownPane': 'Unbekannter Bereich: {pane}'
};
