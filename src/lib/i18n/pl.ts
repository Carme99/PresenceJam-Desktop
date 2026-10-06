/**
 * C6 i18n foundation (docs/scope-3.3.md §C6) — Polish dictionary.
 * Typed against `Dict`, so every English key must be present.
 *
 * Model-written Polish translation (issue #984) — human review pending before this copy is considered final.
 */

import type { Dict } from './en';

export const pl: Dict = {
  // ── common ────────────────────────────────────────────────────────
  'common.back': 'Wstecz',
  'common.backToDashboard': 'Wróć do panelu',
  'common.checkNow': 'Sprawdź stan logowania',
  'common.connected': 'Połączono',
  'common.dismiss': 'Odrzuć',
  'common.openSignInPage': 'Otwórz stronę logowania Microsoft:',
  'common.enterCodeWhenAsked': 'Po wyświetleniu monitu wprowadź ten kod:',
  'common.moreActions': 'Więcej akcji',
  'common.launchAtLogin': 'Uruchamiaj przy logowaniu',
  'common.loading': 'Ładowanie…',
  'common.notConnected': 'Nie połączono',
  'common.reconnecting': 'Ponowne łączenie…',
  'common.reconnect': 'Połącz ponownie',
  'common.retry': 'Ponów',
  'common.bootFailed': 'Nie można wczytać stanu aplikacji',
  'common.resetToDefault': 'Przywróć domyślne',
  'common.themeToggle': 'Przełącz motyw',
  'common.waiting': 'Oczekiwanie…',
  'common.waitingForSignIn': 'Oczekiwanie na zalogowanie…',
  'common.codeExpiresIn': 'Kod wygasa za {time}',
  'common.codeExpired': 'Ten kod wygasł — nie można go już użyć.',
  'common.getNewCode': 'Pobierz nowy kod',
  'common.yes': 'Tak',
  'common.no': 'Nie',
  'common.tagline': 'Spotify → status Teams',

  // ── dashboard ─────────────────────────────────────────────────────
  'dashboard.spotifyOff': 'Spotify wyłączony',
  'dashboard.teamsOff': 'Teams wyłączony',
  'dashboard.syncing': 'Synchronizacja',
  'dashboard.logsDetachedTitle': 'Dzienniki (odłączone — kliknij, aby ustawić fokus)',
  'dashboard.logsTitle': 'Dzienniki',
  'dashboard.logsDetachedAria': 'Dzienniki (odłączone w osobnym oknie)',
  'dashboard.openLogsAria': 'Otwórz dzienniki',
  'dashboard.diagnostics': 'Diagnostyka',
  'dashboard.openDiagnosticsAria': 'Otwórz diagnostykę',
  'dashboard.settingsDetachedTitle': 'Ustawienia (odłączone — kliknij, aby ustawić fokus)',
  'dashboard.settings': 'Ustawienia',
  'dashboard.settingsDetachedAria': 'Ustawienia (odłączone w osobnym oknie)',
  'dashboard.openSettingsAria': 'Otwórz ustawienia',
  'dashboard.about': 'Informacje',
  'dashboard.aboutAria': 'Informacje o PresenceJam',
  'dashboard.pauseSync': 'Wstrzymaj synchronizację',
  'dashboard.resumeSync': 'Wznów synchronizację',
  'dashboard.presenceGated': 'Status wstrzymany — tryb zajętości, połączenie lub prezentacja',
  'dashboard.setupRequired': 'Wymagana konfiguracja',
  'dashboard.setupHint':
    'Połącz Spotify i Microsoft Teams, aby odtwarzane utwory sterowały statusem w Teams.',
  'dashboard.continueSetup': 'Kontynuuj konfigurację',
  'dashboard.playing': 'Odtwarzanie',
  'dashboard.paused': 'Wstrzymano',
  'dashboard.liveStreamAria': 'Transmisja na żywo — pozycja nieznana',
  'dashboard.yourTeamsStatus': 'Status w Teams',
  'dashboard.nothingPlaying': 'Brak odtwarzania',
  'dashboard.nothingPlayingHint':
    'Odtwórz coś w Spotify, a pojawi się to w statusie Teams.',
  'dashboard.syncCrashed': 'Synchronizacja zatrzymała się nieoczekiwanie. Naciśnij wznów (▶), aby ją uruchomić ponownie.',
  'dashboard.credentialCheckFailed': 'Nie można sprawdzić poświadczeń. Sprawdź połączenie i spróbuj ponownie.',
  'dashboard.syncToggleFailed': 'Nie można uruchomić/zatrzymać synchronizacji. Spróbuj ponownie — jeśli problem nie ustąpi, otwórz Diagnostykę z nagłówka panelu.',
  'dashboard.statusNotConfigured': 'Nieskonfigurowany',
  'dashboard.statusNoTrack': 'Brak odtwarzanego utworu',
  'dashboard.live': 'Na żywo',
  'dashboard.refreshStatus': 'Odśwież status',
  'dashboard.refreshing': 'Odświeżanie…',
  'dashboard.refreshFailed': 'Nie można odświeżyć statusu. Spróbuj ponownie.',
  'dashboard.refreshAria': 'Odśwież status Teams teraz',

  // ── logs ──────────────────────────────────────────────────────────
  'logs.title': 'Dzienniki',
  'logs.filterAria': 'Filtr poziomu dziennika',
  'logs.level.all': 'Wszystkie',
  'logs.level.trace': 'Śledzenie',
  'logs.level.debug': 'Debugowanie',
  'logs.level.info': 'Informacje',
  'logs.level.warning': 'Ostrzeżenie',
  'logs.level.error': 'Błąd',
  'logs.count_one': '{count} wpis',
  'logs.count_other': '{count} wpisów',
  // #1154: CLDR `few` (2–4, 22–24…): nominative plural "wpisy".
  // Model-written per #984 provenance — human review pending.
  'logs.count_few': '{count} wpisy',
  'logs.showingOf': 'Widoczne {shown} z {total}',
  'logs.jumpToLatest': 'Przejdź do najnowszych',
  'logs.popOut': 'Odłącz',
  'logs.clear': 'Wyczyść',
  'logs.openFolder': 'Otwórz folder',
  'logs.empty': 'Brak wpisów dziennika',
  'logs.emptyHint': 'Wpisy na żywo pojawią się tutaj po uruchomieniu synchronizacji i odtwarzaniu w Spotify.',

  // ── settings ──────────────────────────────────────────────────────
  'settings.title': 'Ustawienia',
  'settings.popBackIn': 'Zadokuj z powrotem',
  'settings.popOutActionTitle': 'Odłącz do osobnego okna',
  'settings.unsavedChanges': 'Niezapisane zmiany',
  'settings.sectionSpotify': 'Spotify',
  'settings.sectionTeams': 'Microsoft Teams',
  'settings.sectionPresence': 'Obecność',
  'settings.sectionStatusFormat': 'Format statusu',
  'settings.sectionPolling': 'Częstotliwość synchronizacji',
  'settings.sectionNotifications': 'Powiadomienia',
  'settings.sectionAppearance': 'Wygląd',
  'settings.clientId': 'Identyfikator klienta',
  'settings.clientIdPlaceholder': 'Wprowadź identyfikator klienta Spotify',
  'settings.clientSecret': 'Klucz tajny klienta',
  'settings.secretStoredHint':
    'Przechowywany bezpiecznie w pęku kluczy systemu operacyjnego. Aby go wymienić, wróć do panelu i wybierz Kontynuuj konfigurację.',
  'settings.secretNotConfigured': 'Nieskonfigurowany.',
  'settings.runOnboarding': 'Uruchom kreator konfiguracji',
  'settings.toSetUpSpotify': ', aby skonfigurować Spotify.',
  'settings.reconnectSpotify': 'Połącz Spotify ponownie',
  'settings.completeAuthInBrowser': 'Dokończ uwierzytelnianie w przeglądarce.',
  'settings.playbackScopeBanner': 'Spotify dodało elementy sterujące odtwarzaniem. Kliknij Połącz ponownie obok tego komunikatu, aby je włączyć.',
  'settings.spotifySecretConflict':
    'Klucz tajny klienta w pliku konfiguracji różni się od przechowywanego w pęku kluczy. Połącz Spotify ponownie, aby rozwiązać problem.',
  'settings.teamsAuthHint':
    'Uwierzytelnianie Teams korzysta z konta Microsoft 365. Dodatkowa konfiguracja nie jest wymagana.',
  'settings.presenceScopeBanner': 'Teams dodało wykrywanie spotkań/połączeń. Kliknij Połącz ponownie obok tego komunikatu, aby je włączyć.',
  'settings.availabilitySyncLabel': 'Pokazuj status Dostępny podczas słuchania',
  'settings.availabilitySyncHint':
    'Domyślnie wyłączone. Po włączeniu Teams pokazuje status Dostępny (zamiast Zajęty) podczas odtwarzania muzyki. Uwaga: podczas połączeń i spotkań Teams nadal pokazuje Zajęty.',
  'settings.presenceGateLabel': 'Wstrzymuj status podczas spotkań/połączeń/Nie przeszkadzać',
  'settings.presenceGateHint':
    'Domyślnie włączone. Pomija zapisywanie statusu Spotify, gdy Teams zgłasza zajętość, spotkanie, połączenie lub prezentację.',
  'settings.formatTemplate': 'Szablon formatu',
  'settings.formatTemplatePlaceholder': '🎵 {artist} - {track} 🎧',
  'settings.livePreview': 'Podgląd na żywo',
  'settings.placeholdersHint':
    'Dostępne symbole zastępcze: {artist}, {track}, {album}, {emoji}, {device}, {playlist} (lub {context}), {progress}, {shuffle}, {repeat}. Opcje Losowo i Powtarzanie pokazują 🔀/🔁 tylko gdy są włączone.',
  'settings.profanityFilterLabel': 'Filtruj wulgaryzmy w statusie',
  'settings.placeholderTextLabel': 'Tekst zastępczy',
  'settings.placeholderTextHint':
    'Użyj {emoji} dla stanu odtwarzania (🎵 odtwarzanie / ⏸ wstrzymano). Wyświetlane, gdy w informacjach o utworze wykryto wulgaryzmy.',
  'settings.placeholderTextPlaceholder': 'Aktualnie słucham Spotify',
  'settings.profaneSampleToggle': 'Podgląd z wulgarnym przykładowym utworem',
  'settings.defaultIntervalLabel': 'Domyślny interwał: {seconds} s',
  'settings.minIntervalLabel': 'Minimalny interwał (s)',
  'settings.maxIntervalLabel': 'Maksymalny interwał (s)',
  'settings.clampHint':
    'Minimalny interwał przekracza maksymalny — zostanie zapisane maksimum {max} s.',
  'settings.notificationsHint':
    'Każda włączona klasa pokazuje powiadomienie systemowe — pierwsze może poprosić system operacyjny o uprawnienia.',
  'settings.themeLabel': 'Motyw',
  'settings.themeDark': 'Ciemny',
  'settings.themeLight': 'Jasny',
  'settings.languageLabel': 'Język',
  'settings.saveChanges': 'Zapisz zmiany',
  'settings.saving': 'Zapisywanie…',
  'settings.saved': 'Zapisano ustawienia.',
  'settings.failedToSave': 'Nie można zapisać ustawień — zmiany pozostały na miejscu. Spróbuj ponownie.',
  'settings.openLogsFolder': 'Otwórz folder dzienników',
  'settings.previewUnavailable': '(podgląd niedostępny)',

  // ── diagnostics ───────────────────────────────────────────────────
  'diagnostics.title': 'Diagnostyka',
  'diagnostics.localOnlyHint': 'Migawka tylko lokalna — można dołączyć do zgłoszenia błędu.',
  'diagnostics.copy': 'Kopiuj diagnostykę',
  'diagnostics.saveToFile': 'Zapisz do pliku',
  'diagnostics.copied': 'Skopiowano diagnostykę do schowka.',
  'diagnostics.copyFailed': 'Kopiowanie nie powiodło się — użyj opcji „Zapisz do pliku”.',
  'diagnostics.savedToDownloads': 'Zapisano diagnostykę w folderze pobranych plików.',
  'diagnostics.saveFailed': 'Zapis nie powiódł się — użyj opcji „Kopiuj diagnostykę”.',
  'diagnostics.collecting': 'Zbieranie diagnostyki…',
  'diagnostics.collectFailed': 'Nie można zebrać diagnostyki',
  'diagnostics.versions': 'Wersje',
  'diagnostics.configuration': 'Konfiguracja',
  'diagnostics.connections': 'Połączenia',
  'diagnostics.recentLogLines': 'Ostatnie wiersze dziennika',
  'diagnostics.app': 'PresenceJam',
  'diagnostics.tauri': 'Tauri',
  'diagnostics.os': 'System operacyjny',
  'diagnostics.osRelease': 'Wersja',
  'diagnostics.installFlavor': 'Wariant instalacji',
  'diagnostics.unknown': 'nieznany',
  'diagnostics.spotifyClientId': 'Identyfikator klienta Spotify',
  'diagnostics.redirectUri': 'URI przekierowania',
  'diagnostics.notSet': '(nie ustawiono)',
  'diagnostics.clientSecretKeychain': 'Klucz tajny klienta w pęku kluczy',
  'diagnostics.clearOnPause': 'Czyszczenie statusu przy wstrzymaniu',
  'diagnostics.profanityFilter': 'Filtr wulgaryzmów',
  'diagnostics.extraProfanityWords': 'Dodatkowe filtrowane słowa',
  'diagnostics.startMinimized': 'Uruchamianie zminimalizowane',
  'diagnostics.availabilitySync': 'Synchronizacja dostępności Teams',
  'diagnostics.presenceGate': 'Bramka obecności',
  'diagnostics.pollInterval': 'Interwał odpytywania (domyślny/min./maks.)',
  'diagnostics.logging': 'Rejestrowanie',
  'diagnostics.loggingEnabled': 'włączone ({level})',
  'diagnostics.loggingDisabled': 'wyłączone',
  'diagnostics.respectManualStatus': 'Poszanowanie ręcznego statusu',
  'diagnostics.gateOutOfOffice': 'Bramka podczas nieobecności',
  'diagnostics.locale': 'Ustawienia regionalne',
  'diagnostics.updateChannel': 'Kanał aktualizacji',
  'diagnostics.configSnoozed': 'Synchronizacja uśpiona',
  'diagnostics.launchAtLogin': 'Uruchamianie przy logowaniu',
  'diagnostics.statusRules': 'Reguły statusu',
  'diagnostics.statusRulesValue': '{quiet}/{quietTotal} cichych godzin, włączonych reguł utworów: {rules}/{rulesTotal}',
  'diagnostics.spotifyConnected': 'Połączono Spotify',
  'diagnostics.spotifyTokenExpires': 'Token Spotify wygasa',
  'diagnostics.teamsConnected': 'Połączono Teams',
  'diagnostics.teamsTokenExpires': 'Token Teams wygasa',
  'diagnostics.expired': '(wygasł)',
  'diagnostics.keychainSpotifySecret': 'Pęk kluczy: obecny klucz tajny Spotify',
  'diagnostics.keychainEncryptionKey': 'Pęk kluczy: obecny klucz szyfrowania tokenów',
  'diagnostics.tokensNeverIncluded':
    'Wartości tokenów nigdy nie są dołączane — tylko znaczniki wygaśnięcia i flagi obecności.',
  'diagnostics.noLogLinesYet': 'Brak dostępnych wierszy dziennika.',
  'diagnostics.failedInstallTitle': 'Nieudana instalacja aktualizacji',
  'diagnostics.failedInstallVersion': 'Wersja',
  'diagnostics.failedInstallError': 'Błąd',
  'diagnostics.failedInstallTimestamp': 'Podjęto próbę',
  'diagnostics.failedInstallDismissFailed': 'Nie można odrzucić rekordu nieudanej instalacji.',
  'diagnostics.resetTokenStorage': 'Zresetuj lokalny magazyn tokenów',
  'diagnostics.resetTokenConfirm':
    'Spowoduje to usunięcie pliku tokens.json i plików pomocniczych oraz zapisanego klucza szyfrowania tokenów. Konieczne będzie ponowne zalogowanie. Kontynuować?',
  'diagnostics.resetTokenDone': 'Zresetowano magazyn tokenów. Zaloguj się ponownie, aby wznowić synchronizację.',
  'diagnostics.resetTokenFailed': 'Resetowanie nie powiodło się — {error}',
  'diagnostics.syncRunning': 'Synchronizacja uruchomiona',
  'diagnostics.syncSnoozed': 'Uśpiona',
  'diagnostics.syncSnoozeMinutes': 'pozostało {minutes} min',
  'diagnostics.syncManualStatusBlocks': 'Ręczny status wstrzymuje zapisy',
  'diagnostics.syncPresenceGateReason': 'Przyczyna bramki obecności',
  'diagnostics.syncTransientFailures': 'Kolejne niepowodzenia uwierzytelniania',
  'diagnostics.syncNetworkFailures': 'Kolejne niepowodzenia sieciowe',

  // ── reconnect ─────────────────────────────────────────────────────
  'reconnect.title': 'Ponowne łączenie',
  'reconnect.description': 'Synchronizacja wymaga uwagi. Połącz ponownie poniżej, aby wznowić.',
  'reconnect.missingCredentials': 'Brak poświadczeń',
  'reconnect.failed': 'Niepowodzenie',
  'reconnect.readyToReconnect': 'Gotowe do ponownego połączenia',
  'reconnect.spotifyOk': 'Pomyślnie połączono Spotify ponownie.',
  'reconnect.spotifyNotConfigured':
    'Poświadczenia Spotify nie są skonfigurowane na tym komputerze.',
  'reconnect.completeAuthInOpenedBrowser':
    'Dokończ uwierzytelnianie w otwartym oknie przeglądarki.',
  'reconnect.tryAgain': 'Spróbuj ponownie',
  'reconnect.clickBelowSpotify': 'Kliknij poniżej, aby ponownie połączyć konto Spotify.',
  'reconnect.teamsOk': 'Pomyślnie połączono Teams ponownie.',
  'reconnect.clickBelowTeams':
    'Kliknij poniżej, aby ponownie połączyć konto Microsoft Teams.',
  'reconnect.tokenStorageUnusable': 'Zapisanych danych logowania nie można odczytać — przechowywany klucz szyfrowania tokenów jest bezużyteczny.',
  'reconnect.resetTokenStorage': 'Zresetuj lokalny magazyn tokenów',
  'reconnect.resetTokenConfirm':
    'Spowoduje to usunięcie pliku tokens.json i plików pomocniczych oraz zapisanego klucza szyfrowania tokenów. Konieczne będzie ponowne zalogowanie. Kontynuować?',
  'reconnect.resetTokenDone': 'Zresetowano magazyn tokenów. Zaloguj się ponownie poniżej, aby wznowić.',
  'reconnect.resetTokenFailed': 'Resetowanie nie powiodło się — {error}',
  'reconnect.missingCredsTitle': 'Brak poświadczeń Spotify?',
  'reconnect.reenterCredsHint':
    'Konieczne będzie ponowne wprowadzenie identyfikatora klienta i klucza tajnego klienta.',
  'reconnect.goToFullSetup': 'Przejdź do pełnej konfiguracji',
  'reconnect.reconnectTeams': 'Połącz Teams ponownie',

  // ── about ─────────────────────────────────────────────────────────
  'about.version': 'Wersja {version}',
  'about.description':
    'Automatycznie pokazuje odtwarzaną muzykę ze Spotify w statusie Microsoft Teams.',
  'about.statusSync': 'Synchronizacja statusu',
  'about.live': 'Na żywo',
  'about.auth': 'Logowanie',
  'about.authMethod': 'Spotify + Microsoft',
  'about.storage': 'Magazyn',
  'about.osKeychain': 'Pęk kluczy systemu operacyjnego',
  'about.githubRepo': 'Repozytorium GitHub',
  'about.releases': 'Wydania',
  'about.reportIssue': 'Zgłoś problem',

  // ── update banner ─────────────────────────────────────────────────
  'update.available': 'Dostępna aktualizacja v{version}',
  'update.stagedQuit': 'Wersja v{version} zostanie zainstalowana po zamknięciu PresenceJam',
  'update.downloadFailed': 'Pobieranie nie powiodło się — {error}',
  'update.downloadAndInstall': 'Pobierz i zainstaluj',
  'update.downloading': 'Pobieranie…',
  'update.installOnQuit': 'Zainstaluj przy zamknięciu',
  'update.preparing': 'Przygotowywanie…',
  'update.dismissAria': 'Odrzuć pasek aktualizacji',
  'update.confirmQuitInstall':
    'Zainstalować wersję v{staged} przy zamknięciu? Bieżąca wersja: v{current}.',
  'update.confirmQuitInstallUnknown': 'Zainstalować wersję v{staged} przy zamknięciu?',
  'update.stagedVsCurrent':
    'Wersja v{staged} zostanie zainstalowana przy zamknięciu (bieżąca v{current})',
  'update.staleSkipped':
    'Wersja v{staged} została pominięta — bieżąca v{current} jest nowsza.',
  'update.staleSkippedUnknown':
    'Wersja v{staged} została pominięta — nie jest nowsza niż bieżąca wersja.',
  'update.installAnyway': 'Zainstaluj mimo to',
  // #894: a `.deb` / `.rpm` install is updated by its package manager, not by
  // the in-app updater — the release manifest only offers the AppImage payload,
  // which neither installer can apply. The banner shows the command instead of
  // an install button.
  'update.packageManagedDeb':
    'Dostępna jest wersja v{version}. Ta kopia została zainstalowana jako pakiet .deb, więc aktualizacje pochodzą z apt.',
  'update.packageManagedRpm':
    'Dostępna jest wersja v{version}. Ta kopia została zainstalowana jako pakiet .rpm, więc aktualizacje pochodzą z dnf.',
  'update.packageManagerInstallDeb': 'sudo apt install ./{file}',
  'update.packageManagerInstallRpm': 'sudo dnf install ./{file}',

  // ── onboarding ────────────────────────────────────────────────────
  'onboarding.stepOf': 'Krok {step} z 3',
  'onboarding.step1Title': 'Połącz Spotify',
  'onboarding.step1Intro':
    'Wklej poniżej identyfikator klienta i klucz tajny klienta Spotify, a następnie wybierz Połącz Spotify — otworzy się strona logowania Spotify.',
  'onboarding.getCredentials': 'Pobierz poświadczenia Spotify',
  'onboarding.instruction1':
    'Otwórz panel dewelopera Spotify i utwórz aplikację.',
  'onboarding.instruction2': 'W sekcji URI przekierowań dodaj presencejam://callback (dzięki temu Spotify będzie wiedzieć, gdzie przekierować z powrotem).',
  'onboarding.instruction3':
    'Skopiuj identyfikator klienta i klucz tajny klienta z ustawień aplikacji.',
  'onboarding.clientIdPlaceholder': '32-znakowy identyfikator klienta Spotify',
  'onboarding.clientSecretPlaceholder': 'Klucz tajny klienta Spotify',
  'onboarding.connectSpotify': 'Połącz Spotify',
  'onboarding.signInWaiting': 'Logowanie do Spotify oczekuje…',
  'onboarding.manualUrlHint':
    'Dokończ logowanie do Spotify w przeglądarce, a następnie wklej poniżej pełny adres z paska adresu.',
  'onboarding.manualUrlLabel': 'URL przekierowania Spotify',
  'onboarding.manualUrlPlaceholder': 'presencejam://callback?code=…',
  'onboarding.submitCode': 'Prześlij kod',
  'onboarding.connectedToSpotify': 'Połączono ze Spotify',
  'onboarding.continue': 'Kontynuuj →',
  'onboarding.step2Title': 'Połącz Microsoft Teams',
  'onboarding.step2Intro':
    'Używany jest przepływ kodu urządzenia Microsoft — jednorazowy kod wprowadzany na stronie Microsoft. Dodatkowa konfiguracja nie jest wymagana.',
  'onboarding.startMicrosoftSignIn': 'Połącz Microsoft Teams',
  'onboarding.connectedToTeams': 'Połączono z Microsoft Teams',
  'onboarding.step3Title': 'Ostatnie szlify',
  'onboarding.step3Intro':
    'Wybierz wygląd komunikatu o statusie oraz czy PresenceJam ma uruchamiać się po zalogowaniu.',
  'onboarding.statusTemplate': 'Szablon statusu',
  'onboarding.placeholdersHint':
    'Symbole zastępcze: {artist}, {track}, {album}, {emoji}, {device}, {playlist} (lub {context}), {progress}, {shuffle}, {repeat}',
  'onboarding.pollInterval': 'Częstotliwość sprawdzania Spotify: {seconds} s',
  'onboarding.settingUp': 'Konfigurowanie…',
  'onboarding.finishSetup': 'Zakończ konfigurację',

  // ── validation / errors ───────────────────────────────────────────
  'validation.clientIdRequired': 'Identyfikator klienta Spotify jest wymagany.',
  'validation.clientIdFormat':
    'Identyfikator klienta Spotify musi składać się z dokładnie 32 znaków szesnastkowych.',
  'validation.clientSecretRequired': 'Klucz tajny klienta Spotify jest wymagany.',
  'validation.clientSecretTooShort':
    'Ten klucz tajny klienta wygląda na zbyt krótki — powinien mieć co najmniej 32 znaki. Sprawdź, czy nie wystąpił błąd kopiowania.',
  'validation.noCodeInUrl':
    'Ten adres URL nie zawiera kodu logowania — wklej pełny adres z paska adresu przeglądarki po przekierowaniu przez Spotify.',
  'validation.connectBothFirst':
    'Przed zakończeniem konfiguracji połącz Spotify i Teams.',
  'validation.setupFailed': 'Konfiguracja nie powiodła się: {error}',

  // ── routes / chrome ───────────────────────────────────────────────
  'routes.skipToMainContent': 'Przejdź do treści głównej',
  'routes.unknownPane': 'Nieznany panel: {pane}',

  // ── feat/45-features: status rules (#432) + support snapshot (#434) ──
  'rules.sectionTitle': 'Reguły statusu',
  'rules.sectionHint':
    'Ciche godziny i reguły utworów wstrzymują zapis statusu w Teams, korzystając z tej samej ścieżki bramki obecności — usunięta reguła publikuje automatycznie w trakcie utworu.',
  'rules.quietHoursLabel': 'Ciche godziny',
  'rules.noQuietHours': 'Nie zdefiniowano cichych godzin — synchronizacja statusu działa o każdej porze.',
  'rules.quietStart': 'Początek cichych godzin',
  'rules.quietEnd': 'Koniec cichych godzin',
  'rules.quietDays': 'Aktywne dni (brak wyboru = codziennie)',
  'rules.dayEveryDay': 'Codziennie',
  'rules.day1': 'pon.',
  'rules.day2': 'wt.',
  'rules.day3': 'śr.',
  'rules.day4': 'czw.',
  'rules.day5': 'pt.',
  'rules.day6': 'sob.',
  'rules.day7': 'niedz.',
  'rules.addQuietHours': 'Dodaj ciche godziny',
  'rules.trackRulesLabel': 'Reguły utworów',
  'rules.noTrackRules': 'Nie zdefiniowano reguł utworów — wszystkie utwory synchronizują się normalnie.',
  'rules.artistPlaceholder': 'Wykonawca zawiera…',
  'rules.trackPlaceholder': 'Tytuł utworu zawiera…',
  'rules.replacementPlaceholder': 'Opublikuj zamiast tego (puste = wstrzymaj)',
  'rules.addTrackRule': 'Dodaj regułę utworu',
  'rules.removeRule': 'Usuń',
  'rules.ruleEnabled': 'Włączone',
  'logs.copySnapshot': 'Kopiuj migawkę',
  'logs.snapshotCopied': 'Zredagowana migawka skopiowana do schowka.',
  'logs.snapshotCopyFailed': 'Nie można skopiować migawki.',
  'logs.openFolderError':
    'Nie można otworzyć folderu dzienników. Może jeszcze nie istnieć — spróbuj ponownie uruchomić aplikację, aby go utworzyć.',
  // 4.6 additions
  'dashboard.availabilityListening': 'Słuchanie (status Dostępny)',
  'dashboard.availabilityCleared': 'Dostępność wyczyszczona',
  'settings.saveAndLeave': 'Zapisz i wyjdź',
  'settings.discardChanges': 'Odrzuć zmiany',
  'settings.stayHere': 'Zostań tutaj',
  'settings.notificationsDenied':
    'Powiadomienia są blokowane przez system. Zezwól na nie w ustawieniach systemu, a następnie włącz je ponownie.',
  'diagnostics.expiryBuffer': 'Bufor odświeżania tokenu',
  'diagnostics.teamsRefreshTokenPresent': 'Przechowywany token odświeżania Teams',
  'reconnect.restartSignIn': 'Uruchom logowanie ponownie',
  'update.stagingProgress': 'Przygotowywanie aktualizacji — {percent}%',
  'update.cancelStage': 'Anuluj',
  'onboarding.submitting': 'Przesyłanie…',
  'settings.episodeFormatHint':
    'Podcasty i audiobooki używają własnego szablonu — 🎙️ {show} - {episode} — więc powyższy szablon muzyczny ich nie dotyczy.',
  'reconnect.keychainUnavailableBadge':
    'Pęk kluczy niedostępny',
  'reconnect.keychainUnavailableHint':
    'PresenceJam nie może odczytać zapisanego klucza tajnego klienta Spotify: pęk kluczy systemu jest zablokowany lub go brakuje. Odblokuj go (lub zainstaluj dostawcę usługi Secret Service, np. gnome-keyring), a następnie spróbuj ponownie — klucz tajny jest nadal przechowywany, więc nie trzeba ponownie konfigurować Spotify.',
  'settings.secretKeychainUnavailable':
    'Pęk kluczy systemu niedostępny — może być zablokowany lub go brakuje. Odblokuj go (lub zainstaluj dostawcę usługi Secret Service), aby użyć zapisanego klucza tajnego; jest nadal przechowywany.',
  'diagnostics.quarantineTitle': 'Ustawienia zostały zresetowane',
  'diagnostics.quarantineBodyNow':
    'PresenceJam nie mógł odczytać pliku ustawień, więc wszystkie ustawienia zostały zresetowane do wartości domyślnych.',
  'diagnostics.quarantineBodyEarlier':
    'Przy poprzednim uruchomieniu nie można było odczytać pliku ustawień i zresetowano go do wartości domyślnych.',
  'diagnostics.quarantineBackupPresent':
    'Nieczytelny oryginał zachowano obok pliku ustawień jako {name}, więc przechowywane w nim wartości można nadal odzyskać.',
  'diagnostics.quarantineBackupMissing':
    'Nieczytelny oryginał nadal znajduje się obok pliku ustawień jako config.json.',
  'diagnostics.quarantineWhere':
    'Oba pliki znajdują się w folderze PresenceJam w folderze konfiguracji użytkownika — kopia zapasowa leży obok pliku config.json.',
  // 4.6 additions (presence rules #634/#635/#636/#637 + #538 consumption sites)
  'rules.replacementClampHint':
    'Zastępczy status jest ograniczony do {max} znaków; reszta nie zostanie opublikowana.',
  'rules.presenceLabel': 'Obecność, gdy obowiązuje ta reguła',
  'rules.presenceNone': 'Nie zmieniaj obecności',
  'rules.presenceHint':
    'Reguła może ustawiać dostępność/aktywność w Teams, ale tylko gdy włączona jest „Synchronizacja dostępności”; nigdy nie przesłania połączenia, spotkania ani statusu ustawionego ręcznie.',
  'settings.respectManualStatusLabel': 'Nigdy nie nadpisuj statusu ustawionego ręcznie',
  'settings.respectManualStatusHint':
    'Wykorzystuje odczyt obecności już wykonywany przez bramkę, więc nie kosztuje dodatkowego żądania — tekst jest respektowany do czasu jego zmiany lub wygaśnięcia.',
  'settings.gateOutOfOfficeLabel': 'Wstrzymuj podczas nieobecności',
  'settings.gateOutOfOfficeHint':
    'Pomija aktualizację statusu, gdy włączone jest ustawienie nieobecności w Teams. Reguła utworu z własną akcją obecności przesłania to ustawienie.',
  // Issue #872: the OS-level presentation gate (full-screen app, slide
  // deck, Windows Focus Assist Quiet Time). OFF by default, matching how
  // `availabilitySync` and `gateWhenOutOfOffice` shipped — 4.7 behaviour
  // is unchanged until the user opts in.
  'settings.gateWhenPresentingLabel': 'Wstrzymuj podczas prezentowania',
  'settings.gateWhenPresentingHint':
    'Pomija zapisywanie statusu Spotify, gdy system operacyjny zgłasza aplikację pełnoekranową, pokaz slajdów lub tryb cichy. Systemy Linux/macOS nigdy nie zgłaszają tego sygnału, więc przełącznik nie ma tam działania.',
  // Issue #873: the desktop-idle gate. `0` (the default) keeps 4.7
  // behaviour — the app keeps advertising listening until the user
  // explicitly opts in.
  'settings.idleAwayLabel': 'Wstrzymuj, gdy pulpit jest bezczynny',
  'settings.idleAwayHint':
    'Przestaje reklamować status Spotify, gdy pulpit nie odnotuje wejścia z klawiatury/myszy przez podaną liczbę sekund. 60–3600; 0 wyłącza. Systemy Linux/macOS nigdy nie zgłaszają tego sygnału.',
  'settings.extraWordsLabel': 'Niestandardowe słowa do filtrowania',
  'settings.extraWordsHint':
    'Jedno słowo lub fraza w wierszu. Stosowane z tymi samymi granicami co lista wbudowana.',
  'settings.extraWordsPlaceholder': 'słowo lub fraza',
  'settings.extraWordsClampHint':
    'Zachowywane jest tylko pierwszych {max} wpisów po {chars} znaków — filtrowane będzie {kept}.',
  'settings.pauseBackoffMaxLabel': 'Maksimum wycofywania przy wstrzymaniu (sekundy)',
  'settings.pauseBackoffClampHint':
    'Dozwolony zakres to {min}–{max} sekund; użyta zostanie wartość {effective}.',
  'dashboard.presenceGatedQuietHours': 'Status wstrzymany — obowiązują ciche godziny',
  'dashboard.presenceGatedTrackRule': 'Status wstrzymany — dopasowano regułę utworu',
  'dashboard.presenceGatedManualStatus':
    'Status wstrzymany — ustawiono komunikat statusu ręcznie',
  'dashboard.presenceGatedOutOfOffice': 'Status wstrzymany — tryb nieobecności',
  'dashboard.presenceGatedPresenting':
    'Status wstrzymany — prezentacja lub aplikacja pełnoekranowa',
  'dashboard.presenceGatedQuietTime': 'Status wstrzymany — włączony Asystent koncentracji',
  'dashboard.presenceGatedIdle': 'Status wstrzymany — pulpit bezczynny',
  // 4.7.0 — S6 (tray localization): `config.locale` is the single source of
  // truth, so the picker also drives the tray and the native app menu.
  'settings.languageHint':
    'Dotyczy również menu zasobnika i natywnego menu aplikacji.',
  // #984: the follow-system-language toggle beside the picker. The mode
  // re-resolves from the OS language on every boot (and on change) and
  // persists the resolution, so the tray and the native menu follow too.
  'settings.languageFollowSystemLabel': 'Podążaj za językiem systemu',
  'settings.languageFollowSystemHint':
    'Po włączeniu powyższy wybór języka jest ignorowany: aplikacja ustala język na podstawie języka systemu operacyjnego przy każdym uruchomieniu. Nieobsługiwane języki systemu nadal wracają do angielskiego.',

  // Issue #932: the banner used to be Teams-only
  // (`settings.teamsPersistWarning`, added with #562). The new
  // Spotify mirror ships the same copy with a `{provider}` placeholder
  // — the Settings card passes the display name (`Microsoft Teams` or
  // `Spotify`) so one banner covers both providers.
  'settings.authPersistWarning':
    'Zalogowano, ale to urządzenie nie mogło zapisać sesji {provider} — działa do zamknięcia. Połącz {provider} ponownie, aby spróbować zapisać ją jeszcze raz.',

  // 4.7.0 — S4 (rules engine)
  'rules.quietWindowHint':
    'Ciche godziny przechodzą przez północ — zakres 22:00–07:00 biegnie przez noc. Każda połowa należy do nocy, w której się zaczyna: przy wybranym tylko poniedziałku okno obejmuje noc z poniedziałku na wtorek. Godzina końcowa 00:00 oznacza północ (koniec dnia), a początek równy końcowi nigdy nie pasuje.',
  'rules.pausePollingLabel': 'Zatrzymaj odpytywanie w tym oknie',
  'rules.pausePollingHint':
    'Gdy to okno jest aktywne, Spotify nie jest w ogóle odpytywane — brak aktualizacji statusu i brak wywołań Teams. Odpytywanie wznawia się samo po zakończeniu okna.',
  'rules.trackRulesOrderHint':
    'Reguły są oceniane od góry do dołu — wygrywa pierwsze dopasowanie. Reguła bez dni tygodnia obowiązuje codziennie, godzina końcowa 00:00 oznacza koniec dnia, a początek równy końcowi nigdy nie pasuje. Okno przechodzące przez północ należy do nocy, w której się zaczyna: przy wybranym tylko poniedziałku zakres 22:00–07:00 obejmuje noc z poniedziałku na wtorek.',
  'rules.ruleStart': 'Początek okna reguły',
  'rules.ruleEnd': 'Koniec okna reguły',
  'rules.ruleDays': 'Aktywne dni tej reguły (brak wyboru = codziennie)',
  'rules.moveRuleUp': 'Przenieś regułę {n} w górę',
  'rules.moveRuleDown': 'Przenieś regułę {n} w dół',
  'rules.manualStatusLabel': 'Wstrzymaj i zatrzymaj tekst statusu',
  'rules.manualStatusHint':
    'Tekst publikowany jako status w Teams, gdy odtwarzanie jest wstrzymane i gdy nic nie jest odtwarzane. Emoji muzyki jest dodawane automatycznie; wyczyszczenie pola przywraca wartość domyślną.',
  'rules.pausedStatusPlaceholder': 'Wstrzymano',
  'rules.stoppedStatusPlaceholder': 'Brak odtwarzania w Spotify',

  // 4.7.0 — S5 (log rotation + settings export/import)
  'settings.sectionLogging': 'Rejestrowanie',
  'settings.loggingEnabledLabel': 'Zapisuj plik dziennika',
  'settings.logLevelLabel': 'Poziom dziennika',
  'settings.logMaxSizeLabel': 'Maksymalny rozmiar pliku dziennika (MB)',
  'settings.logKeepFilesLabel': 'Liczba przechowywanych archiwalnych plików dziennika',
  'settings.logRotationHint':
    'Limit rozmiaru i liczba archiwalnych plików obowiązują od następnego uruchomienia PresenceJam. Aktualnie zapisywany dziennik się do nich nie wlicza: folder dzienników mieści co najwyżej o jeden plik więcej niż ustawiona liczba. Wyłączenie rejestrowania lub zmiana poziomu działa natychmiast.',
  'settings.sectionBackup': 'Kopia zapasowa',
  'settings.backupHint':
    'Eksport zapisuje kopię tych ustawień, którą można zachować lub przenieść na inny komputer. Klucz tajny klienta Spotify pozostaje w pęku kluczy systemu i nigdy nie jest dołączany — a plik, który go zawiera, jest odrzucany przy imporcie.',
  'settings.backupExport': 'Eksportuj ustawienia…',
  'settings.backupImport': 'Importuj ustawienia…',
  'settings.backupExportDialogTitle': 'Eksport ustawień PresenceJam',
  'settings.backupImportDialogTitle': 'Import ustawień PresenceJam',
  'settings.backupConfirmOverwrite':
    'Import zastępuje wszystkie bieżące ustawienia. Bieżący plik jest zachowywany obok jako config.json.bak. Kontynuować?',
  'settings.backupExported': 'Wyeksportowano ustawienia do {path}',
  'settings.backupImported': 'Zaimportowano ustawienia z {path}',
  'settings.backupError': 'Nie można ukończyć akcji kopii zapasowej: {error}',

  // 4.7.0 — S9 (issue #677: the tray snooze / "pause sync for a while")
  'dashboard.snoozeChip': 'Uśpione — pozostało {remaining} (do {time})',
  'dashboard.snoozeResume': 'Wznów teraz',
  'dashboard.snoozeResuming': 'Wznawianie…',
  'dashboard.snoozeResumeFailed':
    'Nie można wznowić synchronizacji. Uśpienie jest nadal zapisane — spróbuj ponownie.',
  // Issue #736: the chip's live region announces entry/exit once; the
  // per-second countdown is no longer in the live region.
  'dashboard.snoozeStatusEnd': 'Wznowiono synchronizację',

  // 4.7.0 — S7 notifications (#675): one toggle per desktop-notification
  // class, plus the copy for the three classes the always-mounted layout
  // dispatches (track changes keep dispatching from the Dashboard card).
  'settings.notificationsTrackChange': 'Powiadamiaj o zmianie utworu',
  'settings.notificationsSyncStopped': 'Powiadamiaj o samoczynnym zatrzymaniu synchronizacji',
  'settings.notificationsAuthRequired': 'Powiadamiaj o konieczności ponownego zalogowania do Teams',
  'settings.notificationsUpdateStaged': 'Powiadamiaj o aktualizacji do zainstalowania przy zamknięciu',
  'notifications.syncStoppedTitle': 'PresenceJam przestał synchronizować',
  'notifications.syncStoppedBody':
    'Synchronizacja statusu zatrzymała się samoczynnie. Otwórz PresenceJam, aby ją uruchomić ponownie.',
  'notifications.authRequiredTitle': 'Wymagane logowanie do Teams',
  'notifications.authRequiredBody':
    'Sesja Teams wygasła. Zaloguj się ponownie, aby status był nadal synchronizowany.',
  'notifications.updateStagedTitle': 'Aktualizacja gotowa',
  'notifications.updateStagedBody': 'PresenceJam {version} zostanie zainstalowany przy zamknięciu.',
  // 4.7.0 — update channel (#678)
  'settings.sectionUpdates': 'Aktualizacje',
  'settings.updateChannelLabel': 'Kanał wydań',
  'settings.updateChannelStable': 'Stabilny',
  'settings.updateChannelBeta': 'Beta',
  'settings.updateChannelHint':
    'Kompilacje beta korzystają z kroczącego kanału beta, gdy jest dostępny; jeśli go brakuje lub nie ma nowszej wersji, kanał Beta wraca do stabilnego wydania. Kompilacje beta instalują się tylko przy zamknięciu.',
  'update.betaOnQuitOnly':
    'Kanał beta: aktualizacje instalują się przy zamknięciu — w kanale Beta nie ma ścieżki pobierania i ponownego uruchamiania.',
  // 4.7.0 — S8 global hotkeys
  'settings.sectionShortcuts': 'Skróty globalne',
  'settings.shortcutsHint':
    'Działają, gdy okno jest ukryte. Kliknij pole i naciśnij żądaną kombinację — pole rejestruje to, co zostanie naciśnięte, a nie wpisane.',
  'settings.shortcutTogglePlayback': 'Przełącz odtwarzanie',
  'settings.shortcutToggleSync': 'Wstrzymaj lub wznów synchronizację',
  'settings.shortcutUnbound': 'Nie ustawiono — kliknij i naciśnij kombinację',
  'settings.shortcutClear': 'Wyczyść',
  'settings.shortcutRegistered': 'Aktywny',
  'settings.shortcutNotRegistered': 'Niezarejestrowany na tym pulpicie',
  'settings.shortcutCaptureReleased':
    'Zwolniono podczas nagrywania — bieżące powiązanie zadziałałoby zamiast zostać nagrane',
  'settings.shortcutRejected': 'Nie można użyć: {reason}',
  'settings.shortcutRegistrationFailed': 'Rejestracja nie powiodła się na tym pulpicie: {reason}',
  // Issue #968: typed reason codes from the Rust validator. The Settings
  // card maps each `kind` to a dictionary entry so the rejection copy is
  // localized; `shortcutReasonUnknown` renders the backend's free-form text
  // for genuinely foreign refusals (compositor / app-owned combos).
  'settings.shortcutReasonNotAKey': '„{accelerator}” nie jest rozpoznawanym skrótem',
  'settings.shortcutReasonConflict':
    'Konflikt ze skrótem {other} — jeden akcelerator nie może sterować obiema akcjami',
  // Issue #810: a bare key would be grabbed system-wide. Function keys
  // (F1–F24) and media keys are exempt and bind without a modifier.
  'settings.shortcutReasonNeedsModifier':
    '„{accelerator}” wymaga co najmniej jednego modyfikatora — pojedynczy klawisz byłby przechwytywany w każdej aplikacji',
  'settings.shortcutReasonAutostart':
    'Uruchamianie przy logowaniu nie powiodło się: {cause}',
  'settings.shortcutReasonUnknown': '{message}',
  'settings.shortcutReasonX11Unavailable':
    'Skróty globalne wymagają osiągalnego wyświetlacza X11 na tym pulpicie',
  'settings.shortcutReasonWorkerUnavailable':
    'Nie można zweryfikować procesu skrótów globalnych; skróty są niedostępne',
  // 4.7.0 — S12 hygiene (theme/density)
  'settings.themeSystem': 'Systemowy',
  'settings.themeHint':
    '„Systemowy” podąża za wyglądem systemu operacyjnego; Ciemny i Jasny pozostają przypięte.',
  'settings.densityCompactLabel': 'Kompaktowe odstępy',
  'settings.densityHint': 'Zacieśnia odstępy i skalę pisma. Niezależne od motywu.',
  // --- 5.0 wave1 i18n-lib ---
  // Key requests routed through this slice's dictionaries (the owning slice
  // cannot edit them).
  'onboarding.pollIntervalClamped':
    'Zapisany interwał to {stored} s, poza zakresem tego kroku {min}–{max} s — użyta zostanie wartość {seconds} s.',
  'update.stagingProgressLabel': 'Przygotowywanie aktualizacji',
  'rules.presenceAvailable': 'Dostępny',
  'rules.presenceBusyCall': 'Zajęty — połączenie',
  'rules.presenceBusyConference': 'Zajęty — telekonferencja',
  'rules.presenceAway': 'Zaraz wracam',
  'rules.presenceDndPresenting': 'Nie przeszkadzać — prezentowanie',
  // --- 5.0 wave3 features-presence ---
  'dashboard.manualStatusTitle': 'Ręczny status',
  'dashboard.manualStatusPlaceholder': 'Ustaw status widoczny dla zespołu przez jakiś czas',
  'dashboard.manualStatusExpiryLabel': 'Wygasa po',
  'dashboard.manualStatusExpiry15': '15 minut',
  'dashboard.manualStatusExpiry30': '30 minut',
  'dashboard.manualStatusExpiry60': '1 godzina',
  'dashboard.manualStatusExpiry120': '2 godziny',
  'dashboard.manualStatusSet': 'Ustaw status',
  'dashboard.manualStatusClear': 'Wyczyść status',
  'dashboard.manualStatusActive': 'Aktywny do {expiry}',
  'dashboard.manualStatusActiveEmpty': 'Aktywny (wkrótce wygasa)',
  'dashboard.manualStatusRecentTitle': 'Ostatnie statusy',
  'dashboard.manualStatusRecentEmpty': 'Brak ostatnich statusów',
  'dashboard.manualStatusFiltered': 'Status został przepisany przez filtr wulgaryzmów',
  'dashboard.activityTitle': 'Aktywność',
  'dashboard.activityEmpty': 'Brak decyzji — uruchom synchronizację, aby zobaczyć wybory aplikacji',
  'dashboard.volumeLabel': 'Głośność',
  'dashboard.volumeAria': 'Suwak głośności Spotify',
  'dashboard.seekAria': 'Kliknij, aby przewinąć',
  'dashboard.seekUnavailableAria': 'To urządzenie nie obsługuje przewijania',

  // --- 5.0 wave3 features-outlook ---
  'rules.importWorkingHours': 'Importuj godziny pracy z Outlooka',
  'rules.importWorkingHoursHint': 'Odczytuje kartę Godziny pracy i zamienia każdy blok wolny w regułę cichych godzin. Sprawdź podgląd przed zastosowaniem.',
  'rules.importWorkingHoursPreviewTitle': 'Podgląd godzin pracy z Outlooka',
  'rules.importWorkingHoursApply': 'Zastosuj te reguły',
  'rules.importWorkingHoursReplace': 'Zamień istniejące reguły cichych godzin',
  'rules.importWorkingHoursCancel': 'Anuluj',
  'rules.importWorkingHoursReplaceHint': 'Zastępuje istniejące reguły cichych godzin importowanym zestawem. Odznacz, aby zachować oba.',
  'rules.importWorkingHoursDaysLabel': 'Outlook zgłasza {days} pracy w godzinach {start}–{end}',
  'rules.importWorkingHoursDaysAllOff': 'Outlook nie zgłasza dni roboczych — brak danych do importu',

  // --- 5.0 wave3 features-gating ---

  // --- 5.0 wave3 features-profiles ---
  'rules.testTitle': 'Przetestuj te reguły',
  'rules.testHint':
    'Wpisz przykładowy utwór, wybierz minutę dnia + dzień tygodnia i zobacz dokładnie, która reguła (jeśli jakakolwiek) by zadziałała.',
  'rules.testArtistLabel': 'Wykonawca',
  'rules.testTrackLabel': 'Tytuł utworu',
  'rules.testAlbumLabel': 'Album (opcjonalnie)',
  'rules.testShowLabel': 'Program / podcast (opcjonalnie)',
  'rules.testDeviceLabel': 'Urządzenie Spotify (opcjonalnie)',
  'rules.testPlaylistLabel': 'URI playlisty (opcjonalnie)',
  'rules.testDurationLabel': 'Czas trwania (mm:ss, opcjonalnie)',
  'rules.testWeekdayLabel': 'Dzień tygodnia',
  'rules.testMinuteLabel': 'Minuta dnia (GG:MM)',
  'rules.testRun': 'Uruchom test',
  'rules.testRunning': 'Uruchamianie…',
  'rules.testSummaryNoMatch': 'Żadna reguła nie pasowałaby do tego utworu.',
  'rules.testSummaryRuleMatched': 'Reguła {index} zadziałałaby: {summary}',
  'rules.testStepMatched': 'pasuje',
  'rules.testStepNotMatched': 'NIE pasuje',
  'rules.testStepReason': 'przyczyna: {reason}',
  'rules.testStepDisabled': 'reguła jest wyłączona',
  'rules.testStepScheduleOutside': 'harmonogram nie obejmuje tej minuty dnia',
  'rules.testStepNegated': 'negacja odwróciła warunki',
  'rules.matchKindLabel': 'Styl dopasowania',
  'rules.matchKindSubstring': 'Podciąg (domyślny)',
  'rules.matchKindExact': 'Dokładne dopasowanie',
  'rules.matchKindGlob': 'Wzorzec glob',
  'rules.albumSubstringLabel': 'Album zawiera…',
  'rules.showSubstringLabel': 'Program zawiera…',
  'rules.deviceSubstringLabel': 'Urządzenie zawiera…',
  'rules.playlistUriLabel': 'URI playlisty zawiera…',
  'rules.minDurationLabel': 'Minimalny czas trwania (sekundy)',
  'rules.negateLabel': 'Negacja (dopasuj, gdy warunki NIE zachodzą)',
  'rules.actionLabel': 'Akcja',
  'rules.actionSuppress': 'Wstrzymaj status',
  'rules.actionReplace': 'Zamień status na…',
  'rules.actionSnoozeMinutes': 'Uśpij na minuty…',
  'rules.actionProfile': 'Przełącz na profil…',
  'rules.actionPresence': 'Ustaw parę obecności…',
  'rules.actionReplaceStatusPlaceholder': 'Tekst statusu',
  'rules.actionSnoozePlaceholder': 'Minuty',
  'rules.actionProfilePlaceholder': 'Nazwa profilu',
  'rules.actionAvailabilityPlaceholder': 'Dostępność',
  'rules.actionActivityPlaceholder': 'Aktywność',
  'dashboard.gateWhyTitle': 'Dlaczego status jest wstrzymany?',
  'dashboard.gateWhyShow': 'Pokaż przyczynę',
  'dashboard.gateWhyHide': 'Ukryj przyczynę',
  'dashboard.gateWhyEmpty': 'Brak aktywnej bramki — status synchronizuje się normalnie.',

  'profiles.sectionTitle': 'Profile obecności',
  'profiles.sectionHint':
    'Zapisz nazwaną nakładkę (format statusu, bramki, reguły) i przełączaj się zasobnikiem, skrótem klawiszowym lub CLI.',
  'profiles.empty': 'Nie zdefiniowano profili.',
  'profiles.addProfile': 'Dodaj profil',
  'profiles.removeProfile': 'Usuń',
  'profiles.activeProfileLabel': 'Aktywny profil',
  'profiles.activeProfileNone': 'Brak — użyj konfiguracji bazowej',
  'profiles.overlayStatusFormatLabel': 'Format statusu',
  'profiles.overlayClearOnPauseLabel': 'Czyść status po wstrzymaniu',
  'profiles.overlayAvailabilitySyncLabel': 'Synchronizuj dostępność z Teams',
  'profiles.overlayGateOutOfOfficeLabel': 'Bramka przy nieobecności',
  'profiles.overlayGatePresentingLabel': 'Bramka przy aplikacjach pełnoekranowych',
  'profiles.overlayIdleAwayLabel': 'Przestań reklamować po bezczynności (sekundy)',
  'profiles.overlayPreferredPresenceLabel': 'Preferowana obecność',
  'profiles.overlayRulesLabel': 'Podzbiór reguł',
  'profiles.overlayRulesHint': 'Zastępuje bazową listę reguł, gdy ten profil jest aktywny.',
  'profiles.overlayNotificationsLabel': 'Powiadomienia',
  'profiles.profileNameLabel': 'Nazwa profilu',
  'profiles.profileNamePlaceholder': 'np. Skupienie, Trening',
  'profiles.profileNameDuplicate': 'Profil o tej nazwie już istnieje.',
  'profiles.profileNameTooLong': 'Nazwy profili mogą mieć maksymalnie 32 znaki.',
  'profiles.profileNameMissing': 'Nazwa profilu nie może być pusta.',
  'profiles.activeProfileUnknown': 'Nieznany profil — używana jest konfiguracja bazowa.',
  'tray.profilesMenu': 'Profile obecności',
  'tray.profilesMenuNone': 'Nie zdefiniowano profili',
  'tray.profilesMenuActivateBase': 'Użyj konfiguracji bazowej',
  'cli.profileFlag': 'Przełącz na nazwany profil obecności i zakończ.',
  'cli.profileActive': 'Aktywny profil to teraz „{name}”.',
  'cli.profileUnknown': 'Brak profilu o nazwie „{name}” — aktywna pozostaje konfiguracja bazowa.',
  'settings.shortcutToggleProfile': 'Przełączaj profile obecności',

  // --- 5.0 wave3 playback-source ---
  'onboarding.playbackSourceMacNote':
    'Ten komputer działa pod systemem macOS — dostępne jest tylko źródło odtwarzania Spotify. Systemowe źródło sesji multimediów (Windows SMTC / Linux MPRIS) nie jest dostępne w systemie macOS, więc PresenceJam automatycznie wraca do Spotify. Przełącz na Spotify, jeśli kreator zgłasza „brak utworu”, mimo że muzyka jest odtwarzana w innej aplikacji.',

  // --- #1120: the snooze entry announcement is count-aware. A one-minute
  // snooze used to announce "Sync paused for 1 minutes"; German needs
  // "Minute" and French "minute" in the singular.
  'dashboard.snoozeStatusStart_one': 'Synchronizacja wstrzymana na {minutes} minutę',
  'dashboard.snoozeStatusStart_other': 'Synchronizacja wstrzymana na {minutes} minut',
  // #1154: CLDR `few` (2–4, 22–24…): nominative plural "minuty".
  // Model-written per #984 provenance — human review pending.
  'dashboard.snoozeStatusStart_few': 'Synchronizacja wstrzymana na {minutes} minuty',

  // --- #739: the wizard's view name, announced on navigation (there is no
  // heading bar above the step, so the step title cannot stand in for it).
  'onboarding.title': 'Konfiguracja',

  // --- #954 / P7: the compact header's abbreviated sync badge. The full
  // "Synchronisierung" / "Synchronisation" label ellipsises into an unreadable
  // fragment at the 400px minimum, and a second badge row is worse. "Sync" is
  // the established short form in all three locales.
  'dashboard.syncingShort': 'Sync',
  // --- #966 / #981 Settings draft actions ---
  'settings.revertChanges': 'Cofnij zmiany',
  'rules.undoRemove': 'Cofnij usunięcie',
};
