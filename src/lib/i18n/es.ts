/**
 * C6 i18n foundation (docs/scope-3.3.md §C6) — Spanish dictionary.
 * `Typed against Dict`, so every English key must be present.
 *
 * Model-written Spanish translation (issue #984) — human review pending before this copy is considered final.
 */

import type { Dict } from './en';

export const es: Dict = {
  // ── common ────────────────────────────────────────────────────────
  'common.back': 'Atrás',
  'common.backToDashboard': 'Volver al panel',
  'common.checkNow': 'Comprobar el estado de inicio de sesión',
  'common.connected': 'Conectado',
  'common.dismiss': 'Descartar',
  'common.openSignInPage': 'Abra la página de inicio de sesión de Microsoft:',
  'common.enterCodeWhenAsked': 'Introduzca este código cuando se le pida:',
  'common.moreActions': 'Más acciones',
  'common.launchAtLogin': 'Iniciar al iniciar sesión',
  'common.loading': 'Cargando…',
  'common.notConnected': 'Sin conexión',
  'common.reconnecting': 'Reconectando…',
  'common.reconnect': 'Reconectar',
  'common.retry': 'Reintentar',
  'common.bootFailed': 'No se pudo cargar el estado de la aplicación',
  'common.resetToDefault': 'Restablecer el valor predeterminado',
  'common.themeToggle': 'Cambiar el tema',
  'common.waiting': 'En espera…',
  'common.waitingForSignIn': 'En espera del inicio de sesión…',
  'common.codeExpiresIn': 'El código caduca en {time}',
  'common.codeExpired': 'Este código ha caducado — ya no se puede utilizar.',
  'common.getNewCode': 'Obtener un código nuevo',
  'common.yes': 'Sí',
  'common.no': 'No',
  'common.tagline': 'Spotify → Estado de Teams',

  // ── dashboard ─────────────────────────────────────────────────────
  'dashboard.spotifyOff': 'Spotify desactivado',
  'dashboard.teamsOff': 'Teams desactivado',
  'dashboard.syncing': 'Sincronizando',
  'dashboard.logsDetachedTitle': 'Registros (separados — haga clic para enfocar)',
  'dashboard.logsTitle': 'Registros',
  'dashboard.logsDetachedAria': 'Registros (separados en otra ventana)',
  'dashboard.openLogsAria': 'Abrir los registros',
  'dashboard.diagnostics': 'Diagnóstico',
  'dashboard.openDiagnosticsAria': 'Abrir el diagnóstico',
  'dashboard.settingsDetachedTitle': 'Ajustes (separados — haga clic para enfocar)',
  'dashboard.settings': 'Ajustes',
  'dashboard.settingsDetachedAria': 'Ajustes (separados en otra ventana)',
  'dashboard.openSettingsAria': 'Abrir los ajustes',
  'dashboard.about': 'Acerca de',
  'dashboard.aboutAria': 'Acerca de PresenceJam',
  'dashboard.pauseSync': 'Pausar la sincronización',
  'dashboard.resumeSync': 'Reanudar la sincronización',
  'dashboard.presenceGated': 'Estado en pausa — está ocupado, en una llamada o presentando',
  'dashboard.setupRequired': 'Configuración necesaria',
  'dashboard.setupHint':
    'Conecte Spotify y Microsoft Teams para que sus canciones en reproducción actualicen su estado de Teams.',
  'dashboard.continueSetup': 'Continuar la configuración',
  'dashboard.playing': 'En reproducción',
  'dashboard.paused': 'En pausa',
  'dashboard.liveStreamAria': 'Transmisión en directo — posición desconocida',
  'dashboard.yourTeamsStatus': 'Su estado de Teams',
  'dashboard.nothingPlaying': 'Nada en reproducción',
  'dashboard.nothingPlayingHint':
    'Reproduzca algo en Spotify y aparecerá en su estado de Teams.',
  'dashboard.syncCrashed': 'La sincronización se detuvo inesperadamente. Pulse reanudar (▶) para reiniciarla.',
  'dashboard.credentialCheckFailed': 'No se pudieron comprobar sus credenciales. Compruebe su conexión e inténtelo de nuevo.',
  'dashboard.syncToggleFailed': 'No se pudo iniciar ni detener la sincronización. Inténtelo de nuevo; si persiste, abra Diagnóstico desde el encabezado del panel.',
  'dashboard.statusNotConfigured': 'Sin configurar',
  'dashboard.statusNoTrack': 'Ninguna canción en reproducción',
  'dashboard.live': 'En directo',
  'dashboard.refreshStatus': 'Actualizar el estado',
  'dashboard.refreshing': 'Actualizando…',
  'dashboard.refreshFailed': 'No se pudo actualizar el estado. Inténtelo de nuevo.',
  'dashboard.refreshAria': 'Actualizar el estado de Teams ahora',

  // ── logs ──────────────────────────────────────────────────────────
  'logs.title': 'Registros',
  'logs.filterAria': 'Filtro de nivel de registro',
  'logs.level.all': 'Todos',
  'logs.level.trace': 'Traza',
  'logs.level.debug': 'Depuración',
  'logs.level.info': 'Información',
  'logs.level.warning': 'Advertencia',
  'logs.level.error': 'Error',
  'logs.count_one': '{count} entrada',
  'logs.count_other': '{count} entradas',
  'logs.showingOf': 'Mostrando {shown} de {total}',
  'logs.jumpToLatest': 'Ir al más reciente',
  'logs.popOut': 'Separar',
  'logs.clear': 'Borrar',
  'logs.openFolder': 'Abrir la carpeta',
  'logs.empty': 'Aún no hay entradas de registro',
  'logs.emptyHint': 'Las entradas en directo aparecen aquí cuando se inicia la sincronización y Spotify está en reproducción.',

  // ── settings ──────────────────────────────────────────────────────
  'settings.title': 'Ajustes',
  'settings.popBackIn': 'Volver a acoplar',
  'settings.popOutActionTitle': 'Separar en su propia ventana',
  'settings.unsavedChanges': 'Cambios sin guardar',
  'settings.sectionSpotify': 'Spotify',
  'settings.sectionTeams': 'Microsoft Teams',
  'settings.sectionPresence': 'Presencia',
  'settings.sectionStatusFormat': 'Formato del estado',
  'settings.sectionPolling': 'Frecuencia de sincronización',
  'settings.sectionNotifications': 'Notificaciones',
  'settings.sectionAppearance': 'Apariencia',
  'settings.clientId': 'ID de cliente',
  'settings.clientIdPlaceholder': 'Introduzca el ID de cliente de Spotify',
  'settings.clientSecret': 'Secreto de cliente',
  'settings.secretStoredHint':
    'Guardado de forma segura en el llavero de su sistema operativo. Para reemplazarlo, vuelva al panel y seleccione Continuar la configuración.',
  'settings.secretNotConfigured': 'Sin configurar.',
  'settings.runOnboarding': 'Ejecutar el asistente',
  'settings.toSetUpSpotify': 'para configurar Spotify.',
  'settings.reconnectSpotify': 'Reconectar Spotify',
  'settings.completeAuthInBrowser': 'Complete la autenticación en el navegador.',
  'settings.playbackScopeBanner': 'Spotify añadió controles de reproducción. Pulse Reconectar junto a este mensaje para activarlos.',
  'settings.spotifySecretConflict':
    'El secreto de cliente de su archivo de configuración difiere del guardado en el llavero. Reconecte Spotify para resolverlo.',
  'settings.teamsAuthHint':
    'La autenticación de Teams utiliza su cuenta de Microsoft 365. No se necesita configuración adicional.',
  'settings.presenceScopeBanner': 'Teams añadió la detección de reuniones y llamadas. Pulse Reconectar junto a este mensaje para activarla.',
  'settings.availabilitySyncLabel': 'Mostrar Disponible mientras escucha',
  'settings.availabilitySyncHint':
    'Desactivado de forma predeterminada. Al activarlo, Teams le muestra como Disponible (en lugar de Ocupado) mientras suena música. Nota: Teams sigue mostrando Ocupado durante las llamadas y reuniones.',
  'settings.presenceGateLabel': 'Pausar el estado durante reuniones, llamadas o No molestar',
  'settings.presenceGateHint':
    'Activado de forma predeterminada. Omite la escritura de su estado de Spotify mientras Teams indica que está ocupado, en una reunión, en una llamada o presentando.',
  'settings.formatTemplate': 'Plantilla de formato',
  'settings.formatTemplatePlaceholder': '🎵 {artist} - {track} 🎧',
  'settings.livePreview': 'Vista previa en directo',
  'settings.placeholdersHint':
    'Marcadores disponibles: {artist}, {track}, {album}, {emoji}, {device}, {playlist} (o {context}), {progress}, {shuffle}, {repeat}. Shuffle y Repeat muestran 🔀/🔁 solo cuando están activados.',
  'settings.profanityFilterLabel': 'Filtrar blasfemias en el estado',
  'settings.placeholderTextLabel': 'Texto provisional',
  'settings.placeholderTextHint':
    'Utilice {emoji} para el estado de reproducción (🎵 en reproducción / ⏸ en pausa). Se muestra cuando se detectan blasfemias en la información de la pista.',
  'settings.placeholderTextPlaceholder': 'Escuchando Spotify ahora mismo',
  'settings.profaneSampleToggle': 'Vista previa con una pista de muestra con blasfemias',
  'settings.defaultIntervalLabel': 'Intervalo predeterminado: {seconds}s',
  'settings.minIntervalLabel': 'Intervalo mín. (s)',
  'settings.maxIntervalLabel': 'Intervalo máx. (s)',
  'settings.clampHint':
    'El intervalo mínimo supera al máximo — se guardará el máximo como {max}s.',
  'settings.notificationsHint':
    'Cada clase que active muestra una notificación del sistema — la primera puede pedir permiso a su sistema operativo.',
  'settings.themeLabel': 'Tema',
  'settings.themeDark': 'Oscuro',
  'settings.themeLight': 'Claro',
  'settings.languageLabel': 'Idioma',
  'settings.saveChanges': 'Guardar los cambios',
  'settings.saving': 'Guardando…',
  'settings.saved': 'Ajustes guardados.',
  'settings.failedToSave': 'No se pudieron guardar los ajustes — sus cambios siguen aquí. Inténtelo de nuevo.',
  'settings.openLogsFolder': 'Abrir la carpeta de registros',
  'settings.previewUnavailable': '(vista previa no disponible)',

  // ── diagnostics ───────────────────────────────────────────────────
  'diagnostics.title': 'Diagnóstico',
  'diagnostics.localOnlyHint': 'Instantánea solo local — puede adjuntarla a un informe de error.',
  'diagnostics.copy': 'Copiar el diagnóstico',
  'diagnostics.saveToFile': 'Guardar en un archivo',
  'diagnostics.copied': 'Diagnóstico copiado al portapapeles.',
  'diagnostics.copyFailed': 'No se pudo copiar — utilice «Guardar en un archivo» en su lugar.',
  'diagnostics.savedToDownloads': 'Diagnóstico guardado en su carpeta de descargas.',
  'diagnostics.saveFailed': 'No se pudo guardar — utilice «Copiar el diagnóstico» en su lugar.',
  'diagnostics.collecting': 'Recopilando el diagnóstico…',
  'diagnostics.collectFailed': 'No se pudo recopilar el diagnóstico',
  'diagnostics.versions': 'Versiones',
  'diagnostics.configuration': 'Configuración',
  'diagnostics.connections': 'Conexiones',
  'diagnostics.recentLogLines': 'Líneas de registro recientes',
  'diagnostics.app': 'PresenceJam',
  'diagnostics.tauri': 'Tauri',
  'diagnostics.os': 'SO',
  'diagnostics.osRelease': 'Versión',
  'diagnostics.installFlavor': 'Tipo de instalación',
  'diagnostics.unknown': 'desconocido',
  'diagnostics.spotifyClientId': 'ID de cliente de Spotify',
  'diagnostics.redirectUri': 'URI de redirección',
  'diagnostics.notSet': '(sin definir)',
  'diagnostics.clientSecretKeychain': 'Secreto de cliente en el llavero',
  'diagnostics.clearOnPause': 'Borrar el estado al pausar',
  'diagnostics.profanityFilter': 'Filtro de blasfemias',
  'diagnostics.extraProfanityWords': 'Palabras adicionales para filtrar',
  'diagnostics.startMinimized': 'Iniciar minimizado',
  'diagnostics.availabilitySync': 'Sincronización de disponibilidad de Teams',
  'diagnostics.presenceGate': 'Puerta de presencia',
  'diagnostics.pollInterval': 'Intervalo de sondeo (predeterminado/mín./máx.)',
  'diagnostics.logging': 'Registro',
  'diagnostics.loggingEnabled': 'activado ({level})',
  'diagnostics.loggingDisabled': 'desactivado',
  'diagnostics.respectManualStatus': 'Respetar el estado manual',
  'diagnostics.gateOutOfOffice': 'Pausar fuera de la oficina',
  'diagnostics.locale': 'Configuración regional',
  'diagnostics.updateChannel': 'Canal de actualizaciones',
  'diagnostics.configSnoozed': 'Sincronización pospuesta',
  'diagnostics.launchAtLogin': 'Iniciar al iniciar sesión',
  'diagnostics.statusRules': 'Reglas de estado',
  'diagnostics.statusRulesValue': '{quiet}/{quietTotal} horas de silencio, {rules}/{rulesTotal} reglas de pista activadas',
  'diagnostics.spotifyConnected': 'Spotify conectado',
  'diagnostics.spotifyTokenExpires': 'El token de Spotify caduca',
  'diagnostics.teamsConnected': 'Teams conectado',
  'diagnostics.teamsTokenExpires': 'El token de Teams caduca',
  'diagnostics.expired': '(caducado)',
  'diagnostics.keychainSpotifySecret': 'Llavero: secreto de Spotify presente',
  'diagnostics.keychainEncryptionKey': 'Llavero: clave de cifrado de tokens presente',
  'diagnostics.tokensNeverIncluded':
    'Los valores de los tokens nunca se incluyen — solo marcas de caducidad e indicadores de presencia.',
  'diagnostics.noLogLinesYet': 'Aún no hay líneas de registro disponibles.',
  'diagnostics.failedInstallTitle': 'Instalación de actualización fallida',
  'diagnostics.failedInstallVersion': 'Versión',
  'diagnostics.failedInstallError': 'Error',
  'diagnostics.failedInstallTimestamp': 'Intentado el',
  'diagnostics.failedInstallDismissFailed': 'No se pudo descartar el registro de instalación fallida.',
  'diagnostics.resetTokenStorage': 'Restablecer el almacenamiento local de tokens',
  'diagnostics.resetTokenConfirm':
    'Esto elimina tokens.json y sus archivos auxiliares además de la clave de cifrado almacenada. Tendrá que iniciar sesión de nuevo. ¿Desea continuar?',
  'diagnostics.resetTokenDone': 'Almacenamiento de tokens restablecido. Inicie sesión de nuevo para reanudar la sincronización.',
  'diagnostics.resetTokenFailed': 'No se pudo restablecer — {error}',
  'diagnostics.syncRunning': 'Sincronización en curso',
  'diagnostics.syncSnoozed': 'Pospuesta',
  'diagnostics.syncSnoozeMinutes': 'quedan {minutes} min',
  'diagnostics.syncManualStatusBlocks': 'El estado manual retiene las escrituras',
  'diagnostics.syncPresenceGateReason': 'Motivo de la puerta de presencia',
  'diagnostics.syncTransientFailures': 'Fallos de autenticación consecutivos',
  'diagnostics.syncNetworkFailures': 'Fallos de red consecutivos',

  // ── reconnect ─────────────────────────────────────────────────────
  'reconnect.title': 'Reconectar',
  'reconnect.description': 'La sincronización necesita su atención. Reconéctese abajo para reanudarla.',
  'reconnect.missingCredentials': 'Faltan las credenciales',
  'reconnect.failed': 'Fallido',
  'reconnect.readyToReconnect': 'Listo para reconectar',
  'reconnect.spotifyOk': 'Spotify reconectado correctamente.',
  'reconnect.spotifyNotConfigured':
    'Las credenciales de Spotify no están configuradas en este equipo.',
  'reconnect.completeAuthInOpenedBrowser':
    'Complete la autenticación en la ventana del navegador abierta.',
  'reconnect.tryAgain': 'Inténtelo de nuevo',
  'reconnect.clickBelowSpotify': 'Pulse abajo para reconectar su cuenta de Spotify.',
  'reconnect.teamsOk': 'Teams reconectado correctamente.',
  'reconnect.clickBelowTeams':
    'Pulse abajo para reconectar su cuenta de Microsoft Teams.',
  'reconnect.tokenStorageUnusable': 'Sus datos de inicio de sesión guardados no se pueden leer — la clave de cifrado almacenada no se puede utilizar.',
  'reconnect.resetTokenStorage': 'Restablecer el almacenamiento local de tokens',
  'reconnect.resetTokenConfirm':
    'Esto elimina tokens.json y sus archivos auxiliares además de la clave de cifrado almacenada. Tendrá que iniciar sesión de nuevo. ¿Desea continuar?',
  'reconnect.resetTokenDone': 'Almacenamiento de tokens restablecido. Inicie sesión abajo para reanudar.',
  'reconnect.resetTokenFailed': 'No se pudo restablecer — {error}',
  'reconnect.missingCredsTitle': '¿Faltan las credenciales de Spotify?',
  'reconnect.reenterCredsHint':
    'Tendrá que volver a introducir su ID de cliente y su secreto de cliente.',
  'reconnect.goToFullSetup': 'Ir a la configuración completa',
  'reconnect.reconnectTeams': 'Reconectar Teams',

  // ── about ─────────────────────────────────────────────────────────
  'about.version': 'Versión {version}',
  'about.description':
    'Muestra lo que está escuchando en Spotify en su estado de Microsoft Teams — automáticamente.',
  'about.statusSync': 'Sincronización de estado',
  'about.live': 'En directo',
  'about.auth': 'Inicio de sesión',
  'about.authMethod': 'Spotify + Microsoft',
  'about.storage': 'Almacenamiento',
  'about.osKeychain': 'Llavero del SO',
  'about.githubRepo': 'Repositorio de GitHub',
  'about.releases': 'Versiones',
  'about.reportIssue': 'Informar de un problema',

  // ── update banner ─────────────────────────────────────────────────
  'update.available': 'Actualización v{version} disponible',
  'update.stagedQuit': 'La versión v{version} se instalará cuando salga de PresenceJam',
  'update.downloadFailed': 'No se pudo descargar — {error}',
  'update.downloadAndInstall': 'Descargar e instalar',
  'update.downloading': 'Descargando…',
  'update.installOnQuit': 'Instalar al salir',
  'update.preparing': 'Preparando…',
  'update.dismissAria': 'Descartar el aviso de actualización',
  'update.confirmQuitInstall':
    '¿Instalar la versión v{staged} al salir? Versión actual: v{current}.',
  'update.confirmQuitInstallUnknown': '¿Instalar la versión v{staged} al salir?',
  'update.stagedVsCurrent':
    'La versión v{staged} se instalará al salir (actual v{current})',
  'update.staleSkipped':
    'La versión v{staged} se omitió — su versión actual v{current} es más reciente.',
  'update.staleSkippedUnknown':
    'La versión v{staged} se omitió — no es más reciente que su versión actual.',
  'update.installAnyway': 'Instalar de todos modos',
  // #894: a `.deb` / `.rpm` install is updated by its package manager, not by
  // the in-app updater — the release manifest only offers the AppImage payload,
  // which neither installer can apply. The banner shows the command instead of
  // an install button.
  'update.packageManagedDeb':
    'La versión v{version} está disponible. Esta copia se instaló como paquete .deb, por lo que las actualizaciones llegan desde apt.',
  'update.packageManagedRpm':
    'La versión v{version} está disponible. Esta copia se instaló como paquete .rpm, por lo que las actualizaciones llegan desde dnf.',
  'update.packageManagerInstallDeb': 'sudo apt install ./{file}',
  'update.packageManagerInstallRpm': 'sudo dnf install ./{file}',

  // ── onboarding ────────────────────────────────────────────────────
  'onboarding.stepOf': 'Paso {step} de 3',
  'onboarding.step1Title': 'Conectar Spotify',
  'onboarding.step1Intro':
    'Pegue su ID de cliente y su secreto de cliente de Spotify abajo y seleccione Conectar Spotify — abriremos la página de inicio de sesión de Spotify.',
  'onboarding.getCredentials': 'Obtenga sus credenciales de Spotify',
  'onboarding.instruction1':
    'Abra el panel para desarrolladores de Spotify y cree una aplicación.',
  'onboarding.instruction2': 'En URI de redirección, añada presencejam://callback (esto indica a Spotify adónde devolverle).',
  'onboarding.instruction3':
    'Copie el ID de cliente y el secreto de cliente desde los ajustes de la aplicación.',
  'onboarding.clientIdPlaceholder': 'ID de cliente de Spotify de 32 caracteres',
  'onboarding.clientSecretPlaceholder': 'Secreto de cliente de Spotify',
  'onboarding.connectSpotify': 'Conectar Spotify',
  'onboarding.signInWaiting': 'El inicio de sesión de Spotify está en espera…',
  'onboarding.manualUrlHint':
    'Termine de iniciar sesión con Spotify en su navegador y pegue abajo la dirección completa de la barra de direcciones.',
  'onboarding.manualUrlLabel': 'URL de redirección de Spotify',
  'onboarding.manualUrlPlaceholder': 'presencejam://callback?code=…',
  'onboarding.submitCode': 'Enviar el código',
  'onboarding.connectedToSpotify': 'Conectado a Spotify',
  'onboarding.continue': 'Continuar →',
  'onboarding.step2Title': 'Conectar Microsoft Teams',
  'onboarding.step2Intro':
    'Utilizamos el flujo de código de dispositivo de Microsoft — un código único que introduce en una página de Microsoft. No se necesita configuración adicional.',
  'onboarding.startMicrosoftSignIn': 'Conectar Microsoft Teams',
  'onboarding.connectedToTeams': 'Conectado a Microsoft Teams',
  'onboarding.step3Title': 'Toques finales',
  'onboarding.step3Intro':
    'Elija el aspecto de su mensaje de estado y si PresenceJam debe iniciarse al iniciar sesión.',
  'onboarding.statusTemplate': 'Plantilla de estado',
  'onboarding.placeholdersHint':
    'Marcadores: {artist}, {track}, {album}, {emoji}, {device}, {playlist} (o {context}), {progress}, {shuffle}, {repeat}',
  'onboarding.pollInterval': 'Frecuencia de consulta a Spotify: {seconds}s',
  'onboarding.settingUp': 'Configurando…',
  'onboarding.finishSetup': 'Terminar la configuración',

  // ── validation / errors ───────────────────────────────────────────
  'validation.clientIdRequired': 'El ID de cliente de Spotify es obligatorio.',
  'validation.clientIdFormat':
    'El ID de cliente de Spotify debe tener exactamente 32 caracteres hexadecimales.',
  'validation.clientSecretRequired': 'El secreto de cliente de Spotify es obligatorio.',
  'validation.clientSecretTooShort':
    'Ese secreto de cliente parece demasiado corto — debería tener al menos 32 caracteres. Compruebe si hubo un error al copiar.',
  'validation.noCodeInUrl':
    'Esa URL no contiene ningún código de inicio de sesión — pegue la dirección completa de la barra de direcciones de su navegador después de que Spotify le redirija.',
  'validation.connectBothFirst':
    'Conecte Spotify y Teams antes de terminar la configuración.',
  'validation.setupFailed': 'No se pudo completar la configuración: {error}',

  // ── routes / chrome ───────────────────────────────────────────────
  'routes.skipToMainContent': 'Saltar al contenido principal',
  'routes.unknownPane': 'Panel desconocido: {pane}',

  // ── feat/45-features: status rules (#432) + support snapshot (#434) ──
  'rules.sectionTitle': 'Reglas de estado',
  'rules.sectionHint':
    'Las horas de silencio y las reglas de pista suprimen la escritura del estado de Teams, reutilizando la misma vía de puerta de presencia — una regla borrada publica automáticamente a mitad de la pista.',
  'rules.quietHoursLabel': 'Horas de silencio',
  'rules.noQuietHours': 'No hay horas de silencio definidas — el estado se sincroniza a todas horas.',
  'rules.quietStart': 'Inicio de las horas de silencio',
  'rules.quietEnd': 'Fin de las horas de silencio',
  'rules.quietDays': 'Días activos (ninguno seleccionado = todos los días)',
  'rules.dayEveryDay': 'Todos los días',
  'rules.day1': 'lun',
  'rules.day2': 'mar',
  'rules.day3': 'mié',
  'rules.day4': 'jue',
  'rules.day5': 'vie',
  'rules.day6': 'sáb',
  'rules.day7': 'dom',
  'rules.addQuietHours': 'Añadir horas de silencio',
  'rules.trackRulesLabel': 'Reglas de pista',
  'rules.noTrackRules': 'No hay reglas de pista definidas — todas las pistas se sincronizan normalmente.',
  'rules.artistPlaceholder': 'El artista contiene…',
  'rules.trackPlaceholder': 'El título contiene…',
  'rules.replacementPlaceholder': 'Publicar esto en su lugar (vacío = suprimir)',
  'rules.addTrackRule': 'Añadir regla de pista',
  'rules.removeRule': 'Quitar',
  'rules.ruleEnabled': 'Activada',
  'logs.copySnapshot': 'Copiar la instantánea',
  'logs.snapshotCopied': 'Instantánea depurada copiada al portapapeles.',
  'logs.snapshotCopyFailed': 'No se pudo copiar la instantánea.',
  'logs.openFolderError':
    'No se pudo abrir la carpeta de registros. Es posible que aún no exista — intente reiniciar la aplicación para crearla.',
  // 4.6 additions
  'dashboard.availabilityListening': 'Escuchando (Disponible)',
  'dashboard.availabilityCleared': 'Disponibilidad borrada',
  'settings.saveAndLeave': 'Guardar y salir',
  'settings.discardChanges': 'Descartar los cambios',
  'settings.stayHere': 'Quedarse aquí',
  'settings.notificationsDenied':
    'Las notificaciones están bloqueadas por el sistema. Permítalas en los ajustes del sistema y vuelva a activar esto.',
  'diagnostics.expiryBuffer': 'Margen de actualización del token',
  'diagnostics.teamsRefreshTokenPresent': 'Token de actualización de Teams guardado',
  'reconnect.restartSignIn': 'Reiniciar el inicio de sesión',
  'update.stagingProgress': 'Preparando la actualización — {percent}%',
  'update.cancelStage': 'Cancelar',
  'onboarding.submitting': 'Enviando…',
  'settings.episodeFormatHint':
    'Los pódcasts y audiolibros usan su propia plantilla — 🎙️ {show} - {episode} — por lo que la plantilla de música anterior no se aplica a ellos.',
  'reconnect.keychainUnavailableBadge':
    'Llavero no disponible',
  'reconnect.keychainUnavailableHint':
    'PresenceJam no pudo leer su secreto de cliente de Spotify guardado: el llavero del sistema está bloqueado o falta. Desbloquéelo (o instale un proveedor de Secret Service como gnome-keyring) e inténtelo de nuevo — su secreto sigue guardado, así que no necesita configurar Spotify otra vez.',
  'settings.secretKeychainUnavailable':
    'Llavero del sistema no disponible — puede estar bloqueado o faltar. Desbloquéelo (o instale un proveedor de Secret Service) para usar su secreto guardado; sigue guardado.',
  'diagnostics.quarantineTitle': 'Se restablecieron los ajustes',
  'diagnostics.quarantineBodyNow':
    'PresenceJam no pudo leer su archivo de ajustes, por lo que todos los ajustes se restablecieron a sus valores predeterminados.',
  'diagnostics.quarantineBodyEarlier':
    'Un inicio anterior no pudo leer su archivo de ajustes y lo restableció a sus valores predeterminados.',
  'diagnostics.quarantineBackupPresent':
    'El original ilegible se conservó junto a su archivo de ajustes como {name}, por lo que sus valores aún se pueden recuperar.',
  'diagnostics.quarantineBackupMissing':
    'El original ilegible sigue junto a su archivo de ajustes como config.json.',
  'diagnostics.quarantineWhere':
    'Ambos archivos están en la carpeta de PresenceJam dentro de su carpeta de configuración de usuario — la copia de seguridad está junto a config.json.',
  // 4.6 additions (presence rules #634/#635/#636/#637 + #538 consumption sites)
  'rules.replacementClampHint':
    'El estado de reemplazo está limitado a {max} caracteres; el resto no se publica.',
  'rules.presenceLabel': 'Presencia mientras se aplica esta regla',
  'rules.presenceNone': 'No cambiar mi presencia',
  'rules.presenceHint':
    'Una regla puede fijar la disponibilidad o actividad de Teams, pero solo mientras la «sincronización de disponibilidad» está activada; nunca anula una llamada, una reunión ni un estado fijado manualmente.',
  'settings.respectManualStatusLabel': 'No sobrescribir nunca un estado fijado manualmente',
  'settings.respectManualStatusHint':
    'Reutiliza la lectura de presencia que la puerta ya realiza, por lo que no cuesta ninguna solicitud adicional — su texto se respeta hasta que usted lo cambie o caduque.',
  'settings.gateOutOfOfficeLabel': 'Pausar mientras estoy fuera de la oficina',
  'settings.gateOutOfOfficeHint':
    'Omite la actualización del estado mientras su ajuste de fuera de la oficina de Teams está activado. Una regla de pista con su propia acción de presencia lo anula.',
  // Issue #872: the OS-level presentation gate (full-screen app, slide
  // deck, Windows Focus Assist Quiet Time). OFF by default, matching how
  // `availabilitySync` and `gateWhenOutOfOffice` shipped — 4.7 behaviour
  // is unchanged until the user opts in.
  'settings.gateWhenPresentingLabel': 'Pausar mientras presento',
  'settings.gateWhenPresentingHint':
    'Omite la escritura de su estado de Spotify mientras el SO informa de una aplicación a pantalla completa, una presentación o el tiempo de inactividad. Linux y macOS nunca informan esta señal, por lo que el interruptor no tiene efecto allí.',
  // Issue #873: the desktop-idle gate. `0` (the default) keeps 4.7
  // behaviour — the app keeps advertising listening until the user
  // explicitly opts in.
  'settings.idleAwayLabel': 'Pausar cuando mi escritorio está inactivo',
  'settings.idleAwayHint':
    'Deja de anunciar su estado de Spotify cuando el escritorio no ha recibido entrada de teclado o ratón durante estos segundos. 60–3600; 0 lo desactiva. Linux y macOS nunca informan esta señal.',
  'settings.extraWordsLabel': 'Palabras personalizadas para filtrar',
  'settings.extraWordsHint':
    'Una palabra o frase por línea. Se aplican con los mismos límites que la lista integrada.',
  'settings.extraWordsPlaceholder': 'palabra o frase',
  'settings.extraWordsClampHint':
    'Solo se conservan las primeras {max} entradas de {chars} caracteres — se filtrará {kept}.',
  'settings.pauseBackoffMaxLabel': 'Límite de espera en pausa (segundos)',
  'settings.pauseBackoffClampHint':
    'El intervalo permitido es de {min} a {max} segundos; se utilizará {effective}.',
  'dashboard.presenceGatedQuietHours': 'Estado en pausa — las horas de silencio están activas',
  'dashboard.presenceGatedTrackRule': 'Estado en pausa — coincidió una regla de pista',
  'dashboard.presenceGatedManualStatus':
    'Estado en pausa — usted fijó un mensaje de estado manualmente',
  'dashboard.presenceGatedOutOfOffice': 'Estado en pausa — está fuera de la oficina',
  'dashboard.presenceGatedPresenting':
    'Estado en pausa — está presentando o en una aplicación a pantalla completa',
  'dashboard.presenceGatedQuietTime': 'Estado en pausa — Focus Assist está activado',
  'dashboard.presenceGatedIdle': 'Estado en pausa — el escritorio está inactivo',
  // 4.7.0 — S6 (tray localization): `config.locale` is the single source of
  // truth, so the picker also drives the tray and the native app menu.
  'settings.languageHint':
    'También se aplica al menú de la bandeja y al menú nativo de la aplicación.',
  // #984: the follow-system-language toggle beside the picker. The mode
  // re-resolves from the OS language on every boot (and on change) and
  // persists the resolution, so the tray and the native menu follow too.
  'settings.languageFollowSystemLabel': 'Seguir el idioma del sistema',
  'settings.languageFollowSystemHint':
    'Al activarlo, el selector de idioma anterior se ignora: la aplicación vuelve a determinar el idioma desde el idioma de su sistema operativo en cada inicio. Los idiomas del sistema no compatibles siguen recurriendo al inglés.',

  // Issue #932: the banner used to be Teams-only
  // (`settings.teamsPersistWarning`, added with #562). The new
  // Spotify mirror ships the same copy with a `{provider}` placeholder
  // — the Settings card passes the display name (`Microsoft Teams` or
  // `Spotify`) so one banner covers both providers.
  'settings.authPersistWarning':
    'Sesión iniciada, pero este dispositivo no pudo guardar la sesión de {provider} — funciona hasta que salga. Reconecte {provider} para intentar guardarla de nuevo.',

  // 4.7.0 — S4 (rules engine)
  'rules.quietWindowHint':
    'Las horas de silencio cruzan la medianoche — 22:00–07:00 abarca toda la noche. Cada mitad pertenece a la noche en que comienza: solo con el lunes seleccionado, la ventana cubre la noche del lunes hasta la mañana del martes. Una hora de fin de 00:00 significa la medianoche (el fin del día), y un inicio igual al fin nunca coincide.',
  'rules.pausePollingLabel': 'Dejar de sondear durante esta ventana',
  'rules.pausePollingHint':
    'Mientras esta ventana está activa no se consulta Spotify en absoluto — ni actualización de estado ni llamada a Teams. El sondeo se reanuda solo cuando termina la ventana.',
  'rules.trackRulesOrderHint':
    'Las reglas se evalúan de arriba abajo — gana la primera coincidencia. Una regla sin días laborables se aplica todos los días, una hora de fin de 00:00 significa el fin del día y un inicio igual al fin nunca coincide. Una ventana que cruza la medianoche pertenece a la noche en que comienza: solo con el lunes seleccionado, 22:00–07:00 cubre la noche del lunes hasta la mañana del martes.',
  'rules.ruleStart': 'Inicio de la ventana de la regla',
  'rules.ruleEnd': 'Fin de la ventana de la regla',
  'rules.ruleDays': 'Días activos de esta regla (ninguno seleccionado = todos los días)',
  'rules.moveRuleUp': 'Subir la regla {n}',
  'rules.moveRuleDown': 'Bajar la regla {n}',
  'rules.manualStatusLabel': 'Pausar y detener el texto de estado',
  'rules.manualStatusHint':
    'El texto publicado como su estado de Teams mientras la reproducción está en pausa y cuando no suena nada. El emoji de música se añade por usted; borrar un campo restaura el valor predeterminado.',
  'rules.pausedStatusPlaceholder': 'En pausa',
  'rules.stoppedStatusPlaceholder': 'Nada en reproducción en Spotify',

  // 4.7.0 — S5 (log rotation + settings export/import)
  'settings.sectionLogging': 'Registro',
  'settings.loggingEnabledLabel': 'Escribir un archivo de registro',
  'settings.logLevelLabel': 'Nivel de registro',
  'settings.logMaxSizeLabel': 'Tamaño máximo del archivo de registro (MB)',
  'settings.logKeepFilesLabel': 'Archivos de registro archivados por conservar',
  'settings.logRotationHint':
    'El límite de tamaño y el número de archivos archivados se aplican la próxima vez que se inicie PresenceJam. El registro que se está escribiendo ahora no cuenta: la carpeta de registros contiene como máximo un archivo más que el número fijado. Desactivar el registro o cambiar el nivel surte efecto de inmediato.',
  'settings.sectionBackup': 'Copia de seguridad',
  'settings.backupHint':
    'La exportación escribe una copia de estos ajustes que puede conservar o trasladar a otro equipo. Su secreto de cliente de Spotify permanece en el llavero del sistema y nunca se incluye — y un archivo que contenga uno se rechaza al importar.',
  'settings.backupExport': 'Exportar los ajustes…',
  'settings.backupImport': 'Importar los ajustes…',
  'settings.backupExportDialogTitle': 'Exportar los ajustes de PresenceJam',
  'settings.backupImportDialogTitle': 'Importar los ajustes de PresenceJam',
  'settings.backupConfirmOverwrite':
    'La importación reemplaza todos sus ajustes actuales. El archivo actual se conserva junto a él como config.json.bak. ¿Desea continuar?',
  'settings.backupExported': 'Ajustes exportados a {path}',
  'settings.backupImported': 'Ajustes importados desde {path}',
  'settings.backupError': 'No se pudo completar la acción de copia de seguridad: {error}',

  // 4.7.0 — S9 (issue #677: the tray snooze / "pause sync for a while")
  'dashboard.snoozeChip': 'Pospuesto — quedan {remaining} (hasta las {time})',
  'dashboard.snoozeResume': 'Reanudar ahora',
  'dashboard.snoozeResuming': 'Reanudando…',
  'dashboard.snoozeResumeFailed':
    'No se pudo reanudar la sincronización. La posposición sigue guardada — inténtelo de nuevo.',
  // Issue #736: the chip's live region announces entry/exit once; the
  // per-second countdown is no longer in the live region.
  'dashboard.snoozeStatusEnd': 'Sincronización reanudada',

  // 4.7.0 — S7 notifications (#675): one toggle per desktop-notification
  // class, plus the copy for the three classes the always-mounted layout
  // dispatches (track changes keep dispatching from the Dashboard card).
  'settings.notificationsTrackChange': 'Avisarme cuando cambie la pista',
  'settings.notificationsSyncStopped': 'Avisarme cuando la sincronización se detenga sola',
  'settings.notificationsAuthRequired': 'Avisarme cuando tenga que iniciar sesión en Teams de nuevo',
  'settings.notificationsUpdateStaged': 'Avisarme cuando una actualización se instale al salir',
  'notifications.syncStoppedTitle': 'PresenceJam dejó de sincronizar',
  'notifications.syncStoppedBody':
    'La sincronización de estado se detuvo sola. Abra PresenceJam para reiniciarla.',
  'notifications.authRequiredTitle': 'Se requiere iniciar sesión en Teams',
  'notifications.authRequiredBody':
    'Su sesión de Teams caducó. Inicie sesión de nuevo para que su estado siga sincronizándose.',
  'notifications.updateStagedTitle': 'Actualización lista',
  'notifications.updateStagedBody': 'PresenceJam {version} se instalará cuando salga.',
  // 4.7.0 — update channel (#678)
  'settings.sectionUpdates': 'Actualizaciones',
  'settings.updateChannelLabel': 'Canal de versiones',
  'settings.updateChannelStable': 'Estable',
  'settings.updateChannelBeta': 'Beta',
  'settings.updateChannelHint':
    'Las compilaciones Beta usan la fuente Beta continua cuando está disponible; si falta o no tiene una versión más reciente, Beta recurre a la versión estable. Las compilaciones Beta solo se instalan al salir.',
  'update.betaOnQuitOnly':
    'Canal Beta: las actualizaciones se instalan al salir — no hay ruta de descarga y reinicio en Beta.',
  // 4.7.0 — S8 global hotkeys
  'settings.sectionShortcuts': 'Atajos globales',
  'settings.shortcutsHint':
    'Funcionan mientras la ventana está oculta. Haga clic en un campo y pulse la combinación que desee — el campo registra lo que pulsa, no lo que escribe.',
  'settings.shortcutTogglePlayback': 'Alternar la reproducción',
  'settings.shortcutToggleSync': 'Pausar o reanudar la sincronización',
  'settings.shortcutUnbound': 'Sin definir — haga clic y pulse una combinación',
  'settings.shortcutClear': 'Borrar',
  'settings.shortcutRegistered': 'Activo',
  'settings.shortcutNotRegistered': 'No registrado en este escritorio',
  'settings.shortcutCaptureReleased':
    'Liberado durante la grabación — el enlace actual se dispararía en lugar de grabarse',
  'settings.shortcutRejected': 'No se puede utilizar: {reason}',
  'settings.shortcutRegistrationFailed': 'No se pudo registrar en este escritorio: {reason}',
  // Issue #968: typed reason codes from the Rust validator. The Settings
  // card maps each `kind` to a dictionary entry so the rejection copy is
  // localized; `shortcutReasonUnknown` renders the backend's free-form text
  // for genuinely foreign refusals (compositor / app-owned combos).
  'settings.shortcutReasonNotAKey': '«{accelerator}» no es un atajo reconocido',
  'settings.shortcutReasonConflict':
    'Entra en conflicto con el atajo {other} — un acelerador no puede controlar ambas acciones',
  // Issue #810: a bare key would be grabbed system-wide. Function keys
  // (F1–F24) and media keys are exempt and bind without a modifier.
  'settings.shortcutReasonNeedsModifier':
    '«{accelerator}» necesita al menos un modificador — una tecla sola se capturaría en todas las aplicaciones',
  'settings.shortcutReasonAutostart':
    'Falló el inicio al iniciar sesión: {cause}',
  'settings.shortcutReasonUnknown': '{message}',
  'settings.shortcutReasonX11Unavailable':
    'Los atajos globales necesitan un servidor X11 accesible en este escritorio',
  'settings.shortcutReasonWorkerUnavailable':
    'No se pudo verificar el servicio de atajos globales; los atajos no están disponibles',
  // 4.7.0 — S12 hygiene (theme/density)
  'settings.themeSystem': 'Sistema',
  'settings.themeHint':
    '«Sistema» sigue la apariencia de su sistema operativo; Oscuro y Claro quedan fijos.',
  'settings.densityCompactLabel': 'Espaciado compacto',
  'settings.densityHint': 'Reduce el espaciado y la escala tipográfica. Independiente del tema.',
  // --- 5.0 wave1 i18n-lib ---
  // Key requests routed through this slice's dictionaries (the owning slice
  // cannot edit them).
  'onboarding.pollIntervalClamped':
    'Su intervalo guardado es de {stored}s, fuera del intervalo de este paso ({min}–{max}s) — se utilizarán {seconds}s.',
  'update.stagingProgressLabel': 'Preparando la actualización',
  'rules.presenceAvailable': 'Disponible',
  'rules.presenceBusyCall': 'Ocupado — En una llamada',
  'rules.presenceBusyConference': 'Ocupado — En una audioconferencia',
  'rules.presenceAway': 'Ausente',
  'rules.presenceDndPresenting': 'No molestar — Presentando',
  // --- 5.0 wave3 features-presence ---
  'dashboard.manualStatusTitle': 'Estado manual',
  'dashboard.manualStatusPlaceholder': 'Fije un estado que su equipo pueda ver durante un rato',
  'dashboard.manualStatusExpiryLabel': 'Caduca después de',
  'dashboard.manualStatusExpiry15': '15 minutos',
  'dashboard.manualStatusExpiry30': '30 minutos',
  'dashboard.manualStatusExpiry60': '1 hora',
  'dashboard.manualStatusExpiry120': '2 horas',
  'dashboard.manualStatusSet': 'Fijar el estado',
  'dashboard.manualStatusClear': 'Borrar el estado',
  'dashboard.manualStatusActive': 'Activo hasta las {expiry}',
  'dashboard.manualStatusActiveEmpty': 'Activo (caduca pronto)',
  'dashboard.manualStatusRecentTitle': 'Estados recientes',
  'dashboard.manualStatusRecentEmpty': 'Aún no hay estados recientes',
  'dashboard.manualStatusFiltered': 'El filtro de blasfemias reescribió el estado',
  'dashboard.activityTitle': 'Actividad',
  'dashboard.activityEmpty': 'Aún no hay decisiones — inicie la sincronización para ver lo que eligió su aplicación',
  'dashboard.volumeLabel': 'Volumen',
  'dashboard.volumeAria': 'Deslizador de volumen de Spotify',
  'dashboard.seekAria': 'Haga clic para buscar',
  'dashboard.seekUnavailableAria': 'Este dispositivo no admite la búsqueda',

  // --- 5.0 wave3 features-outlook ---
  'rules.importWorkingHours': 'Importar el horario laboral de Outlook',
  'rules.importWorkingHoursHint': 'Lee su pestaña de horario laboral y convierte cada bloque libre en una regla de horas de silencio. Revise antes de aplicar.',
  'rules.importWorkingHoursPreviewTitle': 'Vista previa del horario laboral de Outlook',
  'rules.importWorkingHoursApply': 'Aplicar estas reglas',
  'rules.importWorkingHoursReplace': 'Reemplazar las reglas de horas de silencio existentes',
  'rules.importWorkingHoursCancel': 'Cancelar',
  'rules.importWorkingHoursReplaceHint': 'Reemplaza sus reglas de horas de silencio existentes con el conjunto importado. Desmárquela para conservar ambas.',
  'rules.importWorkingHoursDaysLabel': 'Outlook indica que trabaja {days} de {start} a {end}',
  'rules.importWorkingHoursDaysAllOff': 'Outlook no indica días laborables — no hay nada que importar',

  // --- 5.0 wave3 features-gating ---

  // --- 5.0 wave3 features-profiles ---
  'rules.testTitle': 'Probar estas reglas',
  'rules.testHint':
    'Escriba una pista de muestra, elija un minuto del día y un día de la semana, y vea exactamente qué regla (si hay alguna) se dispararía.',
  'rules.testArtistLabel': 'Artista',
  'rules.testTrackLabel': 'Título de la pista',
  'rules.testAlbumLabel': 'Álbum (opcional)',
  'rules.testShowLabel': 'Programa / pódcast (opcional)',
  'rules.testDeviceLabel': 'Dispositivo de Spotify (opcional)',
  'rules.testPlaylistLabel': 'URI de lista (opcional)',
  'rules.testDurationLabel': 'Duración (mm:ss, opcional)',
  'rules.testWeekdayLabel': 'Día de la semana',
  'rules.testMinuteLabel': 'Minuto del día (HH:MM)',
  'rules.testRun': 'Ejecutar la prueba',
  'rules.testRunning': 'Ejecutando…',
  'rules.testSummaryNoMatch': 'Ninguna regla coincidiría con esta pista.',
  'rules.testSummaryRuleMatched': 'La regla {index} se dispararía: {summary}',
  'rules.testStepMatched': 'coincidió',
  'rules.testStepNotMatched': 'NO coincidió',
  'rules.testStepReason': 'motivo: {reason}',
  'rules.testStepDisabled': 'la regla está desactivada',
  'rules.testStepScheduleOutside': 'el horario no contiene este minuto del día',
  'rules.testStepNegated': 'la negación invirtió las condiciones',
  'rules.matchKindLabel': 'Estilo de coincidencia',
  'rules.matchKindSubstring': 'Subcadena (predeterminado)',
  'rules.matchKindExact': 'Coincidencia exacta',
  'rules.matchKindGlob': 'Patrón glob',
  'rules.albumSubstringLabel': 'El álbum contiene…',
  'rules.showSubstringLabel': 'El programa contiene…',
  'rules.deviceSubstringLabel': 'El dispositivo contiene…',
  'rules.playlistUriLabel': 'La URI de lista contiene…',
  'rules.minDurationLabel': 'Duración mínima (segundos)',
  'rules.negateLabel': 'Negar (coincidir cuando las condiciones NO se cumplen)',
  'rules.actionLabel': 'Acción',
  'rules.actionSuppress': 'Suprimir el estado',
  'rules.actionReplace': 'Reemplazar el estado con…',
  'rules.actionSnoozeMinutes': 'Posponer minutos…',
  'rules.actionProfile': 'Cambiar al perfil…',
  'rules.actionPresence': 'Fijar par de presencia…',
  'rules.actionReplaceStatusPlaceholder': 'Texto de estado',
  'rules.actionSnoozePlaceholder': 'Minutos',
  'rules.actionProfilePlaceholder': 'Nombre del perfil',
  'rules.actionAvailabilityPlaceholder': 'Disponibilidad',
  'rules.actionActivityPlaceholder': 'Actividad',
  'dashboard.gateWhyTitle': '¿Por qué está en pausa el estado?',
  'dashboard.gateWhyShow': 'Mostrar el motivo',
  'dashboard.gateWhyHide': 'Ocultar el motivo',
  'dashboard.gateWhyEmpty': 'Ninguna puerta activa ahora — el estado se está sincronizando normalmente.',

  'profiles.sectionTitle': 'Perfiles de presencia',
  'profiles.sectionHint':
    'Guarde una superposición con nombre (formato de estado, puertas, reglas) y cámbiela con la bandeja, una tecla de acceso rápido o la CLI.',
  'profiles.empty': 'No hay perfiles definidos.',
  'profiles.addProfile': 'Añadir perfil',
  'profiles.removeProfile': 'Quitar',
  'profiles.activeProfileLabel': 'Perfil activo',
  'profiles.activeProfileNone': 'Ninguno — usar la configuración base',
  'profiles.overlayStatusFormatLabel': 'Formato de estado',
  'profiles.overlayClearOnPauseLabel': 'Borrar el estado al pausar',
  'profiles.overlayAvailabilitySyncLabel': 'Sincronizar la disponibilidad con Teams',
  'profiles.overlayGateOutOfOfficeLabel': 'Pausar fuera de la oficina',
  'profiles.overlayGatePresentingLabel': 'Pausar con aplicaciones a pantalla completa',
  'profiles.overlayIdleAwayLabel': 'Dejar de anunciar tras inactividad (segundos)',
  'profiles.overlayPreferredPresenceLabel': 'Presencia preferida',
  'profiles.overlayRulesLabel': 'Subconjunto de reglas',
  'profiles.overlayRulesHint': 'Reemplaza la lista base de reglas mientras este perfil está activo.',
  'profiles.overlayNotificationsLabel': 'Notificaciones',
  'profiles.profileNameLabel': 'Nombre del perfil',
  'profiles.profileNamePlaceholder': 'p. ej., Concentración, Ejercicio',
  'profiles.profileNameDuplicate': 'Ya existe un perfil con este nombre.',
  'profiles.profileNameTooLong': 'Los nombres de perfil deben tener 32 caracteres como máximo.',
  'profiles.profileNameMissing': 'El nombre del perfil no puede estar vacío.',
  'profiles.activeProfileUnknown': 'Perfil desconocido — se usa la configuración base.',
  'tray.profilesMenu': 'Perfiles de presencia',
  'tray.profilesMenuNone': 'No hay perfiles definidos',
  'tray.profilesMenuActivateBase': 'Usar la configuración base',
  'cli.profileFlag': 'Cambiar a un perfil de presencia con nombre y salir.',
  'cli.profileActive': 'El perfil activo ahora es «{name}».',
  'cli.profileUnknown': 'No hay ningún perfil llamado «{name}» — la configuración base sigue activa.',
  'settings.shortcutToggleProfile': 'Rotar los perfiles de presencia',

  // --- 5.0 wave3 playback-source ---
  'onboarding.playbackSourceMacNote':
    'Está en macOS — aquí solo está disponible la fuente de reproducción de Spotify. La fuente de sesión multimedia del sistema (SMTC de Windows / MPRIS de Linux) no está disponible en macOS, por lo que PresenceJam recurre a Spotify automáticamente. Cambie a Spotify si el asistente informa «sin pista» mientras suena música en otra aplicación.',

  // --- #1120: the snooze entry announcement is count-aware. A one-minute
  // snooze used to announce "Sync paused for 1 minutes"; German needs
  // "Minute" and French "minute" in the singular.
  'dashboard.snoozeStatusStart_one': 'Sincronización en pausa durante {minutes} minuto',
  'dashboard.snoozeStatusStart_other': 'Sincronización en pausa durante {minutes} minutos',

  // --- #739: the wizard's view name, announced on navigation (there is no
  // heading bar above the step, so the step title cannot stand in for it).
  'onboarding.title': 'Configuración',

  // --- #954 / P7: the compact header's abbreviated sync badge. The full
  // "Synchronisierung" / "Synchronisation" label ellipsises into an unreadable
  // fragment at the 400px minimum, and a second badge row is worse. "Sync" is
  // the established short form in all three locales.
  'dashboard.syncingShort': 'Sinc',
  // --- #966 / #981 Settings draft actions ---
  'settings.revertChanges': 'Revertir los cambios',
  'rules.undoRemove': 'Deshacer la eliminación',
};
