/**
 * C6 i18n foundation (docs/scope-3.3.md §C6) — French dictionary.
 * Typed against `Dict`, so every English key must be present.
 */

import type { Dict } from './en';

export const fr: Dict = {
  // ── common ────────────────────────────────────────────────────────
  'common.back': 'Retour',
  'common.backToDashboard': 'Retour au tableau de bord',
  'common.checkNow': 'Vérifier l’état de connexion',
  'common.connected': 'Connecté',
  'common.dismiss': 'Fermer',
  'common.openSignInPage': 'Ouvrez la page de connexion Microsoft :',
  'common.enterCodeWhenAsked': 'Saisissez ce code quand on vous le demande :',
  'common.moreActions': 'Plus d’actions',
  'common.launchAtLogin': "Lancer à l'ouverture de session",
  'common.loading': 'Chargement...',
  'common.notConnected': 'Non connecté',
  'common.reconnecting': 'Reconnexion…',
  'common.reconnect': 'Reconnecter',
  'common.resetToDefault': 'Rétablir les valeurs par défaut',
  'common.themeToggle': 'Changer de thème',
  'common.retry': 'Réessayer',
  'common.bootFailed': "Impossible de charger l'état de l'application.",
  'common.waiting': 'En attente…',
  'common.waitingForSignIn': "En attente de connexion…",
  // Best-effort translation (no native review yet) — see wave/slice-e PR body.
  'common.codeExpiresIn': 'Le code expire dans {time}',
  'common.codeExpired': 'Ce code a expiré — il ne peut plus être utilisé.',
  'common.getNewCode': 'Obtenir un nouveau code',
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
    'Statut en pause — vous êtes occupé, en appel ou en présentation',
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
    'Lancez une lecture sur Spotify et elle apparaîtra dans votre statut Teams.',
  'dashboard.syncCrashed':
    'La synchronisation s’est arrêtée de façon inattendue. Appuyez sur Reprendre (▶) pour la relancer.',
  'dashboard.credentialCheckFailed':
    'Impossible de vérifier vos identifiants. Vérifiez votre connexion et réessayez.',
  'dashboard.syncToggleFailed':
    'Impossible de démarrer/arrêter la synchronisation. Réessayez — si le problème persiste, ouvrez Diagnostics depuis l’en-tête du tableau de bord.',
  'dashboard.statusNotConfigured': 'Non configuré',
  'dashboard.statusNoTrack': 'Aucun titre en lecture',
  'dashboard.live': 'En direct',
  'dashboard.refreshStatus': 'Actualiser le statut',
  'dashboard.refreshing': 'Actualisation…',
  'dashboard.refreshFailed':
    "Impossible d'actualiser le statut. Veuillez réessayer.",
  'dashboard.refreshAria': 'Actualiser le statut Teams maintenant',

  // ── logs ──────────────────────────────────────────────────────────
  'logs.title': 'Journaux',
  'logs.filterAria': 'Filtre de niveau de journal',
  'logs.level.all': 'Tous',
  'logs.level.trace': 'Trace',
  'logs.level.debug': 'Debug',
  'logs.level.info': 'Info',
  'logs.level.warning': 'Avertissement',
  'logs.level.error': 'Erreur',
  'logs.count_one': '{count} entrée',
  'logs.count_other': '{count} entrées',
  'logs.showingOf': '{shown} sur {total} affichées',
  'logs.jumpToLatest': 'Aller aux plus récents',
  'logs.popOut': 'Détacher',
  'logs.clear': 'Effacer',
  'logs.openFolder': 'Ouvrir le dossier',
  'logs.empty': 'Aucune entrée de journal pour le moment',
  'logs.emptyHint':
    'Les entrées en direct apparaissent ici dès que la synchronisation démarre et que Spotify joue.',

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
  'settings.sectionPolling': 'Fréquence de synchro',
  'settings.sectionNotifications': 'Notifications',
  'settings.sectionAppearance': 'Apparence',
  'settings.clientId': 'ID client',
  'settings.clientIdPlaceholder': "Saisir l'ID client Spotify",
  'settings.clientSecret': 'Secret client',
  'settings.secretStoredHint':
    'Stocké en toute sécurité dans le trousseau de votre système. Pour le remplacer, retournez au tableau de bord et choisissez « Poursuivre la configuration ».',
  'settings.secretNotConfigured': 'Non configuré.',
  'settings.runOnboarding': "Lancer l'onboarding",
  'settings.toSetUpSpotify': 'pour configurer Spotify.',
  'settings.reconnectSpotify': 'Reconnecter Spotify',
  'settings.completeAuthInBrowser':
    "Terminez l'authentification dans le navigateur.",
  'settings.playbackScopeBanner':
    'Spotify a ajouté des contrôles de lecture. Cliquez sur « Reconnecter » à côté de ce message pour les activer.',
  'settings.spotifySecretConflict':
    'Le secret client du fichier de configuration diffère de celui du trousseau. Reconnectez Spotify pour corriger cela.',
  'settings.teamsAuthHint':
    "L'authentification Teams utilise votre compte Microsoft 365. Aucune configuration supplémentaire requise.",
  'settings.presenceScopeBanner':
    'Teams a ajouté la détection des réunions/appels. Cliquez sur « Reconnecter » à côté de ce message pour l’activer.',
  'settings.availabilitySyncLabel':
    "Afficher « Disponible » pendant l'écoute",
  'settings.availabilitySyncHint':
    'Désactivé par défaut. Quand il est activé, Teams vous affiche « Disponible » (au lieu d’« Occupé ») pendant la lecture. Remarque : Teams affiche toujours « Occupé » pendant les appels et réunions.',
  'settings.presenceGateLabel':
    'Mettre le statut en pause pendant réunions/appels/Ne pas déranger',
  'settings.presenceGateHint':
    "Activé par défaut. Ignore l'écriture de votre statut Spotify quand Teams indique que vous êtes occupé, en réunion, en appel ou en train de présenter.",
  'settings.formatTemplate': 'Modèle de format',
  'settings.formatTemplatePlaceholder': '🎵 {artist} - {track} 🎧',
  'settings.livePreview': 'Aperçu en direct',
  'settings.placeholdersHint':
    'Paramètres disponibles : {artist}, {track}, {album}, {emoji}, {device}, {playlist} (ou {context}), {progress}, {shuffle}, {repeat}. Lecture aléatoire et Répétition n’affichent 🔀/🔁 que lorsqu’ils sont activés.',
  'settings.profanityFilterLabel':
    'Filtrer les grossièretés dans le statut',
  'settings.placeholderTextLabel': 'Texte de substitution',
  'settings.placeholderTextHint':
    "Utilisez {emoji} pour l'état de lecture (🎵 en lecture / ⏸ en pause). Affiché quand des grossièretés sont détectées dans les informations du titre.",
  'settings.placeholderTextPlaceholder': 'Écoute actuellement Spotify',
  'settings.profaneSampleToggle': 'Aperçu avec un titre grossier',
  'settings.defaultIntervalLabel': 'Intervalle par défaut : {seconds}s',
  'settings.minIntervalLabel': 'Intervalle min (s)',
  'settings.maxIntervalLabel': 'Intervalle max (s)',
  'settings.clampHint':
    "L'intervalle min dépasse l'intervalle max — le max sera enregistré comme {max}s.",
  'settings.notificationsHint':
    'Chaque classe activée affiche une notification système — la première peut demander l’autorisation à votre système.',
  'settings.themeLabel': 'Thème',
  'settings.themeDark': 'Sombre',
  'settings.themeLight': 'Clair',
  'settings.languageLabel': 'Langue',
  'settings.autostartError':
    "Échec de la mise à jour du lancement à l'ouverture de session : {error}",
  'settings.saveChanges': 'Enregistrer les modifications',
  'settings.saving': 'Enregistrement…',
  'settings.saved': 'Paramètres enregistrés.',
  'settings.failedToSave': 'Impossible d’enregistrer — vos modifications sont conservées. Réessayez.',
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
  'diagnostics.statusRules': 'Règles de statut',
  'diagnostics.statusRulesValue': '{quiet}/{quietTotal} heures silencieuses, {rules}/{rulesTotal} règles de piste actives',
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
  'diagnostics.failedInstallTitle': "Échec de l'installation de la mise à jour",
  'diagnostics.failedInstallVersion': 'Version',
  'diagnostics.failedInstallError': 'Erreur',
  'diagnostics.failedInstallTimestamp': 'Tentative le',
  'diagnostics.failedInstallDismissFailed':
    "Impossible d'ignorer l'enregistrement de l'échec d'installation.",

  // ── reconnect ─────────────────────────────────────────────────────
  'reconnect.title': 'Reconnexion',
  'reconnect.description':
    'La synchronisation a besoin de votre attention. Reconnectez-vous ci-dessous pour la reprendre.',
  'reconnect.missingCredentials': 'Identifiants manquants',
  'reconnect.failed': 'Échec',
  'reconnect.readyToReconnect': 'Prêt à se reconnecter',
  'reconnect.spotifyOk': 'Spotify reconnecté avec succès.',
  'reconnect.spotifyNotConfigured':
    "Les identifiants Spotify ne sont pas configurés sur cette machine.",
  'reconnect.completeAuthInOpenedBrowser':
    "Terminez l'authentification dans la fenêtre de navigateur ouverte.",
  'reconnect.tryAgain': 'Réessayer',
  'reconnect.clickBelowSpotify':
    'Cliquez ci-dessous pour reconnecter votre compte Spotify.',
  'reconnect.teamsOk': 'Teams reconnecté avec succès.',
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
    'Affiche automatiquement ce que vous écoutez sur Spotify dans votre statut Microsoft Teams.',
  'about.statusSync': 'Synchro du statut',
  'about.live': 'En direct',
  'about.auth': 'Connexion',
  'about.authMethod': 'Spotify + Microsoft',
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
  'update.confirmQuitInstall':
    'Installer v{staged} à la fermeture ? Version actuelle : v{current}.',
  'update.confirmQuitInstallUnknown': 'Installer v{staged} à la fermeture ?',
  'update.stagedVsCurrent':
    'v{staged} sera installée à la fermeture (actuelle v{current})',
  'update.staleSkipped':
    'v{staged} ignorée — votre v{current} actuelle est plus récente.',
  'update.staleSkippedUnknown':
    "v{staged} ignorée — elle n'est pas plus récente que votre version actuelle.",
  'update.installAnyway': 'Installer quand même',

  // ── onboarding ────────────────────────────────────────────────────
  'onboarding.stepOf': 'Étape {step} sur 3',
  'onboarding.step1Title': 'Connecter Spotify',
  'onboarding.step1Intro':
    'Collez ci-dessous votre ID client et votre secret client Spotify, puis choisissez « Connecter Spotify » — nous ouvrirons la page de connexion Spotify.',
  'onboarding.getCredentials': 'Obtenir vos identifiants Spotify',
  'onboarding.instruction1':
    "Ouvrez le tableau de bord développeur Spotify et créez une application.",
  'onboarding.instruction2':
    'Sous « URI de redirection », ajoutez presencejam://callback (cela indique à Spotify où vous renvoyer).',
  'onboarding.instruction3':
    "Copiez l'ID client et le secret client depuis les paramètres de l'application.",
  'onboarding.clientIdPlaceholder': 'ID client Spotify à 32 caractères',
  'onboarding.clientSecretPlaceholder': 'Secret client Spotify',
  'onboarding.connectSpotify': 'Connecter Spotify',
  'onboarding.signInWaiting': 'Connexion Spotify en attente…',
  'onboarding.manualUrlHint':
    'Terminez la connexion Spotify dans votre navigateur, puis collez ci-dessous l’adresse complète de la barre d’adresse.',
  'onboarding.manualUrlLabel': 'URL de redirection Spotify',
  'onboarding.manualUrlPlaceholder': 'presencejam://callback?code=…',
  'onboarding.submitCode': 'Envoyer le code',
  'onboarding.connectedToSpotify': 'Connecté à Spotify',
  'onboarding.continue': 'Continuer →',
  'onboarding.step2Title': 'Connecter Microsoft Teams',
  'onboarding.step2Intro':
    "Nous utilisons le flux par code appareil de Microsoft — un code à usage unique à saisir sur une page Microsoft. Aucune configuration supplémentaire requise.",
  'onboarding.startMicrosoftSignIn': 'Connecter Microsoft Teams',
  'onboarding.connectedToTeams': 'Connecté à Microsoft Teams',
  'onboarding.step3Title': 'Derniers réglages',
  'onboarding.step3Intro':
    "Choisissez l'apparence de votre message de statut et si PresenceJam doit se lancer à l'ouverture de session.",
  'onboarding.statusTemplate': 'Modèle de statut',
  'onboarding.placeholdersHint':
    'Paramètres : {artist}, {track}, {album}, {emoji}, {device}, {playlist} (ou {context}), {progress}, {shuffle}, {repeat}',
  'onboarding.pollInterval':
    'Fréquence de vérification de Spotify : {seconds}s',
  'onboarding.settingUp': 'Configuration…',
  'onboarding.finishSetup': "Terminer la configuration",

  // ── validation / errors ───────────────────────────────────────────
  'validation.clientIdRequired': "L'ID client Spotify est requis.",
  'validation.clientIdFormat':
    "L'ID client Spotify doit comporter exactement 32 caractères hexadécimaux.",
  'validation.clientSecretRequired':
    'Le secret client Spotify est requis.',
  'validation.clientSecretTooShort':
    'Ce secret client semble trop court — il devrait comporter au moins 32 caractères. Vérifiez qu’il n’y a pas d’erreur de copier-coller.',
  'validation.noCodeInUrl':
    'Cette URL ne contient aucun code de connexion — collez l’adresse complète de la barre d’adresse de votre navigateur après la redirection Spotify.',
  'validation.connectBothFirst':
    "Veuillez connecter Spotify et Teams avant de terminer la configuration.",
  'validation.setupFailed': 'Échec de la configuration : {error}',

  // ── routes / chrome ───────────────────────────────────────────────
  'routes.skipToMainContent': 'Aller au contenu principal',
  'routes.unknownPane': 'Volet inconnu : {pane}',

  // ── feat/45-features: status rules (#432) + support snapshot (#434) ──
  'rules.sectionTitle': 'Règles de statut',
  'rules.sectionHint':
    "Les heures silencieuses et les règles de piste suppriment le statut Teams via le même chemin de contrôle de présence — une règle expirée publie automatiquement en cours de piste.",
  'rules.quietHoursLabel': 'Heures silencieuses',
  'rules.noQuietHours': 'Aucune heure silencieuse définie — le statut se synchronise à toute heure.',
  'rules.quietStart': 'Début des heures silencieuses',
  'rules.quietEnd': 'Fin des heures silencieuses',
  'rules.quietDays': 'Jours actifs (aucun = tous les jours)',
  'rules.day1': 'Lun',
  'rules.day2': 'Mar',
  'rules.day3': 'Mer',
  'rules.day4': 'Jeu',
  'rules.day5': 'Ven',
  'rules.day6': 'Sam',
  'rules.day7': 'Dim',
  'rules.addQuietHours': 'Ajouter des heures silencieuses',
  'rules.trackRulesLabel': 'Règles de piste',
  'rules.noTrackRules': 'Aucune règle de piste définie — toutes les pistes se synchronisent normalement.',
  'rules.artistPlaceholder': 'Artiste contient…',
  'rules.trackPlaceholder': 'Titre contient…',
  'rules.replacementPlaceholder': 'Publier ceci à la place (vide = supprimer)',
  'rules.addTrackRule': 'Ajouter une règle de piste',
  'rules.removeRule': 'Supprimer',
  'rules.ruleEnabled': 'Activé',
  'logs.copySnapshot': 'Copier le snapshot',
  'logs.snapshotCopied': 'Snapshot censuré copié dans le presse-papiers.',
  'logs.snapshotCopyFailed': 'Impossible de copier le snapshot.',
  // 4.6 additions
  'dashboard.availabilityListening': 'Écoute en cours (disponible)',
  'dashboard.availabilityCleared': 'Disponibilité réinitialisée',
  'settings.saveAndLeave': 'Enregistrer et quitter',
  'settings.discardChanges': 'Abandonner les modifications',
  'settings.stayHere': 'Rester ici',
  'settings.notificationsDenied':
    'Les notifications sont bloquées par le système. Autorisez-les dans les réglages du système, puis réactivez cette option.',
  'diagnostics.expiryBuffer': "Marge avant expiration du jeton",
  'diagnostics.teamsRefreshTokenPresent': 'Jeton de rafraîchissement Teams enregistré',
  'reconnect.restartSignIn': 'Redémarrer la connexion',
  'update.stagingProgress': 'Préparation de la mise à jour — {percent} %',
  'update.cancelStage': 'Annuler',
  'onboarding.submitting': 'Envoi…',
  'settings.episodeFormatHint':
    'Les podcasts et livres audio utilisent leur propre modèle — 🎙️ {show} - {episode} — votre modèle musical ci-dessus ne leur est pas appliqué.',
  'reconnect.keychainUnavailableBadge':
    'Trousseau indisponible',
  'reconnect.keychainUnavailableHint':
    'PresenceJam n’a pas pu lire votre secret client Spotify enregistré : le trousseau du système est verrouillé ou absent. Déverrouillez-le (ou installez un fournisseur Secret Service comme gnome-keyring), puis réessayez — le secret est toujours stocké, vous n’avez donc pas à reconfigurer Spotify.',
  'settings.secretKeychainUnavailable':
    'Trousseau du système indisponible — il est peut-être verrouillé ou absent. Déverrouillez-le (ou installez un fournisseur Secret Service) pour utiliser votre secret enregistré ; il est toujours stocké.',
  'diagnostics.quarantineTitle': 'Les réglages ont été réinitialisés',
  'diagnostics.quarantineBodyNow':
    "PresenceJam n'a pas pu lire votre fichier de réglages et a donc réinitialisé tous les réglages à leur valeur par défaut.",
  'diagnostics.quarantineBodyEarlier':
    "Un lancement précédent n'a pas pu lire votre fichier de réglages et l'a réinitialisé à ses valeurs par défaut.",
  'diagnostics.quarantineBackupPresent':
    "L'original illisible a été conservé à côté de votre fichier de réglages sous le nom {name}, afin que les valeurs qu'il contenait puissent être récupérées.",
  'diagnostics.quarantineBackupMissing':
    "L'original illisible se trouve toujours à côté de votre fichier de réglages sous le nom config.json.",
  'diagnostics.quarantineWhere':
    "Les deux fichiers se trouvent dans le dossier PresenceJam de votre dossier de configuration utilisateur — la sauvegarde à côté de config.json.",
  // 4.6 additions (presence rules #634/#635/#636/#637 + #538 consumption sites)
  'rules.replacementClampHint':
    'Le statut de remplacement est limité à {max} caractères — {kept} seront publiés.',
  'rules.quietReplacementPlaceholder': 'Publier ceci à la place (vide = masquer)',
  'rules.presenceLabel': 'Présence pendant cette règle',
  'rules.presenceNone': 'Ne pas modifier ma présence',
  'rules.presenceHint':
    "Une règle peut définir la disponibilité/l'activité Teams, mais uniquement si « Synchronisation de la disponibilité » est activée ; elle ne prime jamais sur un appel, une réunion ou un statut que vous avez saisi.",
  'settings.respectManualStatusLabel': 'Ne jamais écraser un statut saisi manuellement',
  'settings.respectManualStatusHint':
    "Réutilise la lecture de présence déjà effectuée par le filtre, donc sans requête supplémentaire — le texte est respecté jusqu'à ce que vous le changiez ou qu'il expire.",
  'settings.gateOutOfOfficeLabel': 'Mettre en pause quand je suis absent',
  'settings.gateOutOfOfficeHint':
    "Ignore la mise à jour du statut tant que votre absence Teams est active. Une règle de piste avec sa propre action de présence est prioritaire.",
  'settings.extraWordsLabel': 'Mots à filtrer personnalisés',
  'settings.extraWordsHint':
    'Un mot ou une expression par ligne. Les mêmes limites de mot que la liste intégrée s’appliquent.',
  'settings.extraWordsPlaceholder': 'mot ou expression',
  'settings.extraWordsClampHint':
    'Seules les {max} premières entrées de {chars} caractères sont conservées — {kept} seront filtrées.',
  'settings.pauseBackoffMaxLabel': 'Plafond du backoff en pause (secondes)',
  'settings.pauseBackoffClampHint':
    'Plage autorisée : {min}–{max} secondes ; {effective} sera utilisé.',
  'dashboard.presenceGatedQuietHours': 'Statut en pause — heures calmes actives',
  'dashboard.presenceGatedTrackRule': 'Statut en pause — une règle de piste correspond',
  'dashboard.presenceGatedManualStatus':
    'Statut en pause — vous avez saisi un statut manuellement',
  'dashboard.presenceGatedOutOfOffice': 'Statut en pause — vous êtes absent',
  // 4.7.0 — S6 (tray localization)
  'settings.languageHint':
    "S'applique aussi au menu de la zone de notification et au menu natif de l'application.",

  // 4.7.0 — S3 tray/logs hygiene
  'settings.teamsPersistWarning':
    'Connecté, mais cet appareil n’a pas pu enregistrer la session — elle fonctionne jusqu’à la fermeture. Reconnectez Teams pour réessayer l’enregistrement.',

  // 4.7.0 — S4 (rules engine)
  'rules.quietWindowHint':
    "Les heures calmes passent minuit — 22:00–07:00 court toute la nuit. Une heure de fin à 00:00 signifie minuit (la fin de la journée), et un début égal à la fin ne correspond jamais.",
  'rules.pausePollingLabel': "Suspendre l'interrogation pendant cette plage",
  'rules.pausePollingHint':
    "Pendant cette plage, Spotify n'est pas interrogé du tout — aucun statut écrit, aucun appel Teams. L'interrogation reprend d'elle-même à la fin de la plage.",
  'rules.trackRulesOrderHint':
    "Les règles sont évaluées de haut en bas — la première qui correspond l'emporte. Sans jour sélectionné, une règle s'applique tous les jours ; une heure de fin à 00:00 signifie la fin de la journée, et un début égal à la fin ne correspond jamais.",
  'rules.ruleStart': 'Début de la plage de la règle',
  'rules.ruleEnd': 'Fin de la plage de la règle',
  'rules.ruleDays': 'Jours actifs de cette règle (aucun sélectionné = tous les jours)',
  'rules.moveRuleUp': 'Déplacer la règle {n} vers le haut',
  'rules.moveRuleDown': 'Déplacer la règle {n} vers le bas',
  'rules.manualStatusLabel': "Texte du statut en pause et à l'arrêt",
  'rules.manualStatusHint':
    "Texte publié comme statut Teams lorsque la lecture est en pause ou qu'il n'y a rien à jouer. L'émoji de musique est ajouté automatiquement ; vider le champ rétablit la valeur par défaut.",
  'rules.pausedStatusPlaceholder': 'En pause',
  'rules.stoppedStatusPlaceholder': 'Rien ne joue sur Spotify',

  // 4.7.0 — S5 (rotation des journaux + export/import des réglages)
  'settings.sectionLogging': 'Journalisation',
  'settings.loggingEnabledLabel': 'Écrire un fichier journal',
  'settings.logLevelLabel': 'Niveau de journalisation',
  'settings.logMaxSizeLabel': 'Taille maximale du fichier journal (Mo)',
  'settings.logKeepFilesLabel': 'Fichiers journaux archivés à conserver',
  'settings.logRotationHint':
    'La taille limite et le nombre de fichiers archivés s’appliquent au prochain démarrage de PresenceJam. Le fichier en cours d’écriture n’en fait pas partie : le dossier des journaux contient au plus un fichier de plus que le nombre indiqué ici. Désactiver la journalisation ou changer de niveau prend effet immédiatement.',
  'settings.sectionBackup': 'Sauvegarde',
  'settings.backupHint':
    'L’export écrit une copie de ces réglages que vous pouvez conserver ou transférer sur une autre machine. Votre secret client Spotify reste dans le trousseau du système et n’est jamais inclus — et un fichier qui en contient un est refusé à l’import.',
  'settings.backupExport': 'Exporter les réglages…',
  'settings.backupImport': 'Importer les réglages…',
  'settings.backupExportDialogTitle': 'Exporter les réglages PresenceJam',
  'settings.backupImportDialogTitle': 'Importer les réglages PresenceJam',
  'settings.backupConfirmOverwrite':
    'L’import remplace tous vos réglages actuels. Le fichier actuel est conservé à côté sous le nom config.json.bak. Continuer ?',
  'settings.backupExported': 'Réglages exportés vers {path}',
  'settings.backupImported': 'Réglages importés depuis {path}',
  'settings.backupError': 'L’action de sauvegarde n’a pas pu aboutir : {error}',

  // 4.7.0 — S9 (issue #677 : la pause depuis la barre de menus)
  'dashboard.snoozeChip': 'En pause — {remaining} restantes (jusqu’à {time})',
  'dashboard.snoozeResume': 'Reprendre maintenant',
  'dashboard.snoozeResuming': 'Reprise…',
  'dashboard.snoozeResumeFailed':
    'Impossible de reprendre la synchronisation. La pause reste enregistrée — réessayez.',

  // 4.7.0 — S7 notifications (#675)
  'settings.notificationsTrackChange': 'Me notifier quand le titre change',
  'settings.notificationsSyncStopped':
    'Me notifier quand la synchronisation s’arrête d’elle-même',
  'settings.notificationsAuthRequired':
    'Me notifier quand je dois me reconnecter à Teams',
  'settings.notificationsUpdateStaged':
    'Me notifier quand une mise à jour s’installera à la fermeture',
  'notifications.syncStoppedTitle': 'PresenceJam a arrêté la synchronisation',
  'notifications.syncStoppedBody':
    'La synchronisation du statut s’est arrêtée d’elle-même. Ouvrez PresenceJam pour la relancer.',
  'notifications.authRequiredTitle': 'Connexion Teams requise',
  'notifications.authRequiredBody':
    'Votre session Teams a expiré. Reconnectez-vous pour que votre statut reste synchronisé.',
  'notifications.updateStagedTitle': 'Mise à jour prête',
  'notifications.updateStagedBody': 'PresenceJam {version} sera installé à la fermeture.',
  // 4.7.0 — update channel (#678)
  'settings.sectionUpdates': 'Mises à jour',
  'settings.updateChannelLabel': 'Canal de publication',
  'settings.updateChannelStable': 'Stable',
  'settings.updateChannelBeta': 'Bêta',
  'settings.updateChannelHint':
    "Aucune version bêta n'est encore publiée : le canal Bêta retombe donc sur la version stable — ce repli est enregistré à chaque vérification. Les versions bêta s'installent uniquement à la fermeture.",
  'update.betaOnQuitOnly':
    "Canal Bêta : les mises à jour s'installent à la fermeture — il n'y a pas de téléchargement suivi d'un redémarrage sur ce canal.",
  // 4.7.0 — S8 global hotkeys
  'settings.sectionShortcuts': 'Raccourcis globaux',
  'settings.shortcutsHint':
    'Ils fonctionnent même lorsque la fenêtre est masquée. Cliquez dans un champ et appuyez sur la combinaison voulue — le champ enregistre ce que vous appuyez, pas ce que vous tapez.',
  'settings.shortcutTogglePlayback': 'Basculer la lecture',
  'settings.shortcutToggleSync': 'Suspendre ou reprendre la synchronisation',
  'settings.shortcutUnbound': 'Non défini — cliquez et appuyez sur une combinaison',
  'settings.shortcutClear': 'Effacer',
  'settings.shortcutRegistered': 'Actif',
  'settings.shortcutNotRegistered': 'Non enregistré sur ce bureau',
  'settings.shortcutCaptureReleased':
    'Libéré pendant l’enregistrement — le raccourci actuel se déclencherait à la place',
  'settings.shortcutRejected': 'Inutilisable : {reason}',
  'settings.shortcutRegistrationFailed':
    'Échec de l’enregistrement sur ce bureau : {reason}',
  // 4.7.0 — S12 hygiène (thème/densité)
  'settings.themeSystem': 'Système',
  'settings.themeHint':
    "« Système » suit l'apparence de votre système d'exploitation ; Sombre et Clair restent fixes.",
  'settings.densityCompactLabel': 'Espacement compact',
  'settings.densityHint': "Resserre les espacements et l'échelle typographique. Indépendant du thème.",
};
