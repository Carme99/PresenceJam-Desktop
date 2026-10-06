/**
 * C6 i18n foundation (docs/scope-3.3.md §C6) — Italian dictionary.
 * Typed against `Dict`, so every English key must be present.
 *
 * Model-written Italian translation (issue #984) — human review pending before this copy is considered final.
 */

import type { Dict } from './en';

export const it: Dict = {
  // ── common ────────────────────────────────────────────────────────
  'common.back': 'Indietro',
  'common.backToDashboard': 'Torna alla dashboard',
  'common.checkNow': 'Verifica lo stato di accesso',
  'common.connected': 'Connesso',
  'common.dismiss': 'Ignora',
  'common.openSignInPage': 'Apri la pagina di accesso Microsoft:',
  'common.enterCodeWhenAsked': 'Inserisci questo codice quando richiesto:',
  'common.moreActions': 'Altre azioni',
  'common.launchAtLogin': 'Avvia all\u2019accesso',
  'common.loading': 'Caricamento…',
  'common.notConnected': 'Non connesso',
  'common.reconnecting': 'Riconnessione…',
  'common.reconnect': 'Riconnetti',
  'common.retry': 'Riprova',
  'common.bootFailed': 'Impossibile caricare lo stato dell\u2019app',
  'common.resetToDefault': 'Ripristina i valori predefiniti',
  'common.themeToggle': 'Cambia tema',
  'common.waiting': 'In attesa…',
  'common.waitingForSignIn': 'In attesa dell\u2019accesso…',
  'common.codeExpiresIn': 'Il codice scade tra {time}',
  'common.codeExpired': 'Questo codice è scaduto — non può più essere utilizzato.',
  'common.getNewCode': 'Richiedi un nuovo codice',
  'common.yes': 'Sì',
  'common.no': 'No',
  'common.tagline': 'Spotify → Stato di Teams',

  // ── dashboard ─────────────────────────────────────────────────────
  'dashboard.spotifyOff': 'Spotify disattivato',
  'dashboard.teamsOff': 'Teams disattivato',
  'dashboard.syncing': 'Sincronizzazione',
  'dashboard.logsDetachedTitle': 'Registri (staccati — fai clic per attivare)',
  'dashboard.logsTitle': 'Registri',
  'dashboard.logsDetachedAria': 'Registri (staccati in una finestra separata)',
  'dashboard.openLogsAria': 'Apri i registri',
  'dashboard.diagnostics': 'Diagnostica',
  'dashboard.openDiagnosticsAria': 'Apri la diagnostica',
  'dashboard.settingsDetachedTitle': 'Impostazioni (staccate — fai clic per attivare)',
  'dashboard.settings': 'Impostazioni',
  'dashboard.settingsDetachedAria': 'Impostazioni (staccate in una finestra separata)',
  'dashboard.openSettingsAria': 'Apri le impostazioni',
  'dashboard.about': 'Informazioni',
  'dashboard.aboutAria': 'Informazioni su PresenceJam',
  'dashboard.pauseSync': 'Metti in pausa la sincronizzazione',
  'dashboard.resumeSync': 'Riprendi la sincronizzazione',
  'dashboard.presenceGated': 'Stato in pausa — sei occupato, in chiamata o in presentazione',
  'dashboard.setupRequired': 'Configurazione richiesta',
  'dashboard.setupHint':
    'Collega Spotify e Microsoft Teams così i brani in riproduzione aggiorneranno lo stato di Teams.',
  'dashboard.continueSetup': 'Continua la configurazione',
  'dashboard.playing': 'In riproduzione',
  'dashboard.paused': 'In pausa',
  'dashboard.liveStreamAria': 'Diretta streaming — posizione sconosciuta',
  'dashboard.yourTeamsStatus': 'Il tuo stato di Teams',
  'dashboard.nothingPlaying': 'Nulla in riproduzione',
  'dashboard.nothingPlayingHint':
    'Riproduci qualcosa su Spotify e apparirà nello stato di Teams.',
  'dashboard.syncCrashed': 'La sincronizzazione si è interrotta inaspettatamente. Premi riprendi (▶) per riavviarla.',
  'dashboard.credentialCheckFailed': 'Impossibile verificare le credenziali. Controlla la connessione e riprova.',
  'dashboard.syncToggleFailed': 'Impossibile avviare/interrompere la sincronizzazione. Riprova — se il problema persiste, apri Diagnostica dall\u2019intestazione della dashboard.',
  'dashboard.statusNotConfigured': 'Non configurato',
  'dashboard.statusNoTrack': 'Nessun brano in riproduzione',
  'dashboard.live': 'Diretta',
  'dashboard.refreshStatus': 'Aggiorna lo stato',
  'dashboard.refreshing': 'Aggiornamento…',
  'dashboard.refreshFailed': 'Impossibile aggiornare lo stato. Riprova.',
  'dashboard.refreshAria': 'Aggiorna ora lo stato di Teams',

  // ── logs ──────────────────────────────────────────────────────────
  'logs.title': 'Registri',
  'logs.filterAria': 'Filtro del livello di registro',
  'logs.level.all': 'Tutti',
  'logs.level.trace': 'Traccia',
  'logs.level.debug': 'Debug',
  'logs.level.info': 'Info',
  'logs.level.warning': 'Avviso',
  'logs.level.error': 'Errore',
  'logs.count_one': '{count} voce',
  'logs.count_other': '{count} voci',
  'logs.showingOf': 'Visualizzate {shown} di {total}',
  'logs.jumpToLatest': 'Vai alla più recente',
  'logs.popOut': 'Stacca',
  'logs.clear': 'Cancella',
  'logs.openFolder': 'Apri la cartella',
  'logs.empty': 'Nessuna voce di registro',
  'logs.emptyHint': 'Le voci in tempo reale appariranno qui quando la sincronizzazione si avvia e Spotify è in riproduzione.',

  // ── settings ──────────────────────────────────────────────────────
  'settings.title': 'Impostazioni',
  'settings.popBackIn': 'Riaggancia',
  'settings.popOutActionTitle': 'Stacca in una finestra separata',
  'settings.unsavedChanges': 'Modifiche non salvate',
  'settings.sectionSpotify': 'Spotify',
  'settings.sectionTeams': 'Microsoft Teams',
  'settings.sectionPresence': 'Presenza',
  'settings.sectionStatusFormat': 'Formato dello stato',
  'settings.sectionPolling': 'Frequenza di sincronizzazione',
  'settings.sectionNotifications': 'Notifiche',
  'settings.sectionAppearance': 'Aspetto',
  'settings.clientId': 'ID client',
  'settings.clientIdPlaceholder': 'Inserisci l\u2019ID client di Spotify',
  'settings.clientSecret': 'Segreto client',
  'settings.secretStoredHint':
    'Conservato in modo sicuro nel portachiavi del sistema operativo. Per sostituirlo, torna alla dashboard e scegli Continua la configurazione.',
  'settings.secretNotConfigured': 'Non configurato.',
  'settings.runOnboarding': 'Avvia la configurazione guidata',
  'settings.toSetUpSpotify': 'per configurare Spotify.',
  'settings.reconnectSpotify': 'Riconnetti Spotify',
  'settings.completeAuthInBrowser': 'Completa l\u2019autenticazione nel browser.',
  'settings.playbackScopeBanner': 'Spotify ha aggiunto i controlli di riproduzione. Fai clic su Riconnetti accanto a questo messaggio per abilitarli.',
  'settings.spotifySecretConflict':
    'Il segreto client nel file di configurazione è diverso da quello nel portachiavi. Riconnetti Spotify per risolvere il problema.',
  'settings.teamsAuthHint':
    'L\u2019autenticazione di Teams utilizza l\u2019account Microsoft 365. Non è richiesta alcuna configurazione aggiuntiva.',
  'settings.presenceScopeBanner': 'Teams ha aggiunto il rilevamento di riunioni/chiamate. Fai clic su Riconnetti accanto a questo messaggio per abilitarlo.',
  'settings.availabilitySyncLabel': 'Mostra Disponibile durante l\u2019ascolto',
  'settings.availabilitySyncHint':
    'Disattivato per impostazione predefinita. Quando attivo, Teams ti mostra come Disponibile (invece di Occupato) durante la riproduzione musicale. Nota: Teams mostra comunque Occupato durante chiamate e riunioni.',
  'settings.presenceGateLabel': 'Metti in pausa lo stato durante riunioni/chiamate/Non disturbare',
  'settings.presenceGateHint':
    'Attivato per impostazione predefinita. Salta la scrittura dello stato di Spotify mentre Teams indica occupato, in riunione, in chiamata o in presentazione.',
  'settings.formatTemplate': 'Modello di formato',
  'settings.formatTemplatePlaceholder': '🎵 {artist} - {track} 🎧',
  'settings.livePreview': 'Anteprima dal vivo',
  'settings.placeholdersHint':
    'Segnaposto disponibili: {artist}, {track}, {album}, {emoji}, {device}, {playlist} (oppure {context}), {progress}, {shuffle}, {repeat}. Mescolamento e Ripetizione mostrano 🔀/🔁 solo quando sono attivi.',
  'settings.profanityFilterLabel': 'Filtra le volgarità nello stato',
  'settings.placeholderTextLabel': 'Testo segnaposto',
  'settings.placeholderTextHint':
    'Usa {emoji} per lo stato di riproduzione (🎵 in riproduzione / ⏸ in pausa). Mostrato quando vengono rilevate volgarità nelle informazioni del brano.',
  'settings.placeholderTextPlaceholder': 'In ascolto su Spotify',
  'settings.profaneSampleToggle': 'Anteprima con un brano di esempio volgare',
  'settings.defaultIntervalLabel': 'Intervallo predefinito: {seconds}s',
  'settings.minIntervalLabel': 'Intervallo minimo (s)',
  'settings.maxIntervalLabel': 'Intervallo massimo (s)',
  'settings.clampHint':
    'L\u2019intervallo minimo supera l\u2019intervallo massimo — il valore massimo verrà salvato come {max}s.',
  'settings.notificationsHint':
    'Ogni categoria abilitata mostra una notifica di sistema — la prima potrebbe richiedere l\u2019autorizzazione del sistema operativo.',
  'settings.themeLabel': 'Tema',
  'settings.themeDark': 'Scuro',
  'settings.themeLight': 'Chiaro',
  'settings.languageLabel': 'Lingua',
  'settings.saveChanges': 'Salva le modifiche',
  'settings.saving': 'Salvataggio…',
  'settings.saved': 'Impostazioni salvate.',
  'settings.failedToSave': 'Impossibile salvare le impostazioni — le modifiche sono ancora qui. Riprova.',
  'settings.openLogsFolder': 'Apri la cartella dei registri',
  'settings.previewUnavailable': '(anteprima non disponibile)',

  // ── diagnostics ───────────────────────────────────────────────────
  'diagnostics.title': 'Diagnostica',
  'diagnostics.localOnlyHint': 'Istantanea solo locale — sicura da allegare a una segnalazione di bug.',
  'diagnostics.copy': 'Copia la diagnostica',
  'diagnostics.saveToFile': 'Salva su file',
  'diagnostics.copied': 'Diagnostica copiata negli appunti.',
  'diagnostics.copyFailed': 'Copia non riuscita — usa invece «Salva su file».',
  'diagnostics.savedToDownloads': 'Diagnostica salvata nella cartella dei download.',
  'diagnostics.saveFailed': 'Salvataggio non riuscito — usa invece «Copia la diagnostica».',
  'diagnostics.collecting': 'Raccolta della diagnostica…',
  'diagnostics.collectFailed': 'Impossibile raccogliere la diagnostica',
  'diagnostics.versions': 'Versioni',
  'diagnostics.configuration': 'Configurazione',
  'diagnostics.connections': 'Connessioni',
  'diagnostics.recentLogLines': 'Righe di registro recenti',
  'diagnostics.app': 'PresenceJam',
  'diagnostics.tauri': 'Tauri',
  'diagnostics.os': 'Sistema operativo',
  'diagnostics.osRelease': 'Versione',
  'diagnostics.installFlavor': 'Tipo di installazione',
  'diagnostics.unknown': 'sconosciuto',
  'diagnostics.spotifyClientId': 'ID client di Spotify',
  'diagnostics.redirectUri': 'URI di reindirizzamento',
  'diagnostics.notSet': '(non impostato)',
  'diagnostics.clientSecretKeychain': 'Segreto client nel portachiavi',
  'diagnostics.clearOnPause': 'Cancella lo stato in pausa',
  'diagnostics.profanityFilter': 'Filtro volgarità',
  'diagnostics.extraProfanityWords': 'Parole volgari aggiuntive',
  'diagnostics.startMinimized': 'Avvia ridotto a icona',
  'diagnostics.availabilitySync': 'Sincronizzazione disponibilità di Teams',
  'diagnostics.presenceGate': 'Blocco di presenza',
  'diagnostics.pollInterval': 'Intervallo di polling (predefinito/min/max)',
  'diagnostics.logging': 'Registrazione',
  'diagnostics.loggingEnabled': 'abilitata ({level})',
  'diagnostics.loggingDisabled': 'disabilitata',
  'diagnostics.respectManualStatus': 'Rispetta lo stato manuale',
  'diagnostics.gateOutOfOffice': 'Blocco quando fuori sede',
  'diagnostics.locale': 'Lingua',
  'diagnostics.updateChannel': 'Canale di aggiornamento',
  'diagnostics.configSnoozed': 'Sincronizzazione posticipata',
  'diagnostics.launchAtLogin': 'Avvia all\u2019accesso',
  'diagnostics.statusRules': 'Regole di stato',
  'diagnostics.statusRulesValue': '{quiet}/{quietTotal} ore di silenzio, {rules}/{rulesTotal} regole brano attive',
  'diagnostics.spotifyConnected': 'Spotify connesso',
  'diagnostics.spotifyTokenExpires': 'Il token Spotify scade',
  'diagnostics.teamsConnected': 'Teams connesso',
  'diagnostics.teamsTokenExpires': 'Il token Teams scade',
  'diagnostics.expired': '(scaduto)',
  'diagnostics.keychainSpotifySecret': 'Portachiavi: segreto Spotify presente',
  'diagnostics.keychainEncryptionKey': 'Portachiavi: chiave di crittografia dei token presente',
  'diagnostics.tokensNeverIncluded':
    'I valori dei token non sono mai inclusi — solo timestamp di scadenza e indicatori di presenza.',
  'diagnostics.noLogLinesYet': 'Nessuna riga di registro disponibile.',
  'diagnostics.failedInstallTitle': 'Installazione aggiornamento non riuscita',
  'diagnostics.failedInstallVersion': 'Versione',
  'diagnostics.failedInstallError': 'Errore',
  'diagnostics.failedInstallTimestamp': 'Tentativo effettuato il',
  'diagnostics.failedInstallDismissFailed': 'Impossibile ignorare il record di installazione non riuscita.',
  'diagnostics.resetTokenStorage': 'Reimposta l\u2019archivio locale dei token',
  'diagnostics.resetTokenConfirm':
    'Questa operazione elimina tokens.json e i file associati più la chiave di crittografia dei token archiviata. Dovrai accedere di nuovo. Continuare?',
  'diagnostics.resetTokenDone': 'Archivio dei token reimpostato. Accedi di nuovo per riprendere la sincronizzazione.',
  'diagnostics.resetTokenFailed': 'Reimpostazione non riuscita — {error}',
  'diagnostics.syncRunning': 'Sincronizzazione in corso',
  'diagnostics.syncSnoozed': 'Posticipata',
  'diagnostics.syncSnoozeMinutes': '{minutes} min rimanenti',
  'diagnostics.syncManualStatusBlocks': 'Lo stato manuale blocca le scritture',
  'diagnostics.syncPresenceGateReason': 'Motivo del blocco di presenza',
  'diagnostics.syncTransientFailures': 'Errori di autenticazione consecutivi',
  'diagnostics.syncNetworkFailures': 'Errori di rete consecutivi',

  // ── reconnect ─────────────────────────────────────────────────────
  'reconnect.title': 'Riconnetti',
  'reconnect.description': 'La sincronizzazione richiede attenzione. Riconnettiti qui sotto per riprendere.',
  'reconnect.missingCredentials': 'Credenziali mancanti',
  'reconnect.failed': 'Non riuscito',
  'reconnect.readyToReconnect': 'Pronto per la riconnessione',
  'reconnect.spotifyOk': 'Spotify riconnesso correttamente.',
  'reconnect.spotifyNotConfigured':
    'Le credenziali di Spotify non sono configurate su questo computer.',
  'reconnect.completeAuthInOpenedBrowser':
    'Completa l\u2019autenticazione nella finestra del browser aperta.',
  'reconnect.tryAgain': 'Riprova',
  'reconnect.clickBelowSpotify': 'Fai clic qui sotto per riconnettere l\u2019account Spotify.',
  'reconnect.teamsOk': 'Teams riconnesso correttamente.',
  'reconnect.clickBelowTeams':
    'Fai clic qui sotto per riconnettere l\u2019account Microsoft Teams.',
  'reconnect.tokenStorageUnusable': 'I dati di accesso salvati non possono essere letti — la chiave di crittografia dei token archiviata non è utilizzabile.',
  'reconnect.resetTokenStorage': 'Reimposta l\u2019archivio locale dei token',
  'reconnect.resetTokenConfirm':
    'Questa operazione elimina tokens.json e i file associati più la chiave di crittografia dei token archiviata. Dovrai accedere di nuovo. Continuare?',
  'reconnect.resetTokenDone': 'Archivio dei token reimpostato. Accedi di nuovo qui sotto per riprendere.',
  'reconnect.resetTokenFailed': 'Reimpostazione non riuscita — {error}',
  'reconnect.missingCredsTitle': 'Credenziali Spotify mancanti?',
  'reconnect.reenterCredsHint':
    'Dovrai reinserire l\u2019ID client e il segreto client.',
  'reconnect.goToFullSetup': 'Vai alla configurazione completa',
  'reconnect.reconnectTeams': 'Riconnetti Teams',

  // ── about ─────────────────────────────────────────────────────────
  'about.version': 'Versione {version}',
  'about.description':
    'Mostra ciò che ascolti su Spotify nello stato di Microsoft Teams — automaticamente.',
  'about.statusSync': 'Sincronizzazione dello stato',
  'about.live': 'Attiva',
  'about.auth': 'Accesso',
  'about.authMethod': 'Spotify + Microsoft',
  'about.storage': 'Archiviazione',
  'about.osKeychain': 'Portachiavi del sistema operativo',
  'about.githubRepo': 'Repository GitHub',
  'about.releases': 'Versioni',
  'about.reportIssue': 'Segnala un problema',

  // ── update banner ─────────────────────────────────────────────────
  'update.available': 'Aggiornamento v{version} disponibile',
  'update.stagedQuit': 'La v{version} verrà installata quando chiudi PresenceJam',
  'update.downloadFailed': 'Download non riuscito — {error}',
  'update.downloadAndInstall': 'Scarica e installa',
  'update.downloading': 'Download…',
  'update.installOnQuit': 'Installa alla chiusura',
  'update.preparing': 'Preparazione…',
  'update.dismissAria': 'Ignora il banner di aggiornamento',
  'update.confirmQuitInstall':
    'Installare la v{staged} alla chiusura? Versione attuale: v{current}.',
  'update.confirmQuitInstallUnknown': 'Installare la v{staged} alla chiusura?',
  'update.stagedVsCurrent':
    'La v{staged} verrà installata alla chiusura (attuale v{current})',
  'update.staleSkipped':
    'La v{staged} è stata ignorata — la v{current} attuale è più recente.',
  'update.staleSkippedUnknown':
    'La v{staged} è stata ignorata — non è più recente della versione attuale.',
  'update.installAnyway': 'Installa comunque',
  // #894: a `.deb` / `.rpm` install is updated by its package manager, not by
  // the in-app updater — the release manifest only offers the AppImage payload,
  // which neither installer can apply. The banner shows the command instead of
  // an install button.
  'update.packageManagedDeb':
    'La v{version} è disponibile. Questa copia è stata installata come pacchetto .deb, quindi gli aggiornamenti arrivano da apt.',
  'update.packageManagedRpm':
    'La v{version} è disponibile. Questa copia è stata installata come pacchetto .rpm, quindi gli aggiornamenti arrivano da dnf.',
  'update.packageManagerInstallDeb': 'sudo apt install ./{file}',
  'update.packageManagerInstallRpm': 'sudo dnf install ./{file}',

  // ── onboarding ────────────────────────────────────────────────────
  'onboarding.stepOf': 'Passaggio {step} di 3',
  'onboarding.step1Title': 'Collega Spotify',
  'onboarding.step1Intro':
    'Incolla qui sotto l\u2019ID client e il segreto client di Spotify, quindi scegli Collega Spotify — apriremo la pagina di accesso di Spotify.',
  'onboarding.getCredentials': 'Ottieni le credenziali Spotify',
  'onboarding.instruction1':
    'Apri la dashboard per sviluppatori di Spotify e crea un\u2019app.',
  'onboarding.instruction2': 'In URI di reindirizzamento, aggiungi presencejam://callback (indica a Spotify dove rimandarti).',
  'onboarding.instruction3':
    'Copia l\u2019ID client e il segreto client dalle impostazioni dell\u2019app.',
  'onboarding.clientIdPlaceholder': 'ID client Spotify di 32 caratteri',
  'onboarding.clientSecretPlaceholder': 'Segreto client di Spotify',
  'onboarding.connectSpotify': 'Collega Spotify',
  'onboarding.signInWaiting': 'Accesso a Spotify in attesa…',
  'onboarding.manualUrlHint':
    'Completa l\u2019accesso con Spotify nel browser, quindi incolla qui sotto l\u2019indirizzo completo dalla barra degli indirizzi.',
  'onboarding.manualUrlLabel': 'URL di reindirizzamento di Spotify',
  'onboarding.manualUrlPlaceholder': 'presencejam://callback?code=…',
  'onboarding.submitCode': 'Invia il codice',
  'onboarding.connectedToSpotify': 'Connesso a Spotify',
  'onboarding.continue': 'Continua →',
  'onboarding.step2Title': 'Collega Microsoft Teams',
  'onboarding.step2Intro':
    'Utilizziamo il flusso con codice dispositivo di Microsoft — un codice monouso da inserire in una pagina Microsoft. Non è richiesta alcuna configurazione aggiuntiva.',
  'onboarding.startMicrosoftSignIn': 'Collega Microsoft Teams',
  'onboarding.connectedToTeams': 'Connesso a Microsoft Teams',
  'onboarding.step3Title': 'Tocchi finali',
  'onboarding.step3Intro':
    'Scegli l\u2019aspetto del messaggio di stato e se PresenceJam deve avviarsi all\u2019accesso.',
  'onboarding.statusTemplate': 'Modello di stato',
  'onboarding.placeholdersHint':
    'Segnaposto: {artist}, {track}, {album}, {emoji}, {device}, {playlist} (oppure {context}), {progress}, {shuffle}, {repeat}',
  'onboarding.pollInterval': 'Frequenza di controllo di Spotify: {seconds}s',
  'onboarding.settingUp': 'Configurazione…',
  'onboarding.finishSetup': 'Completa la configurazione',

  // ── validation / errors ───────────────────────────────────────────
  'validation.clientIdRequired': 'L\u2019ID client di Spotify è obbligatorio.',
  'validation.clientIdFormat':
    'L\u2019ID client di Spotify deve essere esattamente di 32 caratteri esadecimali.',
  'validation.clientSecretRequired': 'Il segreto client di Spotify è obbligatorio.',
  'validation.clientSecretTooShort':
    'Il segreto client sembra troppo corto — dovrebbe essere di almeno 32 caratteri. Controlla eventuali errori di copia-incolla.',
  'validation.noCodeInUrl':
    'Quell\u2019URL non contiene alcun codice di accesso — incolla l\u2019indirizzo completo dalla barra degli indirizzi del browser dopo il reindirizzamento di Spotify.',
  'validation.connectBothFirst':
    'Collega sia Spotify che Teams prima di completare la configurazione.',
  'validation.setupFailed': 'Configurazione non riuscita: {error}',

  // ── routes / chrome ───────────────────────────────────────────────
  'routes.skipToMainContent': 'Vai al contenuto principale',
  'routes.unknownPane': 'Riquadro sconosciuto: {pane}',

  // ── feat/45-features: status rules (#432) + support snapshot (#434) ──
  'rules.sectionTitle': 'Regole di stato',
  'rules.sectionHint':
    'Le ore di silenzio e le regole brano sopprimono la scrittura dello stato di Teams, riutilizzando lo stesso percorso del blocco di presenza — una regola cancellata pubblica automaticamente a metà brano.',
  'rules.quietHoursLabel': 'Ore di silenzio',
  'rules.noQuietHours': 'Nessuna ora di silenzio definita — lo stato si sincronizza a tutte le ore.',
  'rules.quietStart': 'Inizio delle ore di silenzio',
  'rules.quietEnd': 'Fine delle ore di silenzio',
  'rules.quietDays': 'Giorni attivi (nessuna selezione = ogni giorno)',
  'rules.dayEveryDay': 'Ogni giorno',
  'rules.day1': 'Lun',
  'rules.day2': 'Mar',
  'rules.day3': 'Mer',
  'rules.day4': 'Gio',
  'rules.day5': 'Ven',
  'rules.day6': 'Sab',
  'rules.day7': 'Dom',
  'rules.addQuietHours': 'Aggiungi ore di silenzio',
  'rules.trackRulesLabel': 'Regole brano',
  'rules.noTrackRules': 'Nessuna regola brano definita — tutti i brani si sincronizzano normalmente.',
  'rules.artistPlaceholder': 'L\u2019artista contiene…',
  'rules.trackPlaceholder': 'Il titolo del brano contiene…',
  'rules.replacementPlaceholder': 'Pubblica invece questo (vuoto = sopprimi)',
  'rules.addTrackRule': 'Aggiungi regola brano',
  'rules.removeRule': 'Rimuovi',
  'rules.ruleEnabled': 'Abilitata',
  'logs.copySnapshot': 'Copia istantanea',
  'logs.snapshotCopied': 'Istantanea oscurata copiata negli appunti.',
  'logs.snapshotCopyFailed': 'Impossibile copiare l\u2019istantanea.',
  'logs.openFolderError':
    'Impossibile aprire la cartella dei registri. Potrebbe non esistere ancora — prova a riavviare l\u2019app per crearla.',
  // 4.6 additions
  'dashboard.availabilityListening': 'In ascolto (Disponibile)',
  'dashboard.availabilityCleared': 'Disponibilità cancellata',
  'settings.saveAndLeave': 'Salva ed esci',
  'settings.discardChanges': 'Ignora le modifiche',
  'settings.stayHere': 'Resta qui',
  'settings.notificationsDenied':
    'Le notifiche sono bloccate dal sistema. Abilitale nelle impostazioni di sistema, quindi riattiva questa opzione.',
  'diagnostics.expiryBuffer': 'Margine di aggiornamento del token',
  'diagnostics.teamsRefreshTokenPresent': 'Token di aggiornamento di Teams archiviato',
  'reconnect.restartSignIn': 'Riavvia l\u2019accesso',
  'update.stagingProgress': 'Preparazione dell\u2019aggiornamento — {percent}%',
  'update.cancelStage': 'Annulla',
  'onboarding.submitting': 'Invio…',
  'settings.episodeFormatHint':
    'I podcast e gli audiolibri usano un proprio modello — 🎙️ {show} - {episode} — quindi il modello musicale qui sopra non viene applicato a loro.',
  'reconnect.keychainUnavailableBadge':
    'Portachiavi non disponibile',
  'reconnect.keychainUnavailableHint':
    'PresenceJam non ha potuto leggere il segreto client Spotify salvato: il portachiavi di sistema è bloccato o mancante. Sbloccalo (o installa un fornitore di Secret Service come gnome-keyring), quindi riprova — il segreto è ancora archiviato, quindi non devi configurare di nuovo Spotify.',
  'settings.secretKeychainUnavailable':
    'Portachiavi di sistema non disponibile — potrebbe essere bloccato o mancante. Sbloccalo (o installa un fornitore di Secret Service) per usare il segreto salvato; è ancora archiviato.',
  'diagnostics.quarantineTitle': 'Le impostazioni sono state reimpostate',
  'diagnostics.quarantineBodyNow':
    'PresenceJam non ha potuto leggere il file delle impostazioni, quindi ogni impostazione è stata reimpostata al valore predefinito.',
  'diagnostics.quarantineBodyEarlier':
    'Un avvio precedente non ha potuto leggere il file delle impostazioni e lo ha reimpostato ai valori predefiniti.',
  'diagnostics.quarantineBackupPresent':
    'L\u2019originale illeggibile è stato conservato accanto al file delle impostazioni come {name}, così i valori che conteneva possono ancora essere recuperati.',
  'diagnostics.quarantineBackupMissing':
    'L\u2019originale illeggibile si trova ancora accanto al file delle impostazioni come config.json.',
  'diagnostics.quarantineWhere':
    'Entrambi i file si trovano nella cartella PresenceJam all\u2019interno della cartella di configurazione utente — il backup è accanto a config.json.',
  // 4.6 additions (presence rules #634/#635/#636/#637 + #538 consumption sites)
  'rules.replacementClampHint':
    'Lo stato sostitutivo è limitato a {max} caratteri; il resto non viene pubblicato.',
  'rules.presenceLabel': 'Presenza mentre si applica questa regola',
  'rules.presenceNone': 'Non modificare la mia presenza',
  'rules.presenceHint':
    'Una regola può impostare disponibilità/attività di Teams, ma solo mentre la «sincronizzazione disponibilità» è attiva; non ha mai la precedenza su una chiamata, una riunione o uno stato impostato manualmente.',
  'settings.respectManualStatusLabel': 'Non sovrascrivere mai uno stato impostato manualmente',
  'settings.respectManualStatusHint':
    'Riutilizza la lettura di presenza già eseguita dal blocco, quindi non costa alcuna richiesta aggiuntiva — il testo viene rispettato finché non lo modifichi o scade.',
  'settings.gateOutOfOfficeLabel': 'Metti in pausa quando sono fuori sede',
  'settings.gateOutOfOfficeHint':
    'Salta l\u2019aggiornamento dello stato mentre l\u2019impostazione fuori sede di Teams è attiva. Una regola brano con una propria azione di presenza ha la precedenza.',
  // Issue #872: the OS-level presentation gate (full-screen app, slide
  // deck, Windows Focus Assist Quiet Time). OFF by default, matching how
  // `availabilitySync` and `gateWhenOutOfOffice` shipped — 4.7 behaviour
  // is unchanged until the user opts in.
  'settings.gateWhenPresentingLabel': 'Metti in pausa mentre presento',
  'settings.gateWhenPresentingHint':
    'Salta la scrittura dello stato di Spotify mentre il sistema operativo segnala un\u2019app a schermo intero, una presentazione o un periodo di silenzio. Linux/macOS non segnalano mai questo segnale, quindi l\u2019interruttore non ha effetto lì.',
  // Issue #873: the desktop-idle gate. `0` (the default) keeps 4.7
  // behaviour — the app keeps advertising listening until the user
  // explicitly opts in.
  'settings.idleAwayLabel': 'Metti in pausa quando il desktop è inattivo',
  'settings.idleAwayHint':
    'Smette di pubblicizzare lo stato di Spotify quando il desktop non riceve input da tastiera/mouse da questo numero di secondi. 60–3600; 0 disabilita. Linux/macOS non segnalano mai questo segnale.',
  'settings.extraWordsLabel': 'Parole personalizzate da filtrare',
  'settings.extraWordsHint':
    'Una parola o frase per riga. Applicate con gli stessi confini dell\u2019elenco integrato.',
  'settings.extraWordsPlaceholder': 'parola o frase',
  'settings.extraWordsClampHint':
    'Solo le prime {max} voci di {chars} caratteri vengono conservate — {kept} sarà filtrato.',
  'settings.pauseBackoffMaxLabel': 'Tetto di backoff in pausa (secondi)',
  'settings.pauseBackoffClampHint':
    'L\u2019intervallo consentito è {min}–{max} secondi; verrà utilizzato {effective}.',
  'dashboard.presenceGatedQuietHours': 'Stato in pausa — le ore di silenzio sono attive',
  'dashboard.presenceGatedTrackRule': 'Stato in pausa — una regola brano è stata attivata',
  'dashboard.presenceGatedManualStatus':
    'Stato in pausa — hai impostato manualmente un messaggio di stato',
  'dashboard.presenceGatedOutOfOffice': 'Stato in pausa — sei fuori sede',
  'dashboard.presenceGatedPresenting':
    'Stato in pausa — stai presentando o sei in un\u2019app a schermo intero',
  'dashboard.presenceGatedQuietTime': 'Stato in pausa — la modalità Non disturbare è attiva',
  'dashboard.presenceGatedIdle': 'Stato in pausa — il desktop è inattivo',
  // 4.7.0 — S6 (tray localization): `config.locale` is the single source of
  // truth, so the picker also drives the tray and the native app menu.
  'settings.languageHint':
    'Si applica anche al menu della barra delle applicazioni e al menu nativo dell\u2019applicazione.',
  // #984: the follow-system-language toggle beside the picker. The mode
  // re-resolves from the OS language on every boot (and on change) and
  // persists the resolution, so the tray and the native menu follow too.
  'settings.languageFollowSystemLabel': 'Segui la lingua del sistema',
  'settings.languageFollowSystemHint':
    'Quando attiva, il selettore di lingua qui sopra viene ignorato: l\u2019app rileva di nuovo la lingua del sistema operativo a ogni avvio. Le lingue di sistema non supportate ricadono comunque sull\u2019inglese.',

  // Issue #932: the banner used to be Teams-only
  // (`settings.teamsPersistWarning`, added with #562). The new
  // Spotify mirror ships the same copy with a `{provider}` placeholder
  // — the Settings card passes the display name (`Microsoft Teams` or
  // `Spotify`) so one banner covers both providers.
  'settings.authPersistWarning':
    'Accesso effettuato, ma questo dispositivo non ha potuto salvare la sessione di {provider} — funziona finché non esci. Riconnetti {provider} per provare a salvarla di nuovo.',

  // 4.7.0 — S4 (rules engine)
  'rules.quietWindowHint':
    'Le ore di silenzio si estendono oltre la mezzanotte — 22:00–07:00 copre tutta la notte. Ogni metà appartiene alla notte in cui inizia: con solo lunedì selezionato, la finestra copre la notte di lunedì fino a martedì mattina. Un orario di fine 00:00 indica la mezzanotte (la fine del giorno) e un inizio uguale alla fine non corrisponde mai.',
  'rules.pausePollingLabel': 'Interrompi il polling durante questa finestra',
  'rules.pausePollingHint':
    'Mentre questa finestra è attiva, Spotify non viene interrogato affatto — nessun aggiornamento di stato e nessuna chiamata a Teams. Il polling riprende da solo al termine della finestra.',
  'rules.trackRulesOrderHint':
    'Le regole vengono valutate dall\u2019alto verso il basso — vince la prima corrispondente. Una regola senza giorni feriali si applica ogni giorno, un orario di fine 00:00 indica la fine del giorno e un inizio uguale alla fine non corrisponde mai. Una finestra che attraversa la mezzanotte appartiene alla notte in cui inizia: con solo lunedì selezionato, 22:00–07:00 copre la notte di lunedì fino a martedì mattina.',
  'rules.ruleStart': 'Inizio della finestra della regola',
  'rules.ruleEnd': 'Fine della finestra della regola',
  'rules.ruleDays': 'Giorni attivi per questa regola (nessuna selezione = ogni giorno)',
  'rules.moveRuleUp': 'Sposta la regola {n} verso l\u2019alto',
  'rules.moveRuleDown': 'Sposta la regola {n} verso il basso',
  'rules.manualStatusLabel': 'Metti in pausa e interrompi il testo di stato',
  'rules.manualStatusHint':
    'Il testo pubblicato come stato di Teams mentre la riproduzione è in pausa e quando nulla è in riproduzione. L\u2019emoji musicale viene aggiunta automaticamente; cancellando un campo si ripristina il valore predefinito.',
  'rules.pausedStatusPlaceholder': 'In pausa',
  'rules.stoppedStatusPlaceholder': 'Nulla in riproduzione su Spotify',

  // 4.7.0 — S5 (log rotation + settings export/import)
  'settings.sectionLogging': 'Registrazione',
  'settings.loggingEnabledLabel': 'Scrivi un file di registro',
  'settings.logLevelLabel': 'Livello di registro',
  'settings.logMaxSizeLabel': 'Dimensione massima del file di registro (MB)',
  'settings.logKeepFilesLabel': 'File di registro archiviati da conservare',
  'settings.logRotationHint':
    'Il limite di dimensione e il numero di file archiviati si applicano al successivo avvio di PresenceJam. Il registro in scrittura in questo momento non è tra questi: la cartella dei registri contiene al massimo un file in più rispetto al numero impostato. Disattivare la registrazione o modificare il livello ha effetto immediato.',
  'settings.sectionBackup': 'Backup',
  'settings.backupHint':
    'L\u2019esportazione scrive una copia di queste impostazioni che puoi conservare o spostare su un altro computer. Il segreto client di Spotify resta nel portachiavi di sistema e non è mai incluso — e un file che ne contiene uno viene rifiutato all\u2019importazione.',
  'settings.backupExport': 'Esporta le impostazioni…',
  'settings.backupImport': 'Importa le impostazioni…',
  'settings.backupExportDialogTitle': 'Esporta le impostazioni di PresenceJam',
  'settings.backupImportDialogTitle': 'Importa le impostazioni di PresenceJam',
  'settings.backupConfirmOverwrite':
    'L\u2019importazione sostituisce tutte le impostazioni attuali. Il file attuale viene conservato accanto come config.json.bak. Continuare?',
  'settings.backupExported': 'Impostazioni esportate in {path}',
  'settings.backupImported': 'Impostazioni importate da {path}',
  'settings.backupError': 'Impossibile completare l\u2019azione di backup: {error}',

  // 4.7.0 — S9 (issue #677: the tray snooze / "pause sync for a while")
  'dashboard.snoozeChip': 'Posticipata — restano {remaining} (fino alle {time})',
  'dashboard.snoozeResume': 'Riprendi ora',
  'dashboard.snoozeResuming': 'Ripresa…',
  'dashboard.snoozeResumeFailed':
    'Impossibile riprendere la sincronizzazione. La posticipazione è ancora memorizzata — riprova.',
  // Issue #736: the chip's live region announces entry/exit once; the
  // per-second countdown is no longer in the live region.
  'dashboard.snoozeStatusEnd': 'Sincronizzazione ripresa',

  // 4.7.0 — S7 notifications (#675): one toggle per desktop-notification
  // class, plus the copy for the three classes the always-mounted layout
  // dispatches (track changes keep dispatching from the Dashboard card).
  'settings.notificationsTrackChange': 'Avvisami quando cambia il brano',
  'settings.notificationsSyncStopped': 'Avvisami quando la sincronizzazione si interrompe da sola',
  'settings.notificationsAuthRequired': 'Avvisami quando devo accedere di nuovo a Teams',
  'settings.notificationsUpdateStaged': 'Avvisami quando un aggiornamento verrà installato alla chiusura',
  'notifications.syncStoppedTitle': 'PresenceJam ha interrotto la sincronizzazione',
  'notifications.syncStoppedBody':
    'La sincronizzazione dello stato si è interrotta da sola. Apri PresenceJam per riavviarla.',
  'notifications.authRequiredTitle': 'Accesso a Teams richiesto',
  'notifications.authRequiredBody':
    'La sessione di Teams è scaduta. Accedi di nuovo così lo stato continua a sincronizzarsi.',
  'notifications.updateStagedTitle': 'Aggiornamento pronto',
  'notifications.updateStagedBody': 'PresenceJam {version} verrà installato alla chiusura.',
  // 4.7.0 — update channel (#678)
  'settings.sectionUpdates': 'Aggiornamenti',
  'settings.updateChannelLabel': 'Canale di rilascio',
  'settings.updateChannelStable': 'Stabile',
  'settings.updateChannelBeta': 'Beta',
  'settings.updateChannelHint':
    'Le build Beta utilizzano il feed beta progressivo quando disponibile; se manca o non ha una versione più recente, Beta ripiega sul rilascio stabile. Le build Beta si installano solo alla chiusura.',
  'update.betaOnQuitOnly':
    'Canale Beta: gli aggiornamenti si installano alla chiusura — su Beta non esiste un percorso di download e riavvio.',
  // 4.7.0 — S8 global hotkeys
  'settings.sectionShortcuts': 'Scorciatoie globali',
  'settings.shortcutsHint':
    'Funzionano mentre la finestra è nascosta. Fai clic su un campo e premi la combinazione desiderata — il campo registra ciò che premi, non ciò che digiti.',
  'settings.shortcutTogglePlayback': 'Attiva/disattiva la riproduzione',
  'settings.shortcutToggleSync': 'Metti in pausa o riprendi la sincronizzazione',
  'settings.shortcutUnbound': 'Non impostata — fai clic e premi una combinazione',
  'settings.shortcutClear': 'Cancella',
  'settings.shortcutRegistered': 'Attiva',
  'settings.shortcutNotRegistered': 'Non registrata su questo desktop',
  'settings.shortcutCaptureReleased':
    'Rilasciata durante la registrazione — l\u2019associazione attuale verrebbe attivata invece di essere registrata',
  'settings.shortcutRejected': 'Non utilizzabile: {reason}',
  'settings.shortcutRegistrationFailed': 'Registrazione non riuscita su questo desktop: {reason}',
  // Issue #968: typed reason codes from the Rust validator. The Settings
  // card maps each `kind` to a dictionary entry so the rejection copy is
  // localized; `shortcutReasonUnknown` renders the backend's free-form text
  // for genuinely foreign refusals (compositor / app-owned combos).
  'settings.shortcutReasonNotAKey': '«{accelerator}» non è una scorciatoia riconosciuta',
  'settings.shortcutReasonConflict':
    'In conflitto con la scorciatoia {other} — un acceleratore non può controllare entrambe le azioni',
  // Issue #810: a bare key would be grabbed system-wide. Function keys
  // (F1–F24) and media keys are exempt and bind without a modifier.
  'settings.shortcutReasonNeedsModifier':
    '«{accelerator}» richiede almeno un modificatore — un tasto singolo verrebbe intercettato in ogni applicazione',
  'settings.shortcutReasonAutostart':
    'Avvio automatico all\u2019accesso non riuscito: {cause}',
  'settings.shortcutReasonUnknown': '{message}',
  'settings.shortcutReasonX11Unavailable':
    'Le scorciatoie globali richiedono un display X11 raggiungibile su questo desktop',
  'settings.shortcutReasonWorkerUnavailable':
    'Il worker delle scorciatoie globali non può essere verificato; le scorciatoie non sono disponibili',
  // 4.7.0 — S12 hygiene (theme/density)
  'settings.themeSystem': 'Sistema',
  'settings.themeHint':
    '«Sistema» segue l\u2019aspetto del sistema operativo; Scuro e Chiaro restano fissi.',
  'settings.densityCompactLabel': 'Spaziatura compatta',
  'settings.densityHint': 'Riduce la spaziatura e la scala dei caratteri. Indipendente dal tema.',
  // --- 5.0 wave1 i18n-lib ---
  // Key requests routed through this slice's dictionaries (the owning slice
  // cannot edit them).
  'onboarding.pollIntervalClamped':
    'L\u2019intervallo salvato è {stored}s, fuori dall\u2019intervallo {min}–{max}s di questo passaggio — verrà utilizzato {seconds}s.',
  'update.stagingProgressLabel': 'Preparazione dell\u2019aggiornamento',
  'rules.presenceAvailable': 'Disponibile',
  'rules.presenceBusyCall': 'Occupato — In chiamata',
  'rules.presenceBusyConference': 'Occupato — In teleconferenza',
  'rules.presenceAway': 'Assente',
  'rules.presenceDndPresenting': 'Non disturbare — In presentazione',
  // --- 5.0 wave3 features-presence ---
  'dashboard.manualStatusTitle': 'Stato manuale',
  'dashboard.manualStatusPlaceholder': 'Imposta uno stato visibile al team per un po\u2019',
  'dashboard.manualStatusExpiryLabel': 'Scade dopo',
  'dashboard.manualStatusExpiry15': '15 minuti',
  'dashboard.manualStatusExpiry30': '30 minuti',
  'dashboard.manualStatusExpiry60': '1 ora',
  'dashboard.manualStatusExpiry120': '2 ore',
  'dashboard.manualStatusSet': 'Imposta lo stato',
  'dashboard.manualStatusClear': 'Cancella lo stato',
  'dashboard.manualStatusActive': 'Attivo fino alle {expiry}',
  'dashboard.manualStatusActiveEmpty': 'Attivo (scade a breve)',
  'dashboard.manualStatusRecentTitle': 'Stati recenti',
  'dashboard.manualStatusRecentEmpty': 'Nessuno stato recente',
  'dashboard.manualStatusFiltered': 'Lo stato è stato riscritto dal filtro volgarità',
  'dashboard.activityTitle': 'Attività',
  'dashboard.activityEmpty': 'Nessuna decisione finora — avvia la sincronizzazione per vedere le scelte dell\u2019app',
  'dashboard.volumeLabel': 'Volume',
  'dashboard.volumeAria': 'Cursore del volume di Spotify',
  'dashboard.seekAria': 'Fai clic per spostarti',
  'dashboard.seekUnavailableAria': 'Questo dispositivo non supporta lo spostamento',

  // --- 5.0 wave3 features-outlook ---
  'rules.importWorkingHours': 'Importa l\u2019orario di lavoro di Outlook',
  'rules.importWorkingHoursHint': 'Legge la scheda Orario di lavoro e trasforma ogni blocco non lavorativo in una regola di ore di silenzio. Controlla l\u2019anteprima prima di applicare.',
  'rules.importWorkingHoursPreviewTitle': 'Anteprima dell\u2019orario di lavoro di Outlook',
  'rules.importWorkingHoursApply': 'Applica queste regole',
  'rules.importWorkingHoursReplace': 'Sostituisci le regole di ore di silenzio esistenti',
  'rules.importWorkingHoursCancel': 'Annulla',
  'rules.importWorkingHoursReplaceHint': 'Sostituisce le regole di ore di silenzio esistenti con il set importato. Deseleziona per conservare entrambe.',
  'rules.importWorkingHoursDaysLabel': 'Outlook riporta {days} lavorativi dalle {start} alle {end}',
  'rules.importWorkingHoursDaysAllOff': 'Outlook non riporta giorni lavorativi — nulla da importare',

  // --- 5.0 wave3 features-gating ---

  // --- 5.0 wave3 features-profiles ---
  'rules.testTitle': 'Prova queste regole',
  'rules.testHint':
    'Digita un brano di esempio, scegli un minuto del giorno + giorno della settimana e vedi esattamente quale regola (se presente) scatterebbe.',
  'rules.testArtistLabel': 'Artista',
  'rules.testTrackLabel': 'Titolo del brano',
  'rules.testAlbumLabel': 'Album (facoltativo)',
  'rules.testShowLabel': 'Programma / podcast (facoltativo)',
  'rules.testDeviceLabel': 'Dispositivo Spotify (facoltativo)',
  'rules.testPlaylistLabel': 'URI della playlist (facoltativo)',
  'rules.testDurationLabel': 'Durata (mm:ss, facoltativo)',
  'rules.testWeekdayLabel': 'Giorno della settimana',
  'rules.testMinuteLabel': 'Minuto del giorno (HH:MM)',
  'rules.testRun': 'Esegui la prova',
  'rules.testRunning': 'Esecuzione…',
  'rules.testSummaryNoMatch': 'Nessuna regola corrisponderebbe a questo brano.',
  'rules.testSummaryRuleMatched': 'La regola {index} scatterebbe: {summary}',
  'rules.testStepMatched': 'corrisponde',
  'rules.testStepNotMatched': 'NON corrisponde',
  'rules.testStepReason': 'motivo: {reason}',
  'rules.testStepDisabled': 'la regola è disabilitata',
  'rules.testStepScheduleOutside': 'la pianificazione non contiene questo minuto del giorno',
  'rules.testStepNegated': 'la negazione ha invertito le condizioni',
  'rules.matchKindLabel': 'Stile di corrispondenza',
  'rules.matchKindSubstring': 'Sottostringa (predefinito)',
  'rules.matchKindExact': 'Corrispondenza esatta',
  'rules.matchKindGlob': 'Modello glob',
  'rules.albumSubstringLabel': 'L\u2019album contiene…',
  'rules.showSubstringLabel': 'Il programma contiene…',
  'rules.deviceSubstringLabel': 'Il dispositivo contiene…',
  'rules.playlistUriLabel': 'L\u2019URI della playlist contiene…',
  'rules.minDurationLabel': 'Durata minima (secondi)',
  'rules.negateLabel': 'Nega (corrisponde quando le condizioni NON valgono)',
  'rules.actionLabel': 'Azione',
  'rules.actionSuppress': 'Sopprimi lo stato',
  'rules.actionReplace': 'Sostituisci lo stato con…',
  'rules.actionSnoozeMinutes': 'Posticipa di minuti…',
  'rules.actionProfile': 'Passa al profilo…',
  'rules.actionPresence': 'Imposta la coppia di presenza…',
  'rules.actionReplaceStatusPlaceholder': 'Testo di stato',
  'rules.actionSnoozePlaceholder': 'Minuti',
  'rules.actionProfilePlaceholder': 'Nome del profilo',
  'rules.actionAvailabilityPlaceholder': 'Disponibilità',
  'rules.actionActivityPlaceholder': 'Attività',
  'dashboard.gateWhyTitle': 'Perché lo stato è in pausa?',
  'dashboard.gateWhyShow': 'Mostra il motivo',
  'dashboard.gateWhyHide': 'Nascondi il motivo',
  'dashboard.gateWhyEmpty': 'Nessun blocco attivo — lo stato si sta sincronizzando normalmente.',

  'profiles.sectionTitle': 'Profili di presenza',
  'profiles.sectionHint':
    'Salva un overlay denominato (formato di stato, blocchi, regole) e passa da uno all\u2019altro con la barra delle applicazioni, un tasto di scelta rapida o la CLI.',
  'profiles.empty': 'Nessun profilo definito.',
  'profiles.addProfile': 'Aggiungi profilo',
  'profiles.removeProfile': 'Rimuovi',
  'profiles.activeProfileLabel': 'Profilo attivo',
  'profiles.activeProfileNone': 'Nessuno — usa la configurazione di base',
  'profiles.overlayStatusFormatLabel': 'Formato di stato',
  'profiles.overlayClearOnPauseLabel': 'Cancella lo stato in pausa',
  'profiles.overlayAvailabilitySyncLabel': 'Sincronizza la disponibilità con Teams',
  'profiles.overlayGateOutOfOfficeLabel': 'Blocco quando fuori sede',
  'profiles.overlayGatePresentingLabel': 'Blocco con app a schermo intero',
  'profiles.overlayIdleAwayLabel': 'Interrompi la pubblicazione dopo inattività (secondi)',
  'profiles.overlayPreferredPresenceLabel': 'Presenza preferita',
  'profiles.overlayRulesLabel': 'Sottoinsieme di regole',
  'profiles.overlayRulesHint': 'Sostituisce l\u2019elenco di regole di base mentre questo profilo è attivo.',
  'profiles.overlayNotificationsLabel': 'Notifiche',
  'profiles.profileNameLabel': 'Nome del profilo',
  'profiles.profileNamePlaceholder': 'es. Concentrazione, Allenamento',
  'profiles.profileNameDuplicate': 'Esiste già un profilo con questo nome.',
  'profiles.profileNameTooLong': 'I nomi dei profili devono avere al massimo 32 caratteri.',
  'profiles.profileNameMissing': 'Il nome del profilo non può essere vuoto.',
  'profiles.activeProfileUnknown': 'Profilo sconosciuto — viene utilizzata la configurazione di base.',
  'tray.profilesMenu': 'Profili di presenza',
  'tray.profilesMenuNone': 'Nessun profilo definito',
  'tray.profilesMenuActivateBase': 'Usa la configurazione di base',
  'cli.profileFlag': 'Passa a un profilo di presenza denominato ed esci.',
  'cli.profileActive': 'Il profilo attivo ora è «{name}».',
  'cli.profileUnknown': 'Nessun profilo denominato «{name}» — resta attiva la configurazione di base.',
  'settings.shortcutToggleProfile': 'Scorri i profili di presenza',

  // --- 5.0 wave3 playback-source ---
  'onboarding.playbackSourceMacNote':
    'Sei su macOS — qui è disponibile solo la sorgente di riproduzione Spotify. La sorgente di sessione multimediale di sistema (Windows SMTC / Linux MPRIS) non è disponibile su macOS, quindi PresenceJam ripiega automaticamente su Spotify. Passa a Spotify se la procedura guidata segnala «nessun brano» mentre la musica è in riproduzione in un\u2019altra app.',

  // --- #1120: the snooze entry announcement is count-aware. A one-minute
  // snooze used to announce "Sync paused for 1 minutes"; German needs
  // "Minute" and French "minute" in the singular.
  'dashboard.snoozeStatusStart_one': 'Sincronizzazione in pausa per {minutes} minuto',
  'dashboard.snoozeStatusStart_other': 'Sincronizzazione in pausa per {minutes} minuti',

  // --- #739: the wizard's view name, announced on navigation (there is no
  // heading bar above the step, so the step title cannot stand in for it).
  'onboarding.title': 'Configurazione',

  // --- #954 / P7: the compact header's abbreviated sync badge. The full
  // "Synchronisierung" / "Synchronisation" label ellipsises into an unreadable
  // fragment at the 400px minimum, and a second badge row is worse. "Sync" is
  // the established short form in all three locales.
  'dashboard.syncingShort': 'Sync',
  // --- #966 / #981 Settings draft actions ---
  'settings.revertChanges': 'Annulla le modifiche',
  'rules.undoRemove': 'Annulla la rimozione',
};
