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
  'common.openSignInPage': 'Öffnen Sie die Microsoft-Anmeldeseite:',
  'common.enterCodeWhenAsked': 'Geben Sie diesen Code ein, wenn Sie dazu aufgefordert werden:',
  'common.moreActions': 'Weitere Aktionen',
  'common.launchAtLogin': 'Bei der Anmeldung starten',
  'common.loading': 'Wird geladen…',
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
  'logs.count_one': '{count} Eintrag',
  'logs.count_other': '{count} Einträge',
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
    'Verfügbare Platzhalter: {artist}, {track}, {album}, {emoji}, {device}, {playlist} (oder {context}), {progress}, {shuffle}, {repeat}. Shuffle und Repeat zeigen 🔀/🔁 nur, wenn sie aktiv sind.',
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
  'settings.notificationsHint':
    'Jede aktivierte Klasse zeigt eine Systembenachrichtigung — beim ersten Mal fragt das System nach der Berechtigung.',
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
  'diagnostics.statusRules': 'Statusregeln',
  'diagnostics.statusRulesValue': '{quiet}/{quietTotal} Ruhezeiten, {rules}/{rulesTotal} Track-Regeln aktiv',
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
  'onboarding.manualUrlPlaceholder': 'presencejam://callback?code=…',
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
    'Platzhalter: {artist}, {track}, {album}, {emoji}, {device}, {playlist} (oder {context}), {progress}, {shuffle}, {repeat}',
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
  'routes.unknownPane': 'Unbekannter Bereich: {pane}',

  // ── feat/45-features: status rules (#432) + support snapshot (#434) ──
  'rules.sectionTitle': 'Statusregeln',
  'rules.sectionHint':
    'Ruhezeiten und Track-Regeln unterdrücken die Teams-Statusmeldung über denselben Anwesenheits-Gate-Pfad — eine abgelaufene Regel wird automatisch mitten im Track gepostet.',
  'rules.quietHoursLabel': 'Ruhezeiten',
  'rules.noQuietHours': 'Keine Ruhezeiten definiert — der Status wird zu allen Zeiten synchronisiert.',
  'rules.quietStart': 'Beginn der Ruhezeit',
  'rules.quietEnd': 'Ende der Ruhezeit',
  'rules.quietDays': 'Aktive Tage (nichts gewählt = jeden Tag)',
  'rules.day1': 'Mo',
  'rules.day2': 'Di',
  'rules.day3': 'Mi',
  'rules.day4': 'Do',
  'rules.day5': 'Fr',
  'rules.day6': 'Sa',
  'rules.day7': 'So',
  'rules.addQuietHours': 'Ruhezeit hinzufügen',
  'rules.trackRulesLabel': 'Track-Regeln',
  'rules.noTrackRules': 'Keine Track-Regeln definiert — alle Tracks werden normal synchronisiert.',
  'rules.artistPlaceholder': 'Interpret enthält…',
  'rules.trackPlaceholder': 'Titel enthält…',
  'rules.replacementPlaceholder': 'Stattdessen posten (leer = unterdrücken)',
  'rules.addTrackRule': 'Track-Regel hinzufügen',
  'rules.removeRule': 'Entfernen',
  'rules.ruleEnabled': 'Aktiviert',
  'logs.copySnapshot': 'Snapshot kopieren',
  'logs.snapshotCopied': 'Geschwärzter Snapshot in die Zwischenablage kopiert.',
  'logs.snapshotCopyFailed': 'Der Snapshot konnte nicht kopiert werden.',
  // 4.6 additions
  'dashboard.availabilityListening': 'Wird gehört (verfügbar)',
  'dashboard.availabilityCleared': 'Verfügbarkeit zurückgesetzt',
  'settings.saveAndLeave': 'Speichern und verlassen',
  'settings.discardChanges': 'Änderungen verwerfen',
  'settings.stayHere': 'Hier bleiben',
  'settings.notificationsDenied':
    'Benachrichtigungen sind im System blockiert. Erlauben Sie sie in den Systemeinstellungen und aktivieren Sie die Option erneut.',
  'diagnostics.expiryBuffer': 'Puffer vor Token-Ablauf',
  'diagnostics.teamsRefreshTokenPresent': 'Teams-Aktualisierungstoken gespeichert',
  'reconnect.restartSignIn': 'Anmeldung neu starten',
  'update.stagingProgress': 'Update wird vorbereitet — {percent} %',
  'update.cancelStage': 'Abbrechen',
  'onboarding.submitting': 'Wird übermittelt…',
  'settings.episodeFormatHint':
    'Podcasts und Hörbücher nutzen eine eigene Vorlage – 🎙️ {show} - {episode} – Ihre Musikvorlage oben wird dafür nicht verwendet.',
  'reconnect.keychainUnavailableBadge':
    'Schlüsselbund nicht verfügbar',
  'reconnect.keychainUnavailableHint':
    'PresenceJam konnte Ihr gespeichertes Spotify-Client-Geheimnis nicht lesen: Der Systemschlüsselbund ist gesperrt oder fehlt. Entsperren Sie ihn (oder installieren Sie einen Secret-Service-Anbieter wie gnome-keyring) und versuchen Sie es erneut — das Geheimnis ist weiterhin gespeichert, Sie müssen Spotify also nicht erneut einrichten.',
  'settings.secretKeychainUnavailable':
    'Systemschlüsselbund nicht verfügbar — er ist möglicherweise gesperrt oder fehlt. Entsperren Sie ihn (oder installieren Sie einen Secret-Service-Anbieter), um Ihr gespeichertes Geheimnis zu verwenden; es ist weiterhin gespeichert.',
  'diagnostics.quarantineTitle': 'Einstellungen wurden zurückgesetzt',
  'diagnostics.quarantineBodyNow':
    'PresenceJam konnte Ihre Einstellungsdatei nicht lesen und hat deshalb jede Einstellung auf ihren Standardwert zurückgesetzt.',
  'diagnostics.quarantineBodyEarlier':
    'Ein früherer Start konnte Ihre Einstellungsdatei nicht lesen und hat sie auf ihre Standardwerte zurückgesetzt.',
  'diagnostics.quarantineBackupPresent':
    'Das unlesbare Original wurde neben Ihrer Einstellungsdatei als {name} aufbewahrt, sodass die darin enthaltenen Werte wiederhergestellt werden können.',
  'diagnostics.quarantineBackupMissing':
    'Das unlesbare Original liegt weiterhin neben Ihrer Einstellungsdatei als config.json.',
  'diagnostics.quarantineWhere':
    'Beide Dateien liegen im Ordner PresenceJam in Ihrem Benutzer-Konfigurationsordner – die Sicherung neben config.json.',
  // 4.6 additions (presence rules #634/#635/#636/#637 + #538 consumption sites)
  'rules.replacementClampHint':
    'Der Ersatzstatus ist auf {max} Zeichen begrenzt — {kept} werden gepostet.',
  'rules.presenceLabel': 'Präsenz, solange diese Regel gilt',
  'rules.presenceNone': 'Präsenz nicht ändern',
  'rules.presenceHint':
    'Eine Regel kann die Teams-Verfügbarkeit/-Aktivität setzen, aber nur wenn „Verfügbarkeits-Sync“ aktiv ist; sie überschreibt nie einen Anruf, ein Meeting oder einen selbst gesetzten Status.',
  'settings.respectManualStatusLabel': 'Einen selbst gesetzten Status nie überschreiben',
  'settings.respectManualStatusHint':
    'Nutzt die Präsenzabfrage, die das Gate ohnehin durchführt — kein zusätzlicher Aufruf. Der Text bleibt erhalten, bis Sie ihn ändern oder er abläuft.',
  'settings.gateOutOfOfficeLabel': 'Pausieren, wenn ich abwesend bin',
  'settings.gateOutOfOfficeHint':
    'Überspringt die Statusaktualisierung, solange Ihre Teams-Abwesenheit aktiv ist. Eine Titelregel mit eigener Präsenzaktion hat Vorrang.',
  'settings.extraWordsLabel': 'Eigene Wörter für den Filter',
  'settings.extraWordsHint':
    'Ein Wort oder Ausdruck pro Zeile. Es gelten dieselben Wortgrenzen wie für die eingebaute Liste.',
  'settings.extraWordsPlaceholder': 'Wort oder Ausdruck',
  'settings.extraWordsClampHint':
    'Nur die ersten {max} Einträge mit {chars} Zeichen bleiben erhalten — {kept} werden gefiltert.',
  'settings.pauseBackoffMaxLabel': 'Obergrenze für Pausen-Backoff (Sekunden)',
  'settings.pauseBackoffClampHint':
    'Erlaubt sind {min}–{max} Sekunden; verwendet werden {effective}.',
  'dashboard.presenceGatedQuietHours': 'Status pausiert — Ruhezeiten sind aktiv',
  'dashboard.presenceGatedTrackRule': 'Status pausiert — eine Titelregel greift',
  'dashboard.presenceGatedManualStatus':
    'Status pausiert — Sie haben eine Statusmeldung selbst gesetzt',
  'dashboard.presenceGatedOutOfOffice': 'Status pausiert — Sie sind abwesend',
  // 4.7.0 — S6 (tray localization)
  'settings.languageHint':
    'Gilt auch für das Tray-Menü und das native Anwendungsmenü.',

  // 4.7.0 — S3 tray/logs hygiene
  'settings.teamsPersistWarning':
    'Angemeldet, aber dieses Gerät konnte die Sitzung nicht speichern — sie funktioniert bis zum Beenden. Teams erneut verbinden, um das Speichern zu wiederholen.',

  // 4.7.0 — S4 (rules engine)
  'rules.quietWindowHint':
    'Ruhezeiten laufen über Mitternacht — 22:00–07:00 gilt durch die Nacht. Eine Endzeit von 00:00 bedeutet Mitternacht (Tagesende), und gleicher Beginn und Schluss passen nie.',
  'rules.pausePollingLabel': 'Abfrage in diesem Zeitraum anhalten',
  'rules.pausePollingHint':
    'Während dieses Zeitraums wird Spotify gar nicht abgefragt — kein Status, kein Teams-Aufruf. Die Abfrage läuft automatisch weiter, sobald der Zeitraum endet.',
  'rules.trackRulesOrderHint':
    'Regeln werden von oben nach unten geprüft — die erste passende gewinnt. Ohne Wochentage gilt eine Regel jeden Tag; eine Endzeit von 00:00 bedeutet Tagesende, und gleicher Beginn und Schluss passen nie.',
  'rules.ruleStart': 'Beginn des Regelzeitraums',
  'rules.ruleEnd': 'Ende des Regelzeitraums',
  'rules.ruleDays': 'Aktive Tage dieser Regel (keine Auswahl = jeden Tag)',
  'rules.moveRuleUp': 'Regel {n} nach oben verschieben',
  'rules.moveRuleDown': 'Regel {n} nach unten verschieben',
  'rules.manualStatusLabel': 'Statustext für Pause und Stopp',
  'rules.manualStatusHint':
    'Der Text, der als Teams-Status gepostet wird, wenn die Wiedergabe pausiert bzw. nichts läuft. Das Musik-Emoji wird automatisch vorangestellt; ein leeres Feld stellt den Standard wieder her.',
  'rules.pausedStatusPlaceholder': 'Pausiert',
  'rules.stoppedStatusPlaceholder': 'Nichts läuft auf Spotify',

  // 4.7.0 — S5 (Protokollrotation + Einstellungen exportieren/importieren)
  'settings.sectionLogging': 'Protokollierung',
  'settings.loggingEnabledLabel': 'Protokolldatei schreiben',
  'settings.logLevelLabel': 'Protokollstufe',
  'settings.logMaxSizeLabel': 'Maximale Protokolldateigröße (MB)',
  'settings.logKeepFilesLabel': 'Aufzubewahrende archivierte Protokolldateien',
  'settings.logRotationHint':
    'Größenlimit und Anzahl der archivierten Dateien gelten ab dem nächsten Start von PresenceJam. Die gerade geschriebene Datei zählt nicht dazu: Der Protokollordner enthält höchstens eine Datei mehr als die hier eingestellte Anzahl. Das Ausschalten der Protokollierung oder eine andere Stufe wirkt sofort.',
  'settings.sectionBackup': 'Sicherung',
  'settings.backupHint':
    'Exportieren schreibt eine Kopie dieser Einstellungen, die Sie aufbewahren oder auf einen anderen Rechner übertragen können. Ihr Spotify-Client-Geheimnis bleibt im Systemschlüsselbund und ist nie enthalten — eine Datei, die eines enthält, wird beim Import abgelehnt.',
  'settings.backupExport': 'Einstellungen exportieren…',
  'settings.backupImport': 'Einstellungen importieren…',
  'settings.backupExportDialogTitle': 'PresenceJam-Einstellungen exportieren',
  'settings.backupImportDialogTitle': 'PresenceJam-Einstellungen importieren',
  'settings.backupConfirmOverwrite':
    'Beim Import werden alle aktuellen Einstellungen ersetzt. Die aktuelle Datei bleibt daneben als config.json.bak erhalten. Fortfahren?',
  'settings.backupExported': 'Einstellungen exportiert nach {path}',
  'settings.backupImported': 'Einstellungen importiert aus {path}',
  'settings.backupError': 'Die Sicherungsaktion konnte nicht abgeschlossen werden: {error}',

  // 4.7.0 — S9 (issue #677: die Tray-Pause, „Sync pausieren für …“)
  'dashboard.snoozeChip': 'Pausiert — noch {remaining} (bis {time})',
  'dashboard.snoozeResume': 'Jetzt fortsetzen',
  'dashboard.snoozeResuming': 'Wird fortgesetzt…',
  'dashboard.snoozeResumeFailed':
    'Die Synchronisierung konnte nicht fortgesetzt werden. Die Pause bleibt gespeichert — bitte erneut versuchen.',

  // 4.7.0 — S7 notifications (#675)
  'settings.notificationsTrackChange': 'Benachrichtigen, wenn der Titel wechselt',
  'settings.notificationsSyncStopped':
    'Benachrichtigen, wenn die Synchronisierung von selbst stoppt',
  'settings.notificationsAuthRequired':
    'Benachrichtigen, wenn ich mich erneut bei Teams anmelden muss',
  'settings.notificationsUpdateStaged':
    'Benachrichtigen, wenn ein Update beim Beenden installiert wird',
  'notifications.syncStoppedTitle': 'PresenceJam hat die Synchronisierung beendet',
  'notifications.syncStoppedBody':
    'Die Statussynchronisierung wurde von selbst beendet. Öffnen Sie PresenceJam, um sie neu zu starten.',
  'notifications.authRequiredTitle': 'Teams-Anmeldung erforderlich',
  'notifications.authRequiredBody':
    'Ihre Teams-Sitzung ist abgelaufen. Melden Sie sich erneut an, damit Ihr Status synchron bleibt.',
  'notifications.updateStagedTitle': 'Update bereit',
  'notifications.updateStagedBody': 'PresenceJam {version} wird beim Beenden installiert.',
  // 4.7.0 — update channel (#678)
  'settings.sectionUpdates': 'Updates',
  'settings.updateChannelLabel': 'Versionskanal',
  'settings.updateChannelStable': 'Stabil',
  'settings.updateChannelBeta': 'Beta',
  'settings.updateChannelHint':
    'Es ist noch kein Beta-Build veröffentlicht, daher greift der Beta-Kanal derzeit auf die stabile Version zurück — der Rückfall wird bei jeder Prüfung protokolliert. Beta-Builds werden nur beim Beenden installiert.',
  'update.betaOnQuitOnly':
    'Beta-Kanal: Updates werden beim Beenden installiert — auf Beta gibt es keinen Download-und-Neustart-Pfad.',
  // 4.7.0 — S8 global hotkeys
  'settings.sectionShortcuts': 'Globale Tastenkürzel',
  'settings.shortcutsHint':
    'Diese funktionieren auch, wenn das Fenster ausgeblendet ist. Klicken Sie in ein Feld und drücken Sie die gewünschte Kombination — das Feld zeichnet auf, was Sie drücken, nicht was Sie tippen.',
  'settings.shortcutTogglePlayback': 'Wiedergabe umschalten',
  'settings.shortcutToggleSync': 'Synchronisierung pausieren oder fortsetzen',
  'settings.shortcutUnbound': 'Nicht belegt — klicken und Kombination drücken',
  'settings.shortcutClear': 'Löschen',
  'settings.shortcutRegistered': 'Aktiv',
  'settings.shortcutNotRegistered': 'Auf diesem Desktop nicht registriert',
  'settings.shortcutCaptureReleased':
    'Während der Aufzeichnung freigegeben — die aktuelle Belegung würde sonst ausgelöst',
  'settings.shortcutRejected': 'Nicht verwendbar: {reason}',
  'settings.shortcutRegistrationFailed':
    'Registrierung auf diesem Desktop fehlgeschlagen: {reason}',
  // 4.7.0 — S12 Hygiene (Design/Dichte)
  'settings.themeSystem': 'System',
  'settings.themeHint':
    '„System“ folgt der Darstellung Ihres Betriebssystems; Dunkel und Hell bleiben fest gewählt.',
  'settings.densityCompactLabel': 'Kompakte Abstände',
  'settings.densityHint': 'Verkleinert Abstände und Schriftgrößen. Unabhängig vom Design.',
  // --- 5.0 wave1 settings ---
  'rules.presenceAvailable': 'Verfügbar',
  'rules.presenceBusyCall': 'Beschäftigt — In einem Anruf',
  'rules.presenceBusyConference': 'Beschäftigt — In einer Telefonkonferenz',
  'rules.presenceAway': 'Abwesend',
  'rules.presenceDndPresenting': 'Nicht stören — Präsentiert',
};
