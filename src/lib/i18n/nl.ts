/**
 * C6 i18n foundation (docs/scope-3.3.md §C6) — Dutch dictionary.
 * Typed against `Dict`, so every English key must be present.
 *
 * Model-written Dutch translation (issue #984) — human review pending before this copy is considered final.
 */

import type { Dict } from './en';

export const nl: Dict = {
  // ── common ────────────────────────────────────────────────────────
  'common.back': 'Terug',
  'common.backToDashboard': 'Terug naar dashboard',
  'common.checkNow': 'Aanmeldstatus controleren',
  'common.connected': 'Verbonden',
  'common.dismiss': 'Sluiten',
  'common.openSignInPage': 'Open de Microsoft-aanmeldpagina:',
  'common.enterCodeWhenAsked': 'Voer deze code in wanneer erom wordt gevraagd:',
  'common.moreActions': 'Meer acties',
  'common.launchAtLogin': 'Starten bij aanmelden',
  'common.loading': 'Laden…',
  'common.notConnected': 'Niet verbonden',
  'common.reconnecting': 'Opnieuw verbinden…',
  'common.reconnect': 'Opnieuw verbinden',
  'common.retry': 'Opnieuw proberen',
  'common.bootFailed': 'App-status kon niet worden geladen',
  'common.resetToDefault': 'Herstellen naar standaard',
  'common.themeToggle': 'Thema wisselen',
  'common.waiting': 'Wachten…',
  'common.waitingForSignIn': 'Wachten op aanmelden…',
  'common.codeExpiresIn': 'Code verloopt over {time}',
  'common.codeExpired': 'Deze code is verlopen — hij kan niet meer worden gebruikt.',
  'common.getNewCode': 'Nieuwe code ophalen',
  'common.yes': 'Ja',
  'common.no': 'Nee',
  'common.tagline': 'Spotify → Teams-status',

  // ── dashboard ─────────────────────────────────────────────────────
  'dashboard.spotifyOff': 'Spotify uit',
  'dashboard.teamsOff': 'Teams uit',
  'dashboard.syncing': 'Synchroniseren',
  'dashboard.logsDetachedTitle': 'Logboeken (losgekoppeld — klik om te focussen)',
  'dashboard.logsTitle': 'Logboeken',
  'dashboard.logsDetachedAria': 'Logboeken (losgekoppeld in apart venster)',
  'dashboard.openLogsAria': 'Logboeken openen',
  'dashboard.diagnostics': 'Diagnose',
  'dashboard.openDiagnosticsAria': 'Diagnose openen',
  'dashboard.settingsDetachedTitle': 'Instellingen (losgekoppeld — klik om te focussen)',
  'dashboard.settings': 'Instellingen',
  'dashboard.settingsDetachedAria': 'Instellingen (losgekoppeld in apart venster)',
  'dashboard.openSettingsAria': 'Instellingen openen',
  'dashboard.about': 'Over',
  'dashboard.aboutAria': 'Over PresenceJam',
  'dashboard.pauseSync': 'Synchronisatie pauzeren',
  'dashboard.resumeSync': 'Synchronisatie hervatten',
  'dashboard.presenceGated': 'Status gepauzeerd — u bent bezig, in gesprek of presenteert',
  'dashboard.setupRequired': 'Installatie vereist',
  'dashboard.setupHint':
    'Verbind Spotify en Microsoft Teams zodat uw afgespeelde nummers uw Teams-status aansturen.',
  'dashboard.continueSetup': 'Doorgaan met installatie',
  'dashboard.playing': 'Speelt af',
  'dashboard.paused': 'Gepauzeerd',
  'dashboard.liveStreamAria': 'Livestream — positie onbekend',
  'dashboard.yourTeamsStatus': 'Uw Teams-status',
  'dashboard.nothingPlaying': 'Niets speelt af',
  'dashboard.nothingPlayingHint':
    'Speel iets af op Spotify en het verschijnt in uw Teams-status.',
  'dashboard.syncCrashed': 'Synchronisatie onverwacht gestopt. Druk op hervatten (▶) om opnieuw te starten.',
  'dashboard.credentialCheckFailed': 'Uw aanmeldgegevens konden niet worden gecontroleerd. Controleer uw verbinding en probeer het opnieuw.',
  'dashboard.syncToggleFailed': 'Synchronisatie kon niet worden gestart/gestopt. Probeer het opnieuw — als het aanhoudt, open Diagnose via de dashboardkop.',
  'dashboard.statusNotConfigured': 'Niet geconfigureerd',
  'dashboard.statusNoTrack': 'Geen nummer speelt af',
  'dashboard.live': 'Live',
  'dashboard.refreshStatus': 'Status vernieuwen',
  'dashboard.refreshing': 'Vernieuwen…',
  'dashboard.refreshFailed': 'Status kon niet worden vernieuwd. Probeer het opnieuw.',
  'dashboard.refreshAria': 'Teams-status nu vernieuwen',

  // ── logs ──────────────────────────────────────────────────────────
  'logs.title': 'Logboeken',
  'logs.filterAria': 'Filter voor logniveau',
  'logs.level.all': 'Alle',
  'logs.level.trace': 'Trace',
  'logs.level.debug': 'Debug',
  'logs.level.info': 'Info',
  'logs.level.warning': 'Waarschuwing',
  'logs.level.error': 'Fout',
  'logs.count_one': '{count} regel',
  'logs.count_other': '{count} regels',
  'logs.showingOf': '{shown} van {total} weergegeven',
  'logs.jumpToLatest': 'Naar nieuwste gaan',
  'logs.popOut': 'Loskoppelen',
  'logs.clear': 'Wissen',
  'logs.openFolder': 'Map openen',
  'logs.empty': 'Nog geen logregels',
  'logs.emptyHint': 'Liveregels verschijnen hier zodra de synchronisatie start en Spotify afspeelt.',

  // ── settings ──────────────────────────────────────────────────────
  'settings.title': 'Instellingen',
  'settings.popBackIn': 'Terugplaatsen',
  'settings.popOutActionTitle': 'Loskoppelen in eigen venster',
  'settings.unsavedChanges': 'Niet-opgeslagen wijzigingen',
  'settings.sectionSpotify': 'Spotify',
  'settings.sectionTeams': 'Microsoft Teams',
  'settings.sectionPresence': 'Aanwezigheid',
  'settings.sectionStatusFormat': 'Statusopmaak',
  'settings.sectionPolling': 'Synchronisatiefrequentie',
  'settings.sectionNotifications': 'Meldingen',
  'settings.sectionAppearance': 'Weergave',
  'settings.clientId': 'Client-ID',
  'settings.clientIdPlaceholder': 'Spotify Client-ID invoeren',
  'settings.clientSecret': 'Clientgeheim',
  'settings.secretStoredHint':
    'Veilig opgeslagen in de sleutelhanger van uw besturingssysteem. Om het te vervangen, gaat u terug naar het dashboard en kiest u Doorgaan met installatie.',
  'settings.secretNotConfigured': 'Niet geconfigureerd.',
  'settings.runOnboarding': 'Installatie-assistent uitvoeren',
  'settings.toSetUpSpotify': 'om Spotify in te stellen.',
  'settings.reconnectSpotify': 'Spotify opnieuw verbinden',
  'settings.completeAuthInBrowser': 'Voltooi de verificatie in de browser.',
  'settings.playbackScopeBanner': 'Spotify heeft afspeelbediening toegevoegd. Klik op Opnieuw verbinden naast dit bericht om deze in te schakelen.',
  'settings.spotifySecretConflict':
    'Het clientgeheim in uw configuratiebestand wijkt af van dat in de sleutelhanger. Verbind Spotify opnieuw om dit op te lossen.',
  'settings.teamsAuthHint':
    'Teams-verificatie gebruikt uw Microsoft 365-account. Geen extra configuratie vereist.',
  'settings.presenceScopeBanner': 'Teams heeft vergader-/gespreksdetectie toegevoegd. Klik op Opnieuw verbinden naast dit bericht om deze in te schakelen.',
  'settings.availabilitySyncLabel': 'Beschikbaar tonen tijdens luisteren',
  'settings.availabilitySyncHint':
    'Standaard uit. Wanneer aan, toont Teams u als Beschikbaar (in plaats van Bezig) terwijl muziek speelt. Let op: Teams toont nog steeds Bezig tijdens gesprekken en vergaderingen.',
  'settings.presenceGateLabel': 'Status pauzeren tijdens vergaderingen/gesprekken/Niet storen',
  'settings.presenceGateHint':
    'Standaard aan. Slaat het schrijven van uw Spotify-status over terwijl Teams zegt dat u bezig bent, in een vergadering, in gesprek of presenteert.',
  'settings.formatTemplate': 'Opmaaksjabloon',
  'settings.formatTemplatePlaceholder': '🎵 {artist} - {track} 🎧',
  'settings.livePreview': 'Livevoorbeeld',
  'settings.placeholdersHint':
    'Beschikbare tijdelijke aanduidingen: {artist}, {track}, {album}, {emoji}, {device}, {playlist} (of {context}), {progress}, {shuffle}, {repeat}. Shuffle en Repeat tonen 🔀/🔁 alleen wanneer ze aan staan.',
  'settings.profanityFilterLabel': 'Scheldwoorden filteren in status',
  'settings.placeholderTextLabel': 'Tijdelijke tekst',
  'settings.placeholderTextHint':
    'Gebruik {emoji} voor afspeelstatus (🎵 speelt af / ⏸ gepauzeerd). Getoond wanneer scheldwoorden in nummerinfo worden gedetecteerd.',
  'settings.placeholderTextPlaceholder': 'Luistert momenteel naar Spotify',
  'settings.profaneSampleToggle': 'Voorbeeld met grof voorbeeldnummer',
  'settings.defaultIntervalLabel': 'Standaardinterval: {seconds}s',
  'settings.minIntervalLabel': 'Min. interval (s)',
  'settings.maxIntervalLabel': 'Max. interval (s)',
  'settings.clampHint':
    'Min. interval overschrijdt max. interval — max wordt opgeslagen als {max}s.',
  'settings.notificationsHint':
    'Elke klasse die u inschakelt toont een systeemmelding — de eerste kan uw besturingssysteem om toestemming vragen.',
  'settings.themeLabel': 'Thema',
  'settings.themeDark': 'Donker',
  'settings.themeLight': 'Licht',
  'settings.languageLabel': 'Taal',
  'settings.saveChanges': 'Wijzigingen opslaan',
  'settings.saving': 'Opslaan…',
  'settings.saved': 'Instellingen opgeslagen.',
  'settings.failedToSave': 'Instellingen konden niet worden opgeslagen — uw wijzigingen zijn bewaard. Probeer het opnieuw.',
  'settings.openLogsFolder': 'Logboekmap openen',
  'settings.previewUnavailable': '(voorbeeld niet beschikbaar)',

  // ── diagnostics ───────────────────────────────────────────────────
  'diagnostics.title': 'Diagnose',
  'diagnostics.localOnlyHint': 'Alleen-lokale momentopname — veilig om bij een bugrapport te voegen.',
  'diagnostics.copy': 'Diagnose kopiëren',
  'diagnostics.saveToFile': 'Opslaan in bestand',
  'diagnostics.copied': 'Diagnose naar klembord gekopieerd.',
  'diagnostics.copyFailed': 'Kopiëren mislukt — gebruik "Opslaan in bestand".',
  'diagnostics.savedToDownloads': 'Diagnose opgeslagen in uw downloadmap.',
  'diagnostics.saveFailed': 'Opslaan mislukt — gebruik "Diagnose kopiëren".',
  'diagnostics.collecting': 'Diagnose verzamelen…',
  'diagnostics.collectFailed': 'Diagnose verzamelen mislukt',
  'diagnostics.versions': 'Versies',
  'diagnostics.configuration': 'Configuratie',
  'diagnostics.connections': 'Verbindingen',
  'diagnostics.recentLogLines': 'Recente logregels',
  'diagnostics.app': 'PresenceJam',
  'diagnostics.tauri': 'Tauri',
  'diagnostics.os': 'Besturingssysteem',
  'diagnostics.osRelease': 'Release',
  'diagnostics.installFlavor': 'Installatievariant',
  'diagnostics.unknown': 'onbekend',
  'diagnostics.spotifyClientId': 'Spotify-client-ID',
  'diagnostics.redirectUri': 'Redirect-URI',
  'diagnostics.notSet': '(niet ingesteld)',
  'diagnostics.clientSecretKeychain': 'Clientgeheim in sleutelhanger',
  'diagnostics.clearOnPause': 'Status wissen bij pauze',
  'diagnostics.profanityFilter': 'Scheldwoordenfilter',
  'diagnostics.extraProfanityWords': 'Extra scheldwoorden',
  'diagnostics.startMinimized': 'Geminimaliseerd starten',
  'diagnostics.availabilitySync': 'Teams-beschikbaarheidssynchronisatie',
  'diagnostics.presenceGate': 'Aanwezigheidspoort',
  'diagnostics.pollInterval': 'Pollinterval (standaard/min/max)',
  'diagnostics.logging': 'Logboekregistratie',
  'diagnostics.loggingEnabled': 'ingeschakeld ({level})',
  'diagnostics.loggingDisabled': 'uitgeschakeld',
  'diagnostics.respectManualStatus': 'Handmatige status respecteren',
  'diagnostics.gateOutOfOffice': 'Poort bij afwezigheid',
  'diagnostics.locale': 'Landinstelling',
  'diagnostics.updateChannel': 'Updatekanaal',
  'diagnostics.configSnoozed': 'Synchronisatie gesnoozed',
  'diagnostics.launchAtLogin': 'Starten bij aanmelden',
  'diagnostics.statusRules': 'Statusregels',
  'diagnostics.statusRulesValue': '{quiet}/{quietTotal} stille uren, {rules}/{rulesTotal} nummerregels aan',
  'diagnostics.spotifyConnected': 'Spotify verbonden',
  'diagnostics.spotifyTokenExpires': 'Spotify-token verloopt',
  'diagnostics.teamsConnected': 'Teams verbonden',
  'diagnostics.teamsTokenExpires': 'Teams-token verloopt',
  'diagnostics.expired': '(verlopen)',
  'diagnostics.keychainSpotifySecret': 'Sleutelhanger: Spotify-geheim aanwezig',
  'diagnostics.keychainEncryptionKey': 'Sleutelhanger: tokenversleutelingssleutel aanwezig',
  'diagnostics.tokensNeverIncluded':
    'Tokenwaarden worden nooit opgenomen — alleen verlooptijdstippen en aanwezigheidsvlaggen.',
  'diagnostics.noLogLinesYet': 'Nog geen logregels beschikbaar.',
  'diagnostics.failedInstallTitle': 'Mislukte update-installatie',
  'diagnostics.failedInstallVersion': 'Versie',
  'diagnostics.failedInstallError': 'Fout',
  'diagnostics.failedInstallTimestamp': 'Geprobeerd op',
  'diagnostics.failedInstallDismissFailed': 'De mislukte-installatiemelding kon niet worden verwijderd.',
  'diagnostics.resetTokenStorage': 'Lokale tokenopslag resetten',
  'diagnostics.resetTokenConfirm':
    'Hiermee worden tokens.json en bijbehorende bestanden plus de opgeslagen tokenversleutelingssleutel verwijderd. U moet zich opnieuw aanmelden. Doorgaan?',
  'diagnostics.resetTokenDone': 'Tokenopslag gereset. Meld u opnieuw aan om de synchronisatie te hervatten.',
  'diagnostics.resetTokenFailed': 'Resetten mislukt — {error}',
  'diagnostics.syncRunning': 'Synchronisatie actief',
  'diagnostics.syncSnoozed': 'Gesnoozed',
  'diagnostics.syncSnoozeMinutes': 'nog {minutes} min',
  'diagnostics.syncManualStatusBlocks': 'Handmatige status houdt schrijfbewerkingen tegen',
  'diagnostics.syncPresenceGateReason': 'Reden aanwezigheidspoort',
  'diagnostics.syncTransientFailures': 'Opeenvolgende verificatiefouten',
  'diagnostics.syncNetworkFailures': 'Opeenvolgende netwerkfouten',

  // ── reconnect ─────────────────────────────────────────────────────
  'reconnect.title': 'Opnieuw verbinden',
  'reconnect.description': 'Synchronisatie heeft uw aandacht nodig. Verbind hieronder opnieuw om te hervatten.',
  'reconnect.missingCredentials': 'Ontbrekende aanmeldgegevens',
  'reconnect.failed': 'Mislukt',
  'reconnect.readyToReconnect': 'Klaar om opnieuw te verbinden',
  'reconnect.spotifyOk': 'Spotify succesvol opnieuw verbonden.',
  'reconnect.spotifyNotConfigured':
    'Spotify-aanmeldgegevens zijn niet geconfigureerd op deze machine.',
  'reconnect.completeAuthInOpenedBrowser':
    'Voltooi de verificatie in het geopende browservenster.',
  'reconnect.tryAgain': 'Opnieuw proberen',
  'reconnect.clickBelowSpotify': 'Klik hieronder om uw Spotify-account opnieuw te verbinden.',
  'reconnect.teamsOk': 'Teams succesvol opnieuw verbonden.',
  'reconnect.clickBelowTeams':
    'Klik hieronder om uw Microsoft Teams-account opnieuw te verbinden.',
  'reconnect.tokenStorageUnusable': 'Uw opgeslagen aanmeldgegevens kunnen niet worden gelezen — de opgeslagen tokenversleutelingssleutel is onbruikbaar.',
  'reconnect.resetTokenStorage': 'Lokale tokenopslag resetten',
  'reconnect.resetTokenConfirm':
    'Hiermee worden tokens.json en bijbehorende bestanden plus de opgeslagen tokenversleutelingssleutel verwijderd. U moet zich opnieuw aanmelden. Doorgaan?',
  'reconnect.resetTokenDone': 'Tokenopslag gereset. Meld u hieronder opnieuw aan om te hervatten.',
  'reconnect.resetTokenFailed': 'Resetten mislukt — {error}',
  'reconnect.missingCredsTitle': 'Spotify-aanmeldgegevens kwijt?',
  'reconnect.reenterCredsHint':
    'U moet uw Client-ID en Clientgeheim opnieuw invoeren.',
  'reconnect.goToFullSetup': 'Naar volledige installatie',
  'reconnect.reconnectTeams': 'Teams opnieuw verbinden',

  // ── about ─────────────────────────────────────────────────────────
  'about.version': 'Versie {version}',
  'about.description':
    'Toont wat u op Spotify afspeelt in uw Microsoft Teams-status — automatisch.',
  'about.statusSync': 'Statussynchronisatie',
  'about.live': 'Live',
  'about.auth': 'Aanmelden',
  'about.authMethod': 'Spotify + Microsoft',
  'about.storage': 'Opslag',
  'about.osKeychain': 'Sleutelhanger van besturingssysteem',
  'about.githubRepo': 'GitHub-repository',
  'about.releases': 'Releases',
  'about.reportIssue': 'Probleem melden',

  // ── update banner ─────────────────────────────────────────────────
  'update.available': 'Update v{version} beschikbaar',
  'update.stagedQuit': 'v{version} wordt geïnstalleerd wanneer u PresenceJam afsluit',
  'update.downloadFailed': 'Downloaden mislukt — {error}',
  'update.downloadAndInstall': 'Downloaden en installeren',
  'update.downloading': 'Downloaden…',
  'update.installOnQuit': 'Installeren bij afsluiten',
  'update.preparing': 'Voorbereiden…',
  'update.dismissAria': 'Updatebanner sluiten',
  'update.confirmQuitInstall':
    'v{staged} installeren wanneer u afsluit? Huidige versie: v{current}.',
  'update.confirmQuitInstallUnknown': 'v{staged} installeren wanneer u afsluit?',
  'update.stagedVsCurrent':
    'v{staged} wordt geïnstalleerd wanneer u afsluit (huidig v{current})',
  'update.staleSkipped':
    'v{staged} is overgeslagen — uw huidige v{current} is nieuwer.',
  'update.staleSkippedUnknown':
    'v{staged} is overgeslagen — deze is niet nieuwer dan uw huidige versie.',
  'update.installAnyway': 'Toch installeren',
  // #894: a `.deb` / `.rpm` install is updated by its package manager, not by
  // the in-app updater — the release manifest only offers the AppImage payload,
  // which neither installer can apply. The banner shows the command instead of
  // an install button.
  'update.packageManagedDeb':
    'v{version} is beschikbaar. Dit exemplaar is geïnstalleerd als .deb-pakket, dus updates komen via apt.',
  'update.packageManagedRpm':
    'v{version} is beschikbaar. Dit exemplaar is geïnstalleerd als .rpm-pakket, dus updates komen via dnf.',
  'update.packageManagerInstallDeb': 'sudo apt install ./{file}',
  'update.packageManagerInstallRpm': 'sudo dnf install ./{file}',

  // ── onboarding ────────────────────────────────────────────────────
  'onboarding.stepOf': 'Stap {step} van 3',
  'onboarding.step1Title': 'Spotify verbinden',
  'onboarding.step1Intro':
    'Plak hieronder uw Spotify Client-ID en Clientgeheim en kies Spotify verbinden — wij openen de Spotify-aanmeldpagina.',
  'onboarding.getCredentials': 'Haal uw Spotify-aanmeldgegevens op',
  'onboarding.instruction1':
    'Open het Spotify-developerdashboard en maak een app.',
  'onboarding.instruction2': "Voeg onder Redirect-URI's presencejam://callback toe (dit vertelt Spotify waar u naartoe moet worden teruggestuurd).",
  'onboarding.instruction3':
    'Kopieer de Client-ID en het Clientgeheim uit de instellingen van de app.',
  'onboarding.clientIdPlaceholder': '32-teken Spotify Client-ID',
  'onboarding.clientSecretPlaceholder': 'Spotify Clientgeheim',
  'onboarding.connectSpotify': 'Spotify verbinden',
  'onboarding.signInWaiting': 'Spotify-aanmelding wacht…',
  'onboarding.manualUrlHint':
    'Rond het aanmelden met Spotify af in uw browser en plak hieronder het volledige adres uit de adresbalk.',
  'onboarding.manualUrlLabel': 'Spotify-redirect-URL',
  'onboarding.manualUrlPlaceholder': 'presencejam://callback?code=…',
  'onboarding.submitCode': 'Code verzenden',
  'onboarding.connectedToSpotify': 'Verbonden met Spotify',
  'onboarding.continue': 'Doorgaan →',
  'onboarding.step2Title': 'Microsoft Teams verbinden',
  'onboarding.step2Intro':
    'Wij gebruiken de apparaatcodestroom van Microsoft — een eenmalige code die u op een Microsoft-pagina invoert. Geen extra installatie vereist.',
  'onboarding.startMicrosoftSignIn': 'Microsoft Teams verbinden',
  'onboarding.connectedToTeams': 'Verbonden met Microsoft Teams',
  'onboarding.step3Title': 'Laatste details',
  'onboarding.step3Intro':
    'Kies hoe uw statusbericht eruit moet zien en of PresenceJam moet starten wanneer u zich aanmeldt.',
  'onboarding.statusTemplate': 'Statussjabloon',
  'onboarding.placeholdersHint':
    'Tijdelijke aanduidingen: {artist}, {track}, {album}, {emoji}, {device}, {playlist} (of {context}), {progress}, {shuffle}, {repeat}',
  'onboarding.pollInterval': 'Hoe vaak Spotify controleren: {seconds}s',
  'onboarding.settingUp': 'Instellen…',
  'onboarding.finishSetup': 'Installatie voltooien',

  // ── validation / errors ───────────────────────────────────────────
  'validation.clientIdRequired': 'Spotify Client-ID is vereist.',
  'validation.clientIdFormat':
    'Spotify Client-ID moet precies 32 hexadecimale tekens zijn.',
  'validation.clientSecretRequired': 'Spotify Clientgeheim is vereist.',
  'validation.clientSecretTooShort':
    'Dat Clientgeheim lijkt te kort — het moet minimaal 32 tekens zijn. Controleer op een kopieerfout.',
  'validation.noCodeInUrl':
    'Die URL bevat geen aanmeldcode — plak het volledige adres uit de adresbalk van uw browser nadat Spotify u heeft doorgestuurd.',
  'validation.connectBothFirst':
    'Verbind eerst zowel Spotify als Teams voordat u de installatie voltooit.',
  'validation.setupFailed': 'Installatie mislukt: {error}',

  // ── routes / chrome ───────────────────────────────────────────────
  'routes.skipToMainContent': 'Naar hoofdinhoud gaan',
  'routes.unknownPane': 'Onbekend paneel: {pane}',

  // ── feat/45-features: status rules (#432) + support snapshot (#434) ──
  'rules.sectionTitle': 'Statusregels',
  'rules.sectionHint':
    'Stille uren en nummerregels onderdrukken het schrijven van de Teams-status, via hetzelfde aanwezigheidspoortpad — een gewiste regel plaatst automatisch halverwege een nummer.',
  'rules.quietHoursLabel': 'Stille uren',
  'rules.noQuietHours': 'Geen stille uren gedefinieerd — status synchroniseert op alle uren.',
  'rules.quietStart': 'Stille uren beginnen',
  'rules.quietEnd': 'Stille uren eindigen',
  'rules.quietDays': 'Actieve dagen (niets geselecteerd = elke dag)',
  'rules.dayEveryDay': 'Elke dag',
  'rules.day1': 'ma',
  'rules.day2': 'di',
  'rules.day3': 'wo',
  'rules.day4': 'do',
  'rules.day5': 'vr',
  'rules.day6': 'za',
  'rules.day7': 'zo',
  'rules.addQuietHours': 'Stille uren toevoegen',
  'rules.trackRulesLabel': 'Nummerregels',
  'rules.noTrackRules': 'Geen nummerregels gedefinieerd — alle nummers synchroniseren normaal.',
  'rules.artistPlaceholder': 'Artiest bevat…',
  'rules.trackPlaceholder': 'Nummer bevat…',
  'rules.replacementPlaceholder': 'Plaats dit in plaats daarvan (leeg = onderdrukken)',
  'rules.addTrackRule': 'Nummerregel toevoegen',
  'rules.removeRule': 'Verwijderen',
  'rules.ruleEnabled': 'Ingeschakeld',
  'logs.copySnapshot': 'Momentopname kopiëren',
  'logs.snapshotCopied': 'Geradeerde momentopname naar klembord gekopieerd.',
  'logs.snapshotCopyFailed': 'Momentopname kon niet worden gekopieerd.',
  'logs.openFolderError':
    'Logboekmap kon niet worden geopend. Mogelijk bestaat deze nog niet — probeer de app opnieuw te starten om deze aan te maken.',
  // 4.6 additions
  'dashboard.availabilityListening': 'Luistert (Beschikbaar)',
  'dashboard.availabilityCleared': 'Beschikbaarheid gewist',
  'settings.saveAndLeave': 'Opslaan en verlaten',
  'settings.discardChanges': 'Wijzigingen negeren',
  'settings.stayHere': 'Hier blijven',
  'settings.notificationsDenied':
    'Meldingen worden geblokkeerd door het systeem. Sta ze toe in uw systeeminstellingen en zet dit daarna weer aan.',
  'diagnostics.expiryBuffer': 'Tokenvernieuwingsbuffer',
  'diagnostics.teamsRefreshTokenPresent': 'Teams-vernieuwingstoken opgeslagen',
  'reconnect.restartSignIn': 'Aanmelden opnieuw starten',
  'update.stagingProgress': 'Update voorbereiden — {percent}%',
  'update.cancelStage': 'Annuleren',
  'onboarding.submitting': 'Verzenden…',
  'settings.episodeFormatHint':
    'Podcasts en audioboeken gebruiken hun eigen sjabloon — 🎙️ {show} - {episode} — dus uw muzieksjabloon hierboven wordt niet op hen toegepast.',
  'reconnect.keychainUnavailableBadge':
    'Sleutelhanger niet beschikbaar',
  'reconnect.keychainUnavailableHint':
    'PresenceJam kon uw opgeslagen Spotify Clientgeheim niet lezen: de systeemsleutelhanger is vergrendeld of ontbreekt. Ontgrendel deze (of installeer een Secret Service-provider zoals gnome-keyring) en probeer het opnieuw — uw geheim is nog opgeslagen, dus u hoeft Spotify niet opnieuw in te stellen.',
  'settings.secretKeychainUnavailable':
    'Systeemsleutelhanger niet beschikbaar — mogelijk vergrendeld of ontbrekend. Ontgrendel deze (of installeer een Secret Service-provider) om uw opgeslagen geheim te gebruiken; het is nog opgeslagen.',
  'diagnostics.quarantineTitle': 'Instellingen zijn gereset',
  'diagnostics.quarantineBodyNow':
    'PresenceJam kon uw instellingenbestand niet lezen, dus elke instelling is teruggezet naar de standaardwaarde.',
  'diagnostics.quarantineBodyEarlier':
    'Een eerdere start kon uw instellingenbestand niet lezen en heeft het teruggezet naar de standaardwaarden.',
  'diagnostics.quarantineBackupPresent':
    'Het onleesbare origineel is bewaard naast uw instellingenbestand als {name}, zodat de waarden die het bevatte nog kunnen worden hersteld.',
  'diagnostics.quarantineBackupMissing':
    'Het onleesbare origineel staat nog naast uw instellingenbestand als config.json.',
  'diagnostics.quarantineWhere':
    'Beide bestanden staan in de PresenceJam-map in uw gebruikersconfiguratiemap — de back-up staat naast config.json.',
  // 4.6 additions (presence rules #634/#635/#636/#637 + #538 consumption sites)
  'rules.replacementClampHint':
    'Vervangende status is beperkt tot {max} tekens; de rest wordt niet geplaatst.',
  'rules.presenceLabel': 'Aanwezigheid terwijl deze regel geldt',
  'rules.presenceNone': 'Mijn aanwezigheid niet wijzigen',
  'rules.presenceHint':
    'Een regel kan Teams-beschikbaarheid/activiteit instellen, maar alleen terwijl “Beschikbaarheidssynchronisatie” aan staat; deze overschrijft nooit een gesprek, een vergadering of een handmatig ingestelde status.',
  'settings.respectManualStatusLabel': 'Een handmatig ingestelde status nooit overschrijven',
  'settings.respectManualStatusHint':
    'Hergebruikt de aanwezigheidslezing die de poort al uitvoert, dus kost geen extra verzoek — de tekst wordt gerespecteerd totdat u deze wijzigt of deze verloopt.',
  'settings.gateOutOfOfficeLabel': 'Pauzeren terwijl ik afwezig ben',
  'settings.gateOutOfOfficeHint':
    'Slaat de statusupdate over terwijl uw Teams-afwezigheidsinstelling aan staat. Een nummerregel met eigen aanwezigheidsactie heeft voorrang.',
  // Issue #872: the OS-level presentation gate (full-screen app, slide
  // deck, Windows Focus Assist Quiet Time). OFF by default, matching how
  // `availabilitySync` and `gateWhenOutOfOffice` shipped — 4.7 behaviour
  // is unchanged until the user opts in.
  'settings.gateWhenPresentingLabel': 'Pauzeren terwijl ik presenteer',
  'settings.gateWhenPresentingHint':
    'Slaat het schrijven van uw Spotify-status over terwijl het besturingssysteem een volledig-scherm-app, presentatie of Stille tijd meldt. Linux/macOS melden dit signaal nooit, dus de schakelaar doet daar niets.',
  // Issue #873: the desktop-idle gate. `0` (the default) keeps 4.7
  // behaviour — the app keeps advertising listening until the user
  // explicitly opts in.
  'settings.idleAwayLabel': 'Pauzeren wanneer mijn bureaublad inactief is',
  'settings.idleAwayHint':
    'Stopt met het adverteren van uw Spotify-status zodra het bureaublad zo veel seconden geen toetsenbord-/muisinvoer heeft gehad. 60–3600; 0 schakelt uit. Linux/macOS melden dit signaal nooit.',
  'settings.extraWordsLabel': 'Eigen woorden om te filteren',
  'settings.extraWordsHint':
    'Eén woord of zin per regel. Toegepast met dezelfde grenzen als de ingebouwde lijst.',
  'settings.extraWordsPlaceholder': 'woord of zin',
  'settings.extraWordsClampHint':
    'Alleen de eerste {max} items van {chars} tekens worden bewaard — {kept} worden gefilterd.',
  'settings.pauseBackoffMaxLabel': 'Gepauzeerd backoff-plafond (seconden)',
  'settings.pauseBackoffClampHint':
    'Toegestaan bereik is {min}–{max} seconden; {effective} wordt gebruikt.',
  'dashboard.presenceGatedQuietHours': 'Status gepauzeerd — stille uren zijn actief',
  'dashboard.presenceGatedTrackRule': 'Status gepauzeerd — een nummerregel kwam overeen',
  'dashboard.presenceGatedManualStatus':
    'Status gepauzeerd — u heeft handmatig een statusbericht ingesteld',
  'dashboard.presenceGatedOutOfOffice': 'Status gepauzeerd — u bent afwezig',
  'dashboard.presenceGatedPresenting':
    'Status gepauzeerd — u presenteert of gebruikt een volledig-scherm-app',
  'dashboard.presenceGatedQuietTime': 'Status gepauzeerd — Focus Assist staat aan',
  'dashboard.presenceGatedIdle': 'Status gepauzeerd — bureaublad is inactief',
  // 4.7.0 — S6 (tray localization): `config.locale` is the single source of
  // truth, so the picker also drives the tray and the native app menu.
  'settings.languageHint':
    'Geldt ook voor het vakmenu en het systeemeigen applicatiemenu.',
  // Best-effort translation (no native review yet) — see #984.
  'settings.languageFollowSystemLabel': 'De systeemtaal volgen',
  'settings.languageFollowSystemHint':
    'Wanneer ingeschakeld, wordt de taalkiezer hierboven genegeerd: de app neemt bij elke start de taal van uw besturingssysteem over. Niet-ondersteunde systeemtalen vallen nog steeds terug op het Engels.',

  // Issue #932: the banner used to be Teams-only
  // (`settings.teamsPersistWarning`, added with #562). The new
  // Spotify mirror ships the same copy with a `{provider}` placeholder
  // — the Settings card passes the display name (`Microsoft Teams` or
  // `Spotify`) so one banner covers both providers.
  'settings.authPersistWarning':
    'Aangemeld, maar dit apparaat kon de {provider}-sessie niet opslaan — deze werkt tot u afsluit. Verbind {provider} opnieuw om het opslaan opnieuw te proberen.',

  // 4.7.0 — S4 (rules engine)
  'rules.quietWindowHint':
    'Stille uren lopen door over middernacht — 22:00–07:00 loopt door de nacht. Elke helft hoort bij de nacht waarin deze begint: met alleen maandag geselecteerd geldt het venster van maandagnacht tot dinsdagochtend. Een eindtijd van 00:00 betekent middernacht (het einde van de dag), en een start gelijk aan het einde komt nooit overeen.',
  'rules.pausePollingLabel': 'Polling stoppen tijdens dit venster',
  'rules.pausePollingHint':
    'Terwijl dit venster actief is wordt Spotify helemaal niet bevraagd — geen statusupdate en geen Teams-aanroep. Polling wordt vanzelf hervat wanneer het venster eindigt.',
  'rules.trackRulesOrderHint':
    'Regels worden van boven naar beneden geëvalueerd — de eerste overeenkomende wint. Een regel zonder weekdagen geldt elke dag, een eindtijd van 00:00 betekent het einde van de dag, en een start gelijk aan het einde komt nooit overeen. Een venster over middernacht hoort bij de nacht waarin het begint: met alleen maandag geselecteerd geldt 22:00–07:00 van maandagnacht tot dinsdagochtend.',
  'rules.ruleStart': 'Regelvenster start',
  'rules.ruleEnd': 'Regelvenster einde',
  'rules.ruleDays': 'Actieve dagen voor deze regel (niets geselecteerd = elke dag)',
  'rules.moveRuleUp': 'Regel {n} omhoog verplaatsen',
  'rules.moveRuleDown': 'Regel {n} omlaag verplaatsen',
  'rules.manualStatusLabel': 'Pauzeren en statustekst stoppen',
  'rules.manualStatusHint':
    'De tekst die als uw Teams-status wordt geplaatst terwijl het afspelen is gepauzeerd en wanneer niets afspeelt. De muziekemoji wordt voor u toegevoegd; een veld leegmaken herstelt de standaard.',
  'rules.pausedStatusPlaceholder': 'Gepauzeerd',
  'rules.stoppedStatusPlaceholder': 'Niets speelt af op Spotify',

  // 4.7.0 — S5 (log rotation + settings export/import)
  'settings.sectionLogging': 'Logboekregistratie',
  'settings.loggingEnabledLabel': 'Logbestand schrijven',
  'settings.logLevelLabel': 'Logniveau',
  'settings.logMaxSizeLabel': 'Maximale logbestandsgrootte (MB)',
  'settings.logKeepFilesLabel': 'Te bewaren gearchiveerde logbestanden',
  'settings.logRotationHint':
    'De groottelimiet en het aantal gearchiveerde bestanden gelden de volgende keer dat PresenceJam start. Het logboek dat nu wordt geschreven telt niet mee: de logboekmap bevat hoogstens één bestand meer dan het ingestelde aantal. Logboekregistratie uitschakelen of het niveau wijzigen werkt onmiddellijk.',
  'settings.sectionBackup': 'Back-up',
  'settings.backupHint':
    'Exporteren schrijft een kopie van deze instellingen die u kunt bewaren of naar een andere machine kunt verplaatsen. Uw Spotify-clientgeheim blijft in de systeemsleutelhanger en wordt nooit opgenomen — en een bestand dat er een bevat wordt bij import geweigerd.',
  'settings.backupExport': 'Instellingen exporteren…',
  'settings.backupImport': 'Instellingen importeren…',
  'settings.backupExportDialogTitle': 'PresenceJam-instellingen exporteren',
  'settings.backupImportDialogTitle': 'PresenceJam-instellingen importeren',
  'settings.backupConfirmOverwrite':
    'Importeren vervangt al uw huidige instellingen. Het huidige bestand wordt ernaast bewaard als config.json.bak. Doorgaan?',
  'settings.backupExported': 'Instellingen geëxporteerd naar {path}',
  'settings.backupImported': 'Instellingen geïmporteerd uit {path}',
  'settings.backupError': 'De back-upactie kon niet worden voltooid: {error}',

  // 4.7.0 — S9 (issue #677: the tray snooze / "pause sync for a while")
  'dashboard.snoozeChip': 'Gesnoozed — nog {remaining} (tot {time})',
  'dashboard.snoozeResume': 'Nu hervatten',
  'dashboard.snoozeResuming': 'Hervatten…',
  'dashboard.snoozeResumeFailed':
    'Synchronisatie kon niet worden hervat. De snooze is nog opgeslagen — probeer het opnieuw.',
  // Issue #736: the chip's live region announces entry/exit once; the
  // per-second countdown is no longer in the live region.
  'dashboard.snoozeStatusEnd': 'Synchronisatie hervat',

  // 4.7.0 — S7 notifications (#675): one toggle per desktop-notification
  // class, plus the copy for the three classes the always-mounted layout
  // dispatches (track changes keep dispatching from the Dashboard card).
  'settings.notificationsTrackChange': 'Waarschuw mij wanneer het nummer wisselt',
  'settings.notificationsSyncStopped': 'Waarschuw mij wanneer synchronisatie vanzelf stopt',
  'settings.notificationsAuthRequired': 'Waarschuw mij wanneer ik mij opnieuw moet aanmelden bij Teams',
  'settings.notificationsUpdateStaged': 'Waarschuw mij wanneer een update bij afsluiten wordt geïnstalleerd',
  'notifications.syncStoppedTitle': 'PresenceJam is gestopt met synchroniseren',
  'notifications.syncStoppedBody':
    'De statussynchronisatie is vanzelf gestopt. Open PresenceJam om deze opnieuw te starten.',
  'notifications.authRequiredTitle': 'Teams-aanmelding vereist',
  'notifications.authRequiredBody':
    'Uw Teams-sessie is verlopen. Meld u opnieuw aan zodat uw status blijft synchroniseren.',
  'notifications.updateStagedTitle': 'Update gereed',
  'notifications.updateStagedBody': 'PresenceJam {version} wordt geïnstalleerd wanneer u afsluit.',
  // 4.7.0 — update channel (#678)
  'settings.sectionUpdates': 'Updates',
  'settings.updateChannelLabel': 'Releasekanaal',
  'settings.updateChannelStable': 'Stabiel',
  'settings.updateChannelBeta': 'Bèta',
  'settings.updateChannelHint':
    'Bètabuilds gebruiken de rollende bètafeed wanneer beschikbaar; als deze ontbreekt of geen nieuwere versie heeft, valt Bèta terug op de stabiele release. Bètabuilds installeren alleen bij afsluiten.',
  'update.betaOnQuitOnly':
    'Bètakanaal: updates installeren wanneer u afsluit — er is geen download-en-herstartpad op Bèta.',
  // 4.7.0 — S8 global hotkeys
  'settings.sectionShortcuts': 'Globale sneltoetsen',
  'settings.shortcutsHint':
    'Deze werken terwijl het venster verborgen is. Klik op een veld en druk de gewenste combinatie in — het veld registreert wat u indrukt, niet wat u typt.',
  'settings.shortcutTogglePlayback': 'Afspelen wisselen',
  'settings.shortcutToggleSync': 'Synchronisatie pauzeren of hervatten',
  'settings.shortcutUnbound': 'Niet ingesteld — klik en druk een combinatie in',
  'settings.shortcutClear': 'Wissen',
  'settings.shortcutRegistered': 'Actief',
  'settings.shortcutNotRegistered': 'Niet geregistreerd op dit bureaublad',
  'settings.shortcutCaptureReleased':
    'Losgelaten tijdens opname — de huidige binding zou vuren in plaats van te worden opgenomen',
  'settings.shortcutRejected': 'Kan niet worden gebruikt: {reason}',
  'settings.shortcutRegistrationFailed': 'Registratie mislukt op dit bureaublad: {reason}',
  // Issue #968: typed reason codes from the Rust validator. The Settings
  // card maps each `kind` to a dictionary entry so the rejection copy is
  // localized; `shortcutReasonUnknown` renders the backend's free-form text
  // for genuinely foreign refusals (compositor / app-owned combos).
  'settings.shortcutReasonNotAKey': '“{accelerator}” is geen herkende sneltoets',
  'settings.shortcutReasonConflict':
    'Conflicteert met de sneltoets {other} — één versneller kan niet beide acties aansturen',
  // Issue #810: a bare key would be grabbed system-wide. Function keys
  // (F1–F24) and media keys are exempt and bind without a modifier.
  'settings.shortcutReasonNeedsModifier':
    '“{accelerator}” heeft minimaal één modificatietoets nodig — een losse toets zou in elke applicatie worden gegrepen',
  'settings.shortcutReasonAutostart':
    'Starten bij aanmelden mislukt: {cause}',
  'settings.shortcutReasonUnknown': '{message}',
  'settings.shortcutReasonX11Unavailable':
    'Globale sneltoetsen hebben een bereikbaar X11-scherm nodig op dit bureaublad',
  'settings.shortcutReasonWorkerUnavailable':
    'De worker voor globale sneltoetsen kon niet worden geverifieerd; sneltoetsen zijn niet beschikbaar',
  // 4.7.0 — S12 hygiene (theme/density)
  'settings.themeSystem': 'Systeem',
  'settings.themeHint':
    '“Systeem” volgt de weergave van uw besturingssysteem; Donker en Licht blijven vastgezet.',
  'settings.densityCompactLabel': 'Compacte spatiëring',
  'settings.densityHint': 'Verkleint spatiëring en typschaal. Onafhankelijk van het thema.',
  // --- 5.0 wave1 i18n-lib ---
  // Key requests routed through this slice's dictionaries (the owning slice
  // cannot edit them).
  'onboarding.pollIntervalClamped':
    'Uw opgeslagen interval is {stored}s, buiten het bereik {min}–{max}s van deze stap — {seconds}s wordt gebruikt.',
  'update.stagingProgressLabel': 'Update voorbereiden',
  'rules.presenceAvailable': 'Beschikbaar',
  'rules.presenceBusyCall': 'Bezig — In gesprek',
  'rules.presenceBusyConference': 'Bezig — In telefonische vergadering',
  'rules.presenceAway': 'Afwezig',
  'rules.presenceDndPresenting': 'Niet storen — Presenteert',
  // --- 5.0 wave3 features-presence ---
  'dashboard.manualStatusTitle': 'Handmatige status',
  'dashboard.manualStatusPlaceholder': 'Stel een status in die uw team een tijdje kan zien',
  'dashboard.manualStatusExpiryLabel': 'Verloopt na',
  'dashboard.manualStatusExpiry15': '15 minuten',
  'dashboard.manualStatusExpiry30': '30 minuten',
  'dashboard.manualStatusExpiry60': '1 uur',
  'dashboard.manualStatusExpiry120': '2 uur',
  'dashboard.manualStatusSet': 'Status instellen',
  'dashboard.manualStatusClear': 'Status wissen',
  'dashboard.manualStatusActive': 'Actief tot {expiry}',
  'dashboard.manualStatusActiveEmpty': 'Actief (verloopt binnenkort)',
  'dashboard.manualStatusRecentTitle': 'Recente statussen',
  'dashboard.manualStatusRecentEmpty': 'Nog geen recente statussen',
  'dashboard.manualStatusFiltered': 'Status is herschreven door uw scheldwoordenfilter',
  'dashboard.activityTitle': 'Activiteit',
  'dashboard.activityEmpty': 'Nog geen beslissingen — start synchronisatie om te zien wat uw app koos',
  'dashboard.volumeLabel': 'Volume',
  'dashboard.volumeAria': 'Spotify-volumeschuif',
  'dashboard.seekAria': 'Klik om te zoeken',
  'dashboard.seekUnavailableAria': 'Dit apparaat ondersteunt zoeken niet',

  // --- 5.0 wave3 features-outlook ---
  'rules.importWorkingHours': 'Outlook-werkuren importeren',
  'rules.importWorkingHoursHint': 'Leest uw tabblad Werkuren en zet elk vrij blok om in een stille-urenregel. Bekijk vooraf een voorbeeld.',
  'rules.importWorkingHoursPreviewTitle': 'Voorbeeld van Outlook-werkuren',
  'rules.importWorkingHoursApply': 'Deze regels toepassen',
  'rules.importWorkingHoursReplace': 'Bestaande stille-urenregels vervangen',
  'rules.importWorkingHoursCancel': 'Annuleren',
  'rules.importWorkingHoursReplaceHint': 'Vervangt uw bestaande stille-urenregels door de geïmporteerde set. Vink uit om beide te behouden.',
  'rules.importWorkingHoursDaysLabel': 'Outlook meldt {days} werkend op {start}–{end}',
  'rules.importWorkingHoursDaysAllOff': 'Outlook meldt geen werkdagen — niets te importeren',

  // --- 5.0 wave3 features-gating ---

  // --- 5.0 wave3 features-profiles ---
  'rules.testTitle': 'Deze regels testen',
  'rules.testHint':
    'Typ een voorbeeldnummer, kies een minuut-van-de-dag + weekdag en zie precies welke regel (als er een) zou vuren.',
  'rules.testArtistLabel': 'Artiest',
  'rules.testTrackLabel': 'Nummer',
  'rules.testAlbumLabel': 'Album (optioneel)',
  'rules.testShowLabel': 'Show / podcast (optioneel)',
  'rules.testDeviceLabel': 'Spotify-apparaat (optioneel)',
  'rules.testPlaylistLabel': 'Afspeellijst-URI (optioneel)',
  'rules.testDurationLabel': 'Duur (mm:ss, optioneel)',
  'rules.testWeekdayLabel': 'Weekdag',
  'rules.testMinuteLabel': 'Minuut van de dag (HH:MM)',
  'rules.testRun': 'Test uitvoeren',
  'rules.testRunning': 'Uitvoeren…',
  'rules.testSummaryNoMatch': 'Geen regel zou overeenkomen met dit nummer.',
  'rules.testSummaryRuleMatched': 'Regel {index} zou vuren: {summary}',
  'rules.testStepMatched': 'kwam overeen',
  'rules.testStepNotMatched': 'kwam NIET overeen',
  'rules.testStepReason': 'reden: {reason}',
  'rules.testStepDisabled': 'regel is uitgeschakeld',
  'rules.testStepScheduleOutside': 'schema bevat deze minuut-van-de-dag niet',
  'rules.testStepNegated': 'negatie heeft de voorwaarden omgedraaid',
  'rules.matchKindLabel': 'Overeenkomststijl',
  'rules.matchKindSubstring': 'Subtekenreeks (standaard)',
  'rules.matchKindExact': 'Exacte overeenkomst',
  'rules.matchKindGlob': 'Glob-patroon',
  'rules.albumSubstringLabel': 'Album bevat…',
  'rules.showSubstringLabel': 'Show bevat…',
  'rules.deviceSubstringLabel': 'Apparaat bevat…',
  'rules.playlistUriLabel': 'Afspeellijst-URI bevat…',
  'rules.minDurationLabel': 'Minimale duur (seconden)',
  'rules.negateLabel': 'Negeren (overeenkomst wanneer voorwaarden NIET gelden)',
  'rules.actionLabel': 'Actie',
  'rules.actionSuppress': 'Status onderdrukken',
  'rules.actionReplace': 'Status vervangen door…',
  'rules.actionSnoozeMinutes': 'Snoozen voor minuten…',
  'rules.actionProfile': 'Overschakelen naar profiel…',
  'rules.actionPresence': 'Aanwezigheidspaar instellen…',
  'rules.actionReplaceStatusPlaceholder': 'Statustekst',
  'rules.actionSnoozePlaceholder': 'Minuten',
  'rules.actionProfilePlaceholder': 'Profielnaam',
  'rules.actionAvailabilityPlaceholder': 'Beschikbaarheid',
  'rules.actionActivityPlaceholder': 'Activiteit',
  'dashboard.gateWhyTitle': 'Waarom is de status gepauzeerd?',
  'dashboard.gateWhyShow': 'Toon waarom',
  'dashboard.gateWhyHide': 'Verberg waarom',
  'dashboard.gateWhyEmpty': 'Geen actieve poort op dit moment — de status synchroniseert normaal.',

  'profiles.sectionTitle': 'Aanwezigheidsprofielen',
  'profiles.sectionHint':
    'Bewaar een benoemde overlay (statusopmaak, poorten, regels) en wissel met het vak, een sneltoets of de CLI.',
  'profiles.empty': 'Geen profielen gedefinieerd.',
  'profiles.addProfile': 'Profiel toevoegen',
  'profiles.removeProfile': 'Verwijderen',
  'profiles.activeProfileLabel': 'Actief profiel',
  'profiles.activeProfileNone': 'Geen — gebruik de basisconfiguratie',
  'profiles.overlayStatusFormatLabel': 'Statusopmaak',
  'profiles.overlayClearOnPauseLabel': 'Status wissen bij pauze',
  'profiles.overlayAvailabilitySyncLabel': 'Beschikbaarheid synchroniseren met Teams',
  'profiles.overlayGateOutOfOfficeLabel': 'Poort bij afwezigheid',
  'profiles.overlayGatePresentingLabel': 'Poort bij volledig-scherm-apps',
  'profiles.overlayIdleAwayLabel': 'Stop adverteren na inactiviteit (seconden)',
  'profiles.overlayPreferredPresenceLabel': 'Voorkeursaanwezigheid',
  'profiles.overlayRulesLabel': 'Regelsubset',
  'profiles.overlayRulesHint': 'Vervangt de basisregellijst terwijl dit profiel actief is.',
  'profiles.overlayNotificationsLabel': 'Meldingen',
  'profiles.profileNameLabel': 'Profielnaam',
  'profiles.profileNamePlaceholder': 'bijv. Focus, Workout',
  'profiles.profileNameDuplicate': 'Er bestaat al een profiel met deze naam.',
  'profiles.profileNameTooLong': 'Profielnamen mogen maximaal 32 tekens zijn.',
  'profiles.profileNameMissing': 'Profielnaam mag niet leeg zijn.',
  'profiles.activeProfileUnknown': 'Onbekend profiel — basisconfiguratie wordt gebruikt.',
  'tray.profilesMenu': 'Aanwezigheidsprofielen',
  'tray.profilesMenuNone': 'Geen profielen gedefinieerd',
  'tray.profilesMenuActivateBase': 'Basisconfiguratie gebruiken',
  'cli.profileFlag': 'Schakel over naar een benoemd aanwezigheidsprofiel en sluit af.',
  'cli.profileActive': 'Actief profiel is nu "{name}".',
  'cli.profileUnknown': 'Geen profiel met de naam "{name}" — basisconfiguratie blijft actief.',
  'settings.shortcutToggleProfile': 'Aanwezigheidsprofielen doorlopen',

  // --- 5.0 wave3 playback-source ---
  'onboarding.playbackSourceMacNote':
    "U gebruikt macOS — hier is alleen de Spotify-afspeelbron beschikbaar. De systeemmediasessiebron (Windows SMTC / Linux MPRIS) is niet beschikbaar op macOS, dus valt PresenceJam automatisch terug op Spotify. Schakel over naar Spotify als de wizard ooit \"geen nummer\" meldt terwijl in een andere app muziek speelt.",

  // --- #1120: the snooze entry announcement is count-aware. A one-minute
  // snooze used to announce "Sync paused for 1 minutes"; German needs
  // "Minute" and French "minute" in the singular.
  'dashboard.snoozeStatusStart_one': 'Synchronisatie {minutes} minuut gepauzeerd',
  'dashboard.snoozeStatusStart_other': 'Synchronisatie {minutes} minuten gepauzeerd',

  // --- #739: the wizard's view name, announced on navigation (there is no
  // heading bar above the step, so the step title cannot stand in for it).
  'onboarding.title': 'Installatie',

  // --- #954 / P7: the compact header's abbreviated sync badge. The full
  // "Synchronisierung" / "Synchronisation" label ellipsises into an unreadable
  // fragment at the 400px minimum, and a second badge row is worse. "Sync" is
  // the established short form in all three locales.
  'dashboard.syncingShort': 'Sync',
  // --- #966 / #981 Settings draft actions ---
  'settings.revertChanges': 'Wijzigingen terugdraaien',
  'rules.undoRemove': 'Verwijderen ongedaan maken',
};
