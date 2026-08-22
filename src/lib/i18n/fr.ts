/**
 * C6 i18n foundation (docs/scope-3.3.md §C6) — French dictionary.
 * Typed against `Dict`, so every English key must be present.
 */

import type { Dict } from './en';

export const fr: Dict = {
  // ── common ────────────────────────────────────────────────────────
  'common.back': 'Retour',
  'common.backToDashboard': 'Retour au tableau de bord',
  'common.checkNow': "Je me suis connecté — vérifier maintenant",
  'common.connected': 'Connecté',
  'common.dismiss': 'Fermer',
  'common.goTo': 'Allez sur',
  'common.andEnterCode': 'et saisissez ce code',
  'common.launchAtLogin': "Lancer à l'ouverture de session",
  'common.loading': 'Chargement...',
  'common.notConnected': 'Non connecté',
  'common.reconnecting': 'Reconnexion…',
  'common.reconnect': 'Reconnecter',
  'common.resetToDefault': 'Rétablir les valeurs par défaut',
  'common.themeToggle': 'Changer de thème',
  'common.waiting': 'En attente…',
  'common.waitingForSignIn': "En attente de connexion…",
  'common.yes': 'Oui',
  'common.no': 'Non',
  'common.tagline': 'Spotify → Statut Teams',

  // ── dashboard ─────────────────────────────────────────────────────
  'dashboard.spotifyOff': 'Spotify désactivé',
  'dashboard.teamsOff': 'Teams désactivé',
  'dashboard.syncing': 'Synchronisation',
  'dashboard.logsDetachedTitle':
    'Journaux (détachés — cliquez pour activer)',
  'dashboard.logsTitle': 'Journaux',
  'dashboard.logsDetachedAria':
    'Journaux (détachés dans une fenêtre séparée)',
  'dashboard.openLogsAria': 'Ouvrir les journaux',
  'dashboard.diagnostics': 'Diagnostics',
  'dashboard.openDiagnosticsAria': 'Ouvrir les diagnostics',
  'dashboard.settingsDetachedTitle':
    'Paramètres (détachés — cliquez pour activer)',
  'dashboard.settings': 'Paramètres',
  'dashboard.settingsDetachedAria':
    'Paramètres (détachés dans une fenêtre séparée)',
  'dashboard.openSettingsAria': 'Ouvrir les paramètres',
  'dashboard.about': 'À propos',
  'dashboard.aboutAria': 'À propos de PresenceJam',
  'dashboard.pauseSync': 'Mettre la synchronisation en pause',
  'dashboard.resumeSync': 'Reprendre la synchronisation',
  'dashboard.presenceGated':
    "Statut en pause pendant que vous êtes occupé/en réunion",
  'dashboard.setupRequired': 'Configuration requise',
  'dashboard.setupHint':
    "Connectez Spotify et Microsoft Teams pour que vos titres en cours de lecture pilotent votre statut Teams.",
  'dashboard.continueSetup': 'Poursuivre la configuration',
  'dashboard.playing': 'En lecture',
  'dashboard.paused': 'En pause',
  'dashboard.liveStreamAria': 'Flux en direct — position inconnue',
  'dashboard.yourTeamsStatus': 'Votre statut Teams',
  'dashboard.nothingPlaying': 'Aucune lecture',
  'dashboard.nothingPlayingHint':
    "Lancez une lecture sur Spotify et nous la transmettrons à Teams.",
  'dashboard.syncCrashed':
    "La synchronisation s'est arrêtée de manière inattendue. Veuillez redémarrer PresenceJam.",
  'dashboard.credentialCheckFailed':
    "Impossible de vérifier les identifiants — veuillez réessayer.",
  'dashboard.statusNotConfigured': 'Non configuré',
  'dashboard.statusNoTrack': 'Aucun titre en lecture',
  'dashboard.live': 'En direct',

  // ── logs ──────────────────────────────────────────────────────────
  'logs.title': 'Journaux',
  'logs.filterAria': 'Filtre de niveau de journal',
  'logs.level.all': 'Tous',
  'logs.level.trace': 'Trace',
  'logs.level.debug': 'Debug',
  'logs.level.info': 'Info',
  'logs.level.warning': 'Avertissement',
  'logs.level.error': 'Erreur',
  'logs.countOne': '{count} entrée',
  'logs.countOther': '{count} entrées',
  'logs.popOut': 'Détacher',
  'logs.clear': 'Effacer',
  'logs.openFolder': 'Ouvrir le dossier',
  'logs.empty': 'Aucune entrée de journal pour le moment',
  'logs.emptyHint':
    "Les entrées en direct apparaissent ici pendant l'exécution de la boucle d'interrogation.",

  // ── settings ──────────────────────────────────────────────────────
  'settings.title': 'Paramètres',
  'settings.popBackIn': 'Rattacher',
  'settings.popOutActionTitle':
    'Détacher dans sa propre fenêtre',
  'settings.unsavedChanges': 'Modifications non enregistrées',
  'settings.sectionSpotify': 'Spotify',
  'settings.sectionTeams': 'Microsoft Teams',
  'settings.sectionPresence': 'Présence',
  'settings.sectionStatusFormat': 'Format du statut',
  'settings.sectionPolling': 'Interrogation',
  'settings.sectionNotifications': 'Notifications',
  'settings.sectionAppearance': 'Apparence',
  'settings.clientId': 'ID client',
  'settings.clientIdPlaceholder': "Saisir l'ID client Spotify",
  'settings.clientSecret': 'Secret client',
  'settings.secretStoredHint':
    "Stocké en toute sécurité dans le trousseau de votre système d'exploitation. Pour le remplacer, relancez l'onboarding.",
  'settings.secretNotConfigured': 'Non configuré.',
  'settings.runOnboarding': "Lancer l'onboarding",
  'settings.toSetUpSpotify': 'pour configurer Spotify.',
  'settings.reconnectSpotify': 'Reconnecter Spotify',
  'settings.completeAuthInBrowser':
    "Terminez l'authentification dans le navigateur.",
  'settings.playbackScopeBanner':
    'Le contrôle de la lecture nécessite une reconnexion unique.',
  'settings.teamsAuthHint':
    "L'authentification Teams utilise votre compte Microsoft 365. Aucune configuration supplémentaire requise.",
  'settings.presenceScopeBanner':
    'Les fonctions de présence nécessitent une reconnexion Teams unique.',
  'settings.availabilitySyncLabel':
    "Afficher « Disponible » pendant l'écoute",
  'settings.availabilitySyncHint':
    "Désactivé par défaut. Affiche « Disponible » (et non « Occupé ») dans Teams pendant la lecture d'un titre, car setPresence ne prend en charge que la combinaison Busy/InACall — voir la limite de setPresence.",
  'settings.presenceGateLabel':
    'Mettre le statut en pause pendant réunions/appels/Ne pas déranger',
  'settings.presenceGateHint':
    "Activé par défaut. Ignore l'écriture de votre statut Spotify quand Teams indique que vous êtes occupé, en réunion, en appel ou en train de présenter.",
  'settings.formatTemplate': 'Modèle de format',
  'settings.formatTemplatePlaceholder': '🎵 {artist} - {track} 🎧',
  'settings.livePreview': 'Aperçu en direct',
  'settings.placeholdersHint':
    'Paramètres disponibles : {artist}, {track}, {album}, {emoji}',
  'settings.profanityFilterLabel':
    'Filtrer les grossièretés dans le statut',
  'settings.placeholderTextLabel': 'Texte de substitution',
  'settings.placeholderTextHint':
    "Utilisez {emoji} pour l'état de lecture (🎵 en lecture / ⏸ en pause). Affiché quand des grossièretés sont détectées dans les informations du titre.",
  'settings.placeholderTextPlaceholder': 'Écoute actuellement Spotify',
  'settings.defaultIntervalLabel': 'Intervalle par défaut : {seconds}s',
  'settings.minIntervalLabel': 'Intervalle min (s)',
  'settings.maxIntervalLabel': 'Intervalle max (s)',
  'settings.clampHint':
    "L'intervalle min dépasse l'intervalle max — le max sera enregistré comme {max}s.",
  'settings.notificationsToggle':
    'Notification bureau au changement de titre',
  'settings.notificationsHint':
    'Affiche une notification système quand le titre change. Désactivé par défaut.',
  'settings.themeLabel': 'Thème',
  'settings.themeDark': 'Sombre',
  'settings.themeLight': 'Clair',
  'settings.languageLabel': 'Langue',
  'settings.autostartError':
    "Échec de la mise à jour du lancement à l'ouverture de session : {error}",
  'settings.saveChanges': 'Enregistrer les modifications',
  'settings.saving': 'Enregistrement…',
  'settings.saved': 'Paramètres enregistrés !',
  'settings.failedToSave': "Échec de l'enregistrement",
  'settings.openLogsFolder': 'Ouvrir le dossier des journaux',
  'settings.previewUnavailable': '(aperçu indisponible)',

  // ── diagnostics ───────────────────────────────────────────────────
  'diagnostics.title': 'Diagnostics',
  'diagnostics.localOnlyHint':
    'Instantané purement local — peut être joint sans risque à un rapport de bug.',
  'diagnostics.copy': 'Copier les diagnostics',
  'diagnostics.saveToFile': 'Enregistrer dans un fichier',
  'diagnostics.copied': 'Diagnostics copiés dans le presse-papiers.',
  'diagnostics.copyFailed':
    'Échec de la copie — utilisez plutôt « Enregistrer dans un fichier ».',
  'diagnostics.savedToDownloads':
    'Diagnostics enregistrés dans votre dossier de téléchargements.',
  'diagnostics.saveFailed':
    "Échec de l'enregistrement — utilisez plutôt « Copier les diagnostics ».",
  'diagnostics.collecting': 'Collecte des diagnostics…',
  'diagnostics.collectFailed': 'Échec de la collecte des diagnostics',
  'diagnostics.versions': 'Versions',
  'diagnostics.configuration': 'Configuration',
  'diagnostics.connections': 'Connexions',
  'diagnostics.recentLogLines': 'Lignes de journal récentes',
  'diagnostics.app': 'PresenceJam',
  'diagnostics.tauri': 'Tauri',
  'diagnostics.os': 'Système',
  'diagnostics.spotifyClientId': 'ID client Spotify',
  'diagnostics.redirectUri': 'URI de redirection',
  'diagnostics.notSet': '(non défini)',
  'diagnostics.clientSecretKeychain': 'Secret client dans le trousseau',
  'diagnostics.clearOnPause': 'Effacer le statut en pause',
  'diagnostics.profanityFilter': 'Filtre de grossièretés',
  'diagnostics.startMinimized': 'Démarrer réduit',
  'diagnostics.availabilitySync': 'Synchro de disponibilité Teams',
  'diagnostics.presenceGate': "Verrou de présence",
  'diagnostics.pollInterval':
    'Intervalle d’interrogation (défaut/min/max)',
  'diagnostics.logging': 'Journalisation',
  'diagnostics.loggingEnabled': 'activée ({level})',
  'diagnostics.loggingDisabled': 'désactivée',
  'diagnostics.launchAtLogin': "Lancer à l'ouverture de session",
  'diagnostics.spotifyConnected': 'Spotify connecté',
  'diagnostics.spotifyTokenExpires': "Expiration du token Spotify",
  'diagnostics.teamsConnected': 'Teams connecté',
  'diagnostics.teamsTokenExpires': 'Expiration du token Teams',
  'diagnostics.expired': '(expiré)',
  'diagnostics.keychainSpotifySecret':
    'Trousseau : secret Spotify présent',
  'diagnostics.keychainEncryptionKey':
    'Trousseau : clé de chiffrement des tokens présente',
  'diagnostics.tokensNeverIncluded':
    "Les valeurs des tokens ne sont jamais incluses — uniquement les horodatages d'expiration et les indicateurs de présence.",
  'diagnostics.noLogLinesYet':
    'Aucune ligne de journal disponible pour le moment.',

  // ── reconnect ─────────────────────────────────────────────────────
  'reconnect.title': 'Reconnexion',
  'reconnect.description':
    'Votre session a expiré. Reconnectez-vous ci-dessous pour reprendre la synchronisation.',
  'reconnect.missingCredentials': 'Identifiants manquants',
  'reconnect.failed': 'Échec',
  'reconnect.readyToReconnect': 'Prêt à se reconnecter',
  'reconnect.needsReconnect': 'Reconnexion nécessaire',
  'reconnect.spotifyOk': 'Spotify reconnecté avec succès.',
  'reconnect.spotifyNotConfigured':
    "Les identifiants Spotify ne sont pas configurés sur cette machine.",
  'reconnect.completeAuthInOpenedBrowser':
    "Terminez l'authentification dans la fenêtre de navigateur ouverte.",
  'reconnect.tryAgain': 'Réessayer',
  'reconnect.clickBelowSpotify':
    'Cliquez ci-dessous pour reconnecter votre compte Spotify.',
  'reconnect.teamsOk': 'Teams reconnecté avec succès.',
  'reconnect.reconnectSpotify': 'Reconnecter Spotify',
  'reconnect.clickBelowTeams':
    'Cliquez ci-dessous pour reconnecter votre compte Microsoft Teams.',
  'reconnect.missingCredsTitle': 'Identifiants Spotify manquants ?',
  'reconnect.reenterCredsHint':
    "Vous devrez saisir à nouveau votre ID client et votre secret client.",
  'reconnect.goToFullSetup': 'Passer à la configuration complète',
  'reconnect.reconnectTeams': 'Reconnecter Teams',

  // ── about ─────────────────────────────────────────────────────────
  'about.version': 'Version {version}',
  'about.description':
    "Transmet automatiquement ce que vous écoutez sur Spotify vers votre statut Microsoft Teams.",
  'about.statusSync': 'Synchro du statut',
  'about.live': 'En direct',
  'about.auth': 'Authentification',
  'about.authMethod': 'PKCE / Code appareil',
  'about.storage': 'Stockage',
  'about.osKeychain': 'Trousseau du système',
  'about.githubRepo': 'Dépôt GitHub',
  'about.releases': 'Versions',
  'about.reportIssue': 'Signaler un problème',

  // ── update banner ─────────────────────────────────────────────────
  'update.available': 'Mise à jour v{version} disponible',
  'update.stagedQuit':
    'v{version} sera installée quand vous quitterez PresenceJam',
  'update.downloadFailed': 'Échec du téléchargement — {error}',
  'update.downloadAndInstall': 'Télécharger et installer',
  'update.downloading': 'Téléchargement…',
  'update.installOnQuit': "Installer à la fermeture",
  'update.preparing': 'Préparation…',
  'update.dismissAria': 'Fermer la bannière de mise à jour',

  // ── onboarding ────────────────────────────────────────────────────
  'onboarding.stepOf': 'Étape {step} sur 3',
  'onboarding.step1Title': 'Connecter Spotify',
  'onboarding.step1Intro':
    "Collez l'ID client et le secret client de votre application Spotify. Nous lancerons le processus de connexion dès que vous cliquerez sur le bouton.",
  'onboarding.getCredentials': 'Obtenir vos identifiants Spotify',
  'onboarding.instruction1':
    "Ouvrez le tableau de bord développeur Spotify et créez une application.",
  'onboarding.instruction2':
    'Ajoutez presencejam://callback comme URI de redirection.',
  'onboarding.instruction3':
    "Copiez l'ID client et le secret client depuis les paramètres de l'application.",
  'onboarding.clientIdPlaceholder': 'ID client Spotify à 32 caractères',
  'onboarding.clientSecretPlaceholder': 'Secret client Spotify',
  'onboarding.connectSpotify': 'Connecter Spotify',
  'onboarding.signInWaiting': 'Connexion Spotify en attente…',
  'onboarding.manualUrlHint':
    "Terminez l'autorisation dans votre navigateur, ou collez l'URL de redirection ci-dessous.",
  'onboarding.submitCode': 'Envoyer le code',
  'onboarding.connectedToSpotify': 'Connecté à Spotify',
  'onboarding.continue': 'Continuer →',
  'onboarding.step2Title': 'Se connecter avec Microsoft',
  'onboarding.step2Intro':
    "Nous utilisons le flux par code appareil de Microsoft — un code à usage unique à saisir sur une page Microsoft. Aucune configuration supplémentaire requise.",
  'onboarding.startMicrosoftSignIn': 'Lancer la connexion Microsoft',
  'onboarding.connectedToTeams': 'Connecté à Microsoft Teams',
  'onboarding.step3Title': 'Derniers réglages',
  'onboarding.step3Intro':
    "Choisissez l'apparence de votre message de statut et si PresenceJam doit se lancer à l'ouverture de session.",
  'onboarding.statusTemplate': 'Modèle de statut',
  'onboarding.placeholdersHint':
    'Paramètres : {artist}, {track}, {album}, {emoji}',
  'onboarding.pollInterval':
    "Intervalle d'interrogation par défaut : {seconds}s",
  'onboarding.settingUp': 'Configuration…',
  'onboarding.finishSetup': "Terminer la configuration",

  // ── validation / errors ───────────────────────────────────────────
  'validation.clientIdRequired': "L'ID client Spotify est requis.",
  'validation.clientIdFormat':
    "L'ID client Spotify doit comporter exactement 32 caractères hexadécimaux.",
  'validation.clientSecretRequired':
    'Le secret client Spotify est requis.',
  'validation.clientSecretTooShort':
    'Le secret client Spotify semble invalide (trop court — au moins 32 caractères requis).',
  'validation.noCodeInUrl':
    "Aucun code trouvé dans l'URL — collez l'URL de redirection complète avec ?code=…",
  'validation.connectBothFirst':
    "Veuillez connecter Spotify et Teams avant de terminer la configuration.",
  'validation.setupFailed': 'Échec de la configuration : {error}',

  // ── routes / chrome ───────────────────────────────────────────────
  'routes.skipToMainContent': 'Aller au contenu principal',
  'routes.unknownPane': 'Volet inconnu : {pane}'
};
