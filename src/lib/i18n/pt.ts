/**
 * C6 i18n foundation (docs/scope-3.3.md §C6) — Brazilian-Portuguese dictionary.
 * Typed against `Dict`, so every English key must be present.
 * Model-written Brazilian-Portuguese translation (issue #984) — human review pending before this copy is considered final.
 */

import type { Dict } from './en';

export const pt: Dict = {
  // ── common ────────────────────────────────────────────────────────
  'common.back': 'Voltar',
  'common.backToDashboard': 'Voltar ao painel',
  'common.checkNow': 'Verificar status de login',
  'common.connected': 'Conectado',
  'common.dismiss': 'Dispensar',
  'common.openSignInPage': 'Abra a página de login da Microsoft:',
  'common.enterCodeWhenAsked': 'Digite este código quando solicitado:',
  'common.moreActions': 'Mais ações',
  'common.launchAtLogin': 'Iniciar ao entrar',
  'common.loading': 'Carregando…',
  'common.notConnected': 'Não conectado',
  'common.reconnecting': 'Reconectando…',
  'common.reconnect': 'Reconectar',
  'common.retry': 'Tentar novamente',
  'common.bootFailed': 'Não foi possível carregar o estado do app',
  'common.resetToDefault': 'Restaurar padrão',
  'common.themeToggle': 'Alternar tema',
  'common.waiting': 'Aguardando…',
  'common.waitingForSignIn': 'Aguardando login…',
  'common.codeExpiresIn': 'O código expira em {time}',
  'common.codeExpired': 'Este código expirou — não pode mais ser usado.',
  'common.getNewCode': 'Obter um novo código',
  'common.yes': 'Sim',
  'common.no': 'Não',
  'common.tagline': 'Spotify → Status do Teams',

  // ── dashboard ─────────────────────────────────────────────────────
  'dashboard.spotifyOff': 'Spotify desligado',
  'dashboard.teamsOff': 'Teams desligado',
  'dashboard.syncing': 'Sincronizando',
  'dashboard.logsDetachedTitle': 'Registros (destacado — clique para focar)',
  'dashboard.logsTitle': 'Registros',
  'dashboard.logsDetachedAria': 'Registros (destacado em janela separada)',
  'dashboard.openLogsAria': 'Abrir registros',
  'dashboard.diagnostics': 'Diagnóstico',
  'dashboard.openDiagnosticsAria': 'Abrir diagnóstico',
  'dashboard.settingsDetachedTitle': 'Configurações (destacada — clique para focar)',
  'dashboard.settings': 'Configurações',
  'dashboard.settingsDetachedAria': 'Configurações (destacada em janela separada)',
  'dashboard.openSettingsAria': 'Abrir configurações',
  'dashboard.about': 'Sobre',
  'dashboard.aboutAria': 'Sobre o PresenceJam',
  'dashboard.pauseSync': 'Pausar sincronização',
  'dashboard.resumeSync': 'Retomar sincronização',
  'dashboard.presenceGated': 'Status pausado — você está ocupado, em chamada ou apresentando',
  'dashboard.setupRequired': 'Configuração necessária',
  'dashboard.setupHint':
    'Conecte o Spotify e o Microsoft Teams para que suas faixas em reprodução atualizem seu status do Teams.',
  'dashboard.continueSetup': 'Continuar configuração',
  'dashboard.playing': 'Tocando',
  'dashboard.paused': 'Pausado',
  'dashboard.liveStreamAria': 'Transmissão ao vivo — posição desconhecida',
  'dashboard.yourTeamsStatus': 'Seu status do Teams',
  'dashboard.nothingPlaying': 'Nada tocando',
  'dashboard.nothingPlayingHint':
    'Toque algo no Spotify e ele aparecerá no seu status do Teams.',
  'dashboard.syncCrashed': 'A sincronização parou inesperadamente. Pressione retomar (▶) para reiniciá-la.',
  'dashboard.credentialCheckFailed': 'Não foi possível verificar suas credenciais. Verifique sua conexão e tente novamente.',
  'dashboard.syncToggleFailed': 'Não foi possível iniciar/parar a sincronização. Tente novamente — se persistir, abra Diagnóstico no cabeçalho do painel.',
  'dashboard.statusNotConfigured': 'Não configurado',
  'dashboard.statusNoTrack': 'Nenhuma faixa tocando',
  'dashboard.live': 'Ao vivo',
  'dashboard.refreshStatus': 'Atualizar status',
  'dashboard.refreshing': 'Atualizando…',
  'dashboard.refreshFailed': 'Não foi possível atualizar o status. Tente novamente.',
  'dashboard.refreshAria': 'Atualizar status do Teams agora',

  // ── logs ──────────────────────────────────────────────────────────
  'logs.title': 'Registros',
  'logs.filterAria': 'Filtro de nível de log',
  'logs.level.all': 'Todos',
  'logs.level.trace': 'Rastreamento',
  'logs.level.debug': 'Depuração',
  'logs.level.info': 'Informações',
  'logs.level.warning': 'Aviso',
  'logs.level.error': 'Erro',
  'logs.count_one': '{count} entrada',
  'logs.count_other': '{count} entradas',
  // #1154: `{key}_few` — o português não tem `few` CLDR; espelha `_other`
  // para a paridade Dict (só o polonês seleciona `few`).
  'logs.count_few': '{count} entradas',
  'logs.showingOf': 'Mostrando {shown} de {total}',
  'logs.jumpToLatest': 'Ir para o mais recente',
  'logs.popOut': 'Destacar',
  'logs.clear': 'Limpar',
  'logs.openFolder': 'Abrir pasta',
  'logs.empty': 'Sem entradas de log ainda',
  'logs.emptyHint': 'Entradas ao vivo aparecem aqui quando a sincronização começa e o Spotify está tocando.',

  // ── settings ──────────────────────────────────────────────────────
  'settings.title': 'Configurações',
  'settings.popBackIn': 'Recolocar',
  'settings.popOutActionTitle': 'Destacar em janela própria',
  'settings.unsavedChanges': 'Alterações não salvas',
  'settings.sectionSpotify': 'Spotify',
  'settings.sectionTeams': 'Microsoft Teams',
  'settings.sectionPresence': 'Presença',
  'settings.sectionStatusFormat': 'Formato do status',
  'settings.sectionPolling': 'Frequência de sincronização',
  'settings.sectionNotifications': 'Notificações',
  'settings.sectionAppearance': 'Aparência',
  'settings.clientId': 'ID do cliente',
  'settings.clientIdPlaceholder': 'Digite o ID do cliente do Spotify',
  'settings.clientSecret': 'Segredo do cliente',
  'settings.secretStoredHint':
    'Armazenado com segurança no chaveiro do seu sistema operacional. Para substituí-lo, volte ao painel e escolha Continuar configuração.',
  'settings.secretNotConfigured': 'Não configurado.',
  'settings.runOnboarding': 'Executar integração',
  'settings.toSetUpSpotify': 'para configurar o Spotify.',
  'settings.reconnectSpotify': 'Reconectar Spotify',
  'settings.completeAuthInBrowser': 'Conclua a autenticação no navegador.',
  'settings.playbackScopeBanner': 'O Spotify adicionou controles de reprodução. Clique em Reconectar ao lado desta mensagem para ativá-los.',
  'settings.spotifySecretConflict':
    'O segredo do cliente no seu arquivo de configuração difere do que está no chaveiro. Reconecte o Spotify para resolver.',
  'settings.teamsAuthHint':
    'A autenticação do Teams usa sua conta do Microsoft 365. Nenhuma configuração adicional necessária.',
  'settings.presenceScopeBanner': 'O Teams adicionou detecção de reuniões/chamadas. Clique em Reconectar ao lado desta mensagem para ativá-la.',
  'settings.availabilitySyncLabel': 'Mostrar Disponível ao ouvir',
  'settings.availabilitySyncHint':
    'Desativado por padrão. Quando ativado, o Teams mostra você como Disponível (em vez de Ocupado) enquanto a música toca. Observação: o Teams continua mostrando Ocupado durante chamadas e reuniões.',
  'settings.presenceGateLabel': 'Pausar status durante reuniões/chamadas/Não perturbe',
  'settings.presenceGateHint':
    'Ativado por padrão. Não grava seu status do Spotify enquanto o Teams indica que você está ocupado, em reunião, em chamada ou apresentando.',
  'settings.formatTemplate': 'Modelo de formato',
  'settings.formatTemplatePlaceholder': '🎵 {artist} - {track} 🎧',
  'settings.livePreview': 'Prévia ao vivo',
  'settings.placeholdersHint':
    'Espaços disponíveis: {artist}, {track}, {album}, {emoji}, {device}, {playlist} (ou {context}), {progress}, {shuffle}, {repeat}. Aleatório e Repetir mostram 🔀/🔁 apenas quando ativados.',
  'settings.profanityFilterLabel': 'Filtrar palavrões no status',
  'settings.placeholderTextLabel': 'Texto de espaço reservado',
  'settings.placeholderTextHint':
    'Use {emoji} para o estado de reprodução (🎵 tocando / ⏸ pausado). Exibido quando palavrão é detectado nas informações da faixa.',
  'settings.placeholderTextPlaceholder': 'Ouvindo Spotify agora',
  'settings.profaneSampleToggle': 'Prévia com faixa de exemplo com palavrão',
  'settings.defaultIntervalLabel': 'Intervalo padrão: {seconds}s',
  'settings.minIntervalLabel': 'Intervalo mín. (s)',
  'settings.maxIntervalLabel': 'Intervalo máx. (s)',
  'settings.clampHint':
    'Intervalo mín. excede o máx. — o máx. será salvo como {max}s.',
  'settings.notificationsHint':
    'Cada classe ativada mostra uma notificação do sistema — a primeira pode pedir permissão ao SO.',
  'settings.themeLabel': 'Tema',
  'settings.themeDark': 'Escuro',
  'settings.themeLight': 'Claro',
  'settings.languageLabel': 'Idioma',
  'settings.saveChanges': 'Salvar alterações',
  'settings.saving': 'Salvando…',
  'settings.saved': 'Configurações salvas.',
  'settings.failedToSave': 'Não foi possível salvar as configurações — suas edições continuam aqui. Tente novamente.',
  'settings.openLogsFolder': 'Abrir pasta de logs',
  'settings.previewUnavailable': '(prévia indisponível)',
  // ── diagnostics ───────────────────────────────────────────────────
  'diagnostics.title': 'Diagnóstico',
  'diagnostics.localOnlyHint': 'Instantâneo apenas local — seguro para anexar a um relato de bug.',
  'diagnostics.copy': 'Copiar diagnóstico',
  'diagnostics.saveToFile': 'Salvar em arquivo',
  'diagnostics.copied': 'Diagnóstico copiado para a área de transferência.',
  'diagnostics.copyFailed': 'Falha na cópia — use "Salvar em arquivo".',
  'diagnostics.savedToDownloads': 'Diagnóstico salvo na sua pasta de downloads.',
  'diagnostics.saveFailed': 'Falha ao salvar — use "Copiar diagnóstico".',
  'diagnostics.collecting': 'Coletando diagnóstico…',
  'diagnostics.collectFailed': 'Falha ao coletar diagnóstico',
  'diagnostics.versions': 'Versões',
  'diagnostics.configuration': 'Configuração',
  'diagnostics.connections': 'Conexões',
  'diagnostics.recentLogLines': 'Linhas de log recentes',
  'diagnostics.app': 'PresenceJam',
  'diagnostics.tauri': 'Tauri',
  'diagnostics.os': 'SO',
  'diagnostics.osRelease': 'Versão',
  'diagnostics.installFlavor': 'Tipo de instalação',
  'diagnostics.unknown': 'desconhecido',
  'diagnostics.spotifyClientId': 'ID do cliente do Spotify',
  'diagnostics.redirectUri': 'URI de redirecionamento',
  'diagnostics.notSet': '(não definido)',
  'diagnostics.clientSecretKeychain': 'Segredo do cliente no chaveiro',
  'diagnostics.clearOnPause': 'Limpar status ao pausar',
  'diagnostics.profanityFilter': 'Filtro de palavrões',
  'diagnostics.extraProfanityWords': 'Palavras extras de palavrão',
  'diagnostics.startMinimized': 'Iniciar minimizado',
  'diagnostics.availabilitySync': 'Sincronização de disponibilidade do Teams',
  'diagnostics.presenceGate': 'Bloqueio de presença',
  'diagnostics.pollInterval': 'Intervalo de consulta (padrão/mín./máx.)',
  'diagnostics.logging': 'Registro de log',
  'diagnostics.loggingEnabled': 'ativado ({level})',
  'diagnostics.loggingDisabled': 'desativado',
  'diagnostics.respectManualStatus': 'Respeitar status manual',
  'diagnostics.gateOutOfOffice': 'Bloquear fora do expediente',
  'diagnostics.locale': 'Localidade',
  'diagnostics.updateChannel': 'Canal de atualização',
  'diagnostics.configSnoozed': 'Sincronização adiada',
  'diagnostics.launchAtLogin': 'Iniciar ao entrar',
  'diagnostics.statusRules': 'Regras de status',
  'diagnostics.statusRulesValue': '{quiet}/{quietTotal} horários silenciosos, {rules}/{rulesTotal} regras de faixa ativadas',
  'diagnostics.spotifyConnected': 'Spotify conectado',
  'diagnostics.spotifyTokenExpires': 'Token do Spotify expira',
  'diagnostics.teamsConnected': 'Teams conectado',
  'diagnostics.teamsTokenExpires': 'Token do Teams expira',
  'diagnostics.expired': '(expirado)',
  'diagnostics.keychainSpotifySecret': 'Chaveiro: segredo do Spotify presente',
  'diagnostics.keychainEncryptionKey': 'Chaveiro: chave de criptografia de tokens presente',
  'diagnostics.tokensNeverIncluded':
    'Valores de token nunca são incluídos — apenas carimbos de expiração e indicadores de presença.',
  'diagnostics.noLogLinesYet': 'Sem linhas de log disponíveis ainda.',
  'diagnostics.failedInstallTitle': 'Instalação de atualização com falha',
  'diagnostics.failedInstallVersion': 'Versão',
  'diagnostics.failedInstallError': 'Erro',
  'diagnostics.failedInstallTimestamp': 'Tentado em',
  'diagnostics.failedInstallDismissFailed': 'Não foi possível dispensar o registro de instalação com falha.',
  'diagnostics.resetTokenStorage': 'Redefinir armazenamento local de tokens',
  'diagnostics.resetTokenConfirm':
    'Isso exclui tokens.json e seus auxiliares, além da chave de criptografia de tokens armazenada. Você precisará entrar novamente. Continuar?',
  'diagnostics.resetTokenDone': 'Armazenamento de tokens redefinido. Entre novamente para retomar a sincronização.',
  'diagnostics.resetTokenFailed': 'Falha na redefinição — {error}',
  'diagnostics.syncRunning': 'Sincronização em execução',
  'diagnostics.syncSnoozed': 'Adiada',
  'diagnostics.syncSnoozeMinutes': '{minutes} min restantes',
  'diagnostics.syncManualStatusBlocks': 'Status manual segurando as gravações',
  'diagnostics.syncPresenceGateReason': 'Motivo do bloqueio de presença',
  'diagnostics.syncTransientFailures': 'Falhas de autenticação consecutivas',
  'diagnostics.syncNetworkFailures': 'Falhas de rede consecutivas',

  // ── reconnect ─────────────────────────────────────────────────────
  'reconnect.title': 'Reconectar',
  'reconnect.description': 'A sincronização precisa da sua atenção. Reconecte abaixo para retomar.',
  'reconnect.missingCredentials': 'Credenciais ausentes',
  'reconnect.failed': 'Falhou',
  'reconnect.readyToReconnect': 'Pronto para reconectar',
  'reconnect.spotifyOk': 'Spotify reconectado com sucesso.',
  'reconnect.spotifyNotConfigured':
    'As credenciais do Spotify não estão configuradas nesta máquina.',
  'reconnect.completeAuthInOpenedBrowser':
    'Conclua a autenticação na janela do navegador aberta.',
  'reconnect.tryAgain': 'Tentar novamente',
  'reconnect.clickBelowSpotify': 'Clique abaixo para reconectar sua conta do Spotify.',
  'reconnect.teamsOk': 'Teams reconectado com sucesso.',
  'reconnect.clickBelowTeams':
    'Clique abaixo para reconectar sua conta do Microsoft Teams.',
  'reconnect.tokenStorageUnusable': 'Seus dados de login salvos não podem ser lidos — a chave de criptografia de tokens armazenada está inutilizável.',
  'reconnect.resetTokenStorage': 'Redefinir armazenamento local de tokens',
  'reconnect.resetTokenConfirm':
    'Isso exclui tokens.json e seus auxiliares, além da chave de criptografia de tokens armazenada. Você precisará entrar novamente. Continuar?',
  'reconnect.resetTokenDone': 'Armazenamento de tokens redefinido. Entre novamente abaixo para retomar.',
  'reconnect.resetTokenFailed': 'Falha na redefinição — {error}',
  'reconnect.missingCredsTitle': 'Credenciais do Spotify ausentes?',
  'reconnect.reenterCredsHint':
    'Você precisará digitar novamente seu ID do cliente e seu segredo do cliente.',
  'reconnect.goToFullSetup': 'Ir para a configuração completa',
  'reconnect.reconnectTeams': 'Reconectar Teams',

  // ── about ─────────────────────────────────────────────────────────
  'about.version': 'Versão {version}',
  'about.description':
    'Mostra o que você está ouvindo no Spotify no seu status do Microsoft Teams — automaticamente.',
  'about.statusSync': 'Sincronização de status',
  'about.live': 'Ao vivo',
  'about.auth': 'Login',
  'about.authMethod': 'Spotify + Microsoft',
  'about.storage': 'Armazenamento',
  'about.osKeychain': 'Chaveiro do SO',
  'about.githubRepo': 'Repositório do GitHub',
  'about.releases': 'Versões',
  'about.reportIssue': 'Relatar um problema',

  // ── update banner ─────────────────────────────────────────────────
  'update.available': 'Atualização v{version} disponível',
  'update.stagedQuit': 'A v{version} será instalada quando você sair do PresenceJam',
  'update.downloadFailed': 'Falha no download — {error}',
  'update.downloadAndInstall': 'Baixar e instalar',
  'update.downloading': 'Baixando…',
  'update.installOnQuit': 'Instalar ao sair',
  'update.preparing': 'Preparando…',
  'update.dismissAria': 'Dispensar banner de atualização',
  'update.confirmQuitInstall':
    'Instalar a v{staged} ao sair? Versão atual: v{current}.',
  'update.confirmQuitInstallUnknown': 'Instalar a v{staged} ao sair?',
  'update.stagedVsCurrent':
    'A v{staged} será instalada quando você sair (atual v{current})',
  'update.staleSkipped':
    'A v{staged} foi ignorada — sua atual v{current} é mais recente.',
  'update.staleSkippedUnknown':
    'A v{staged} foi ignorada — ela não é mais recente que sua versão atual.',
  'update.installAnyway': 'Instalar mesmo assim',
  // #894: uma instalação `.deb` / `.rpm` é atualizada pelo gerenciador de
  // pacotes, não pelo atualizador interno — o manifesto de versão oferece
  // apenas o pacote AppImage, que nenhum dos instaladores consegue aplicar.
  // O banner mostra o comando em vez do botão de instalar.
  'update.packageManagedDeb':
    'A v{version} está disponível. Esta cópia foi instalada como pacote .deb, então as atualizações vêm do apt.',
  'update.packageManagedRpm':
    'A v{version} está disponível. Esta cópia foi instalada como pacote .rpm, então as atualizações vêm do dnf.',
  'update.packageManagerInstallDeb': 'sudo apt install ./{file}',
  'update.packageManagerInstallRpm': 'sudo dnf install ./{file}',

  // ── onboarding ────────────────────────────────────────────────────
  'onboarding.stepOf': 'Etapa {step} de 3',
  'onboarding.step1Title': 'Conectar Spotify',
  'onboarding.step1Intro':
    'Cole seu ID do cliente e seu segredo do cliente do Spotify abaixo e escolha Conectar Spotify — abriremos a página de login do Spotify.',
  'onboarding.getCredentials': 'Obtenha suas credenciais do Spotify',
  'onboarding.instruction1':
    'Abra o painel de desenvolvedor do Spotify e crie um app.',
  'onboarding.instruction2': 'Em URIs de redirecionamento, adicione presencejam://callback (isso diz ao Spotify para onde enviar você de volta).',
  'onboarding.instruction3':
    'Copie o ID do cliente e o segredo do cliente das configurações do app.',
  'onboarding.clientIdPlaceholder': 'ID do cliente do Spotify com 32 caracteres',
  'onboarding.clientSecretPlaceholder': 'Segredo do cliente do Spotify',
  'onboarding.connectSpotify': 'Conectar Spotify',
  'onboarding.signInWaiting': 'Login do Spotify aguardando…',
  'onboarding.manualUrlHint':
    'Termine o login com o Spotify no seu navegador e cole abaixo o endereço completo da barra de endereços.',
  'onboarding.manualUrlLabel': 'URL de redirecionamento do Spotify',
  'onboarding.manualUrlPlaceholder': 'presencejam://callback?code=…',
  'onboarding.submitCode': 'Enviar código',
  'onboarding.connectedToSpotify': 'Conectado ao Spotify',
  'onboarding.continue': 'Continuar →',
  'onboarding.step2Title': 'Conectar Microsoft Teams',
  'onboarding.step2Intro':
    'Usamos o fluxo de código de dispositivo da Microsoft — um código único que você digita em uma página da Microsoft. Nenhuma configuração extra necessária.',
  'onboarding.startMicrosoftSignIn': 'Conectar Microsoft Teams',
  'onboarding.connectedToTeams': 'Conectado ao Microsoft Teams',
  'onboarding.step3Title': 'Toques finais',
  'onboarding.step3Intro':
    'Escolha como sua mensagem de status deve ficar e se o PresenceJam deve iniciar quando você entrar.',
  'onboarding.statusTemplate': 'Modelo de status',
  'onboarding.placeholdersHint':
    'Espaços: {artist}, {track}, {album}, {emoji}, {device}, {playlist} (ou {context}), {progress}, {shuffle}, {repeat}',
  'onboarding.pollInterval': 'Com que frequência verificar o Spotify: {seconds}s',
  'onboarding.settingUp': 'Configurando…',
  'onboarding.finishSetup': 'Concluir configuração',

  // ── validation / errors ───────────────────────────────────────────
  'validation.clientIdRequired': 'O ID do cliente do Spotify é obrigatório.',
  'validation.clientIdFormat':
    'O ID do cliente do Spotify deve ter exatamente 32 caracteres hexadecimais.',
  'validation.clientSecretRequired': 'O segredo do cliente do Spotify é obrigatório.',
  'validation.clientSecretTooShort':
    'Esse segredo do cliente parece curto demais — deve ter pelo menos 32 caracteres. Verifique se houve erro de copiar e colar.',
  'validation.noCodeInUrl':
    'Essa URL não contém código de login — cole o endereço completo da barra de endereços do seu navegador depois que o Spotify redirecionar você.',
  'validation.connectBothFirst':
    'Conecte o Spotify e o Teams antes de concluir a configuração.',
  'validation.setupFailed': 'Falha na configuração: {error}',

  // ── routes / chrome ───────────────────────────────────────────────
  'routes.skipToMainContent': 'Pular para o conteúdo principal',
  'routes.unknownPane': 'Painel desconhecido: {pane}',

  // ── feat/45-features: status rules (#432) + support snapshot (#434) ──
  'rules.sectionTitle': 'Regras de status',
  'rules.sectionHint':
    'Horários silenciosos e regras de faixa suprimem a gravação do status do Teams, reutilizando o mesmo caminho do bloqueio de presença — uma regra liberada publica automaticamente no meio da faixa.',
  'rules.quietHoursLabel': 'Horários silenciosos',
  'rules.noQuietHours': 'Nenhum horário silencioso definido — o status sincroniza a toda hora.',
  'rules.quietStart': 'Início do horário silencioso',
  'rules.quietEnd': 'Fim do horário silencioso',
  'rules.quietDays': 'Dias ativos (nenhum selecionado = todos os dias)',
  'rules.dayEveryDay': 'Todos os dias',
  'rules.day1': 'Seg',
  'rules.day2': 'Ter',
  'rules.day3': 'Qua',
  'rules.day4': 'Qui',
  'rules.day5': 'Sex',
  'rules.day6': 'Sáb',
  'rules.day7': 'Dom',
  'rules.addQuietHours': 'Adicionar horário silencioso',
  'rules.trackRulesLabel': 'Regras de faixa',
  'rules.noTrackRules': 'Nenhuma regra de faixa definida — todas as faixas sincronizam normalmente.',
  'rules.artistPlaceholder': 'Artista contém…',
  'rules.trackPlaceholder': 'Título da faixa contém…',
  'rules.replacementPlaceholder': 'Publicar isto em vez disso (vazio = suprimir)',
  'rules.addTrackRule': 'Adicionar regra de faixa',
  'rules.removeRule': 'Remover',
  'rules.ruleEnabled': 'Ativada',
  'logs.copySnapshot': 'Copiar instantâneo',
  'logs.snapshotCopied': 'Instantâneo redigido copiado para a área de transferência.',
  'logs.snapshotCopyFailed': 'Não foi possível copiar o instantâneo.',
  'logs.openFolderError':
    'Não foi possível abrir a pasta de logs. Ela pode ainda não existir — tente reiniciar o app para criá-la.',
  // 4.6 additions
  'dashboard.availabilityListening': 'Ouvindo (Disponível)',
  'dashboard.availabilityCleared': 'Disponibilidade limpa',
  'settings.saveAndLeave': 'Salvar e sair',
  'settings.discardChanges': 'Descartar alterações',
  'settings.stayHere': 'Ficar aqui',
  'settings.notificationsDenied':
    'As notificações estão bloqueadas pelo sistema. Permita-as nas configurações do sistema e ative isto novamente.',
  'diagnostics.expiryBuffer': 'Margem de atualização do token',
  'diagnostics.teamsRefreshTokenPresent': 'Token de atualização do Teams armazenado',
  'reconnect.restartSignIn': 'Reiniciar login',
  'update.stagingProgress': 'Preparando atualização — {percent}%',
  'update.cancelStage': 'Cancelar',
  'onboarding.submitting': 'Enviando…',
  'settings.episodeFormatHint':
    'Podcasts e audiolivros usam modelo próprio — 🎙️ {show} - {episode} — então seu modelo de música acima não se aplica a eles.',
  'reconnect.keychainUnavailableBadge':
    'Chaveiro indisponível',
  'reconnect.keychainUnavailableHint':
    'O PresenceJam não conseguiu ler seu segredo do cliente do Spotify salvo: o chaveiro do sistema está bloqueado ou ausente. Desbloqueie-o (ou instale um provedor de Secret Service como o gnome-keyring) e tente novamente — seu segredo continua armazenado, então você não precisa configurar o Spotify de novo.',
  'settings.secretKeychainUnavailable':
    'Chaveiro do sistema indisponível — pode estar bloqueado ou ausente. Desbloqueie-o (ou instale um provedor de Secret Service) para usar seu segredo salvo; ele continua armazenado.',
  'diagnostics.quarantineTitle': 'As configurações foram redefinidas',
  'diagnostics.quarantineBodyNow':
    'O PresenceJam não conseguiu ler seu arquivo de configurações, então todas as configurações foram redefinidas para o padrão.',
  'diagnostics.quarantineBodyEarlier':
    'Uma execução anterior não conseguiu ler seu arquivo de configurações e o redefiniu para os padrões.',
  'diagnostics.quarantineBackupPresent':
    'O original ilegível foi mantido ao lado do seu arquivo de configurações como {name}, então os valores que ele continha ainda podem ser recuperados.',
  'diagnostics.quarantineBackupMissing':
    'O original ilegível continua ao lado do seu arquivo de configurações como config.json.',
  'diagnostics.quarantineWhere':
    'Ambos os arquivos ficam na pasta PresenceJam dentro da sua pasta de configuração de usuário — o backup está ao lado de config.json.',
  // 4.6 additions (presence rules #634/#635/#636/#637 + #538 consumption sites)
  'rules.replacementClampHint':
    'O status substituto é limitado a {max} caracteres; o restante não é publicado.',
  'rules.presenceLabel': 'Presença enquanto esta regra se aplica',
  'rules.presenceNone': 'Não alterar minha presença',
  'rules.presenceHint':
    'Uma regra pode definir disponibilidade/atividade do Teams, mas só enquanto a "sincronização de disponibilidade" estiver ativada; ela nunca substitui uma chamada, uma reunião ou um status definido por você manualmente.',
  'settings.respectManualStatusLabel': 'Nunca sobrescrever um status que eu defini manualmente',
  'settings.respectManualStatusHint':
    'Reutiliza a leitura de presença que o bloqueio já faz, então não custa nenhuma requisição extra — seu texto é respeitado até você alterá-lo ou ele expirar.',
  'settings.gateOutOfOfficeLabel': 'Pausar enquanto estou fora do expediente',
  'settings.gateOutOfOfficeHint':
    'Ignora a atualização de status enquanto sua configuração de fora do expediente do Teams estiver ativada. Uma regra de faixa com ação própria de presença substitui isto.',
  // Issue #872: the OS-level presentation gate (full-screen app, slide
  // deck, Windows Focus Assist Quiet Time). OFF by default, matching how
  // `availabilitySync` and `gateWhenOutOfOffice` shipped — 4.7 behaviour
  // is unchanged until the user opts in.
  'settings.gateWhenPresentingLabel': 'Pausar enquanto estou apresentando',
  'settings.gateWhenPresentingHint':
    'Não grava seu status do Spotify enquanto o SO relata app em tela cheia, apresentação de slides ou Horário Silencioso. Linux/macOS nunca relatam esse sinal, então a chave não tem efeito lá.',
  // Issue #873: the desktop-idle gate. `0` (the default) keeps 4.7
  // behaviour — the app keeps advertising listening until the user
  // explicitly opts in.
  'settings.idleAwayLabel': 'Pausar quando minha área de trabalho está ociosa',
  'settings.idleAwayHint':
    'Para de anunciar seu status do Spotify quando a área de trabalho fica sem entrada de teclado/mouse por estes segundos. 60–3600; 0 desativa. Linux/macOS nunca relatam esse sinal.',
  'settings.extraWordsLabel': 'Palavras personalizadas para filtrar',
  'settings.extraWordsHint':
    'Uma palavra ou frase por linha. Aplicada com os mesmos limites da lista interna.',
  'settings.extraWordsPlaceholder': 'palavra ou frase',
  'settings.extraWordsClampHint':
    'Apenas as primeiras {max} entradas de {chars} caracteres são mantidas — {kept} serão filtradas.',
  'settings.pauseBackoffMaxLabel': 'Teto de espera ao pausar (segundos)',
  'settings.pauseBackoffClampHint':
    'O intervalo permitido é de {min}–{max} segundos; {effective} será usado.',
  'dashboard.presenceGatedQuietHours': 'Status pausado — horário silencioso ativo',
  'dashboard.presenceGatedTrackRule': 'Status pausado — uma regra de faixa coincidiu',
  'dashboard.presenceGatedManualStatus':
    'Status pausado — você definiu uma mensagem de status manualmente',
  'dashboard.presenceGatedOutOfOffice': 'Status pausado — você está fora do expediente',
  'dashboard.presenceGatedPresenting':
    'Status pausado — você está apresentando ou em app de tela cheia',
  'dashboard.presenceGatedQuietTime': 'Status pausado — Foco Assist está ativado',
  'dashboard.presenceGatedIdle': 'Status pausado — área de trabalho ociosa',
  // 4.7.0 — S6 (tray localization): `config.locale` is the single source of
  // truth, so the picker also drives the tray and the native app menu.
  'settings.languageHint':
    'Também se aplica ao menu da bandeja e ao menu nativo do aplicativo.',
  // #984: the follow-system-language toggle beside the picker. The mode
  // re-resolves from the OS language on every boot (and on change) and
  // persists the resolution, so the tray and the native menu follow too.
  'settings.languageFollowSystemLabel': 'Seguir o idioma do sistema',
  'settings.languageFollowSystemHint':
    'Quando ativado, o seletor de idioma acima é ignorado: o app resolve novamente a partir do idioma do seu sistema operacional a cada inicialização. Idiomas do sistema sem suporte continuam recorrendo ao inglês.',
  // Issue #932: the banner used to be Teams-only
  // (`settings.teamsPersistWarning`, added with #562). The new
  // Spotify mirror ships the same copy with a `{provider}` placeholder
  // — the Settings card passes the display name (`Microsoft Teams` or
  // `Spotify`) so one banner covers both providers.
  'settings.authPersistWarning':
    'Login feito, mas este dispositivo não conseguiu salvar a sessão de {provider} — funciona até você sair. Reconecte {provider} para tentar salvar novamente.',

  // 4.7.0 — S4 (rules engine)
  'rules.quietWindowHint':
    'Horários silenciosos atravessam a meia-noite — 22:00–07:00 avança pela madrugada. Cada metade pertence à noite em que começa: só com segunda selecionada, a janela cobre a noite de segunda até a manhã de terça. Fim 00:00 significa meia-noite (fim do dia), e início igual ao fim nunca coincide.',
  'rules.pausePollingLabel': 'Interromper consultas nesta janela',
  'rules.pausePollingHint':
    'Enquanto esta janela está ativa, o Spotify não é consultado — sem atualização de status e sem chamada ao Teams. As consultas voltam sozinhas quando a janela termina.',
  'rules.trackRulesOrderHint':
    'As regras são avaliadas de cima para baixo — a primeira que coincidir vence. Uma regra sem dias da semana vale todos os dias, fim 00:00 significa fim do dia, e início igual ao fim nunca coincide. Uma janela que cruza a meia-noite pertence à noite em que começa: só com segunda selecionada, 22:00–07:00 cobre a noite de segunda até a manhã de terça.',
  'rules.ruleStart': 'Início da janela da regra',
  'rules.ruleEnd': 'Fim da janela da regra',
  'rules.ruleDays': 'Dias ativos desta regra (nenhum selecionado = todos os dias)',
  'rules.moveRuleUp': 'Mover regra {n} para cima',
  'rules.moveRuleDown': 'Mover regra {n} para baixo',
  'rules.manualStatusLabel': 'Pausar e parar texto de status',
  'rules.manualStatusHint':
    'O texto publicado como seu status do Teams enquanto a reprodução está pausada e quando nada está tocando. O emoji de música é adicionado para você; limpar um campo restaura o padrão.',
  'rules.pausedStatusPlaceholder': 'Pausado',
  'rules.stoppedStatusPlaceholder': 'Nada tocando no Spotify',

  // 4.7.0 — S5 (log rotation + settings export/import)
  'settings.sectionLogging': 'Registro de log',
  'settings.loggingEnabledLabel': 'Gravar um arquivo de log',
  'settings.logLevelLabel': 'Nível de log',
  'settings.logMaxSizeLabel': 'Tamanho máximo do arquivo de log (MB)',
  'settings.logKeepFilesLabel': 'Arquivos de log arquivados a manter',
  'settings.logRotationHint':
    'O limite de tamanho e o número de arquivos arquivados valem na próxima vez que o PresenceJam iniciar. O log sendo gravado agora não conta: a pasta de logs guarda no máximo um arquivo a mais que o número definido. Desativar o registro ou mudar o nível vale na hora.',
  'settings.sectionBackup': 'Backup',
  'settings.backupHint':
    'Exportar grava uma cópia destas configurações que você pode guardar ou levar para outra máquina. Seu segredo do cliente do Spotify fica no chaveiro do sistema e nunca é incluído — e um arquivo que contenha um é recusado na importação.',
  'settings.backupExport': 'Exportar configurações…',
  'settings.backupImport': 'Importar configurações…',
  'settings.backupExportDialogTitle': 'Exportar configurações do PresenceJam',
  'settings.backupImportDialogTitle': 'Importar configurações do PresenceJam',
  'settings.backupConfirmOverwrite':
    'Importar substitui todas as suas configurações atuais. O arquivo atual é mantido ao lado como config.json.bak. Continuar?',
  'settings.backupExported': 'Configurações exportadas para {path}',
  'settings.backupImported': 'Configurações importadas de {path}',
  'settings.backupError': 'A ação de backup não pôde ser concluída: {error}',

  // 4.7.0 — S9 (issue #677: the tray snooze / "pause sync for a while")
  'dashboard.snoozeChip': 'Adiado — resta {remaining} (até {time})',
  'dashboard.snoozeResume': 'Retomar agora',
  'dashboard.snoozeResuming': 'Retomando…',
  'dashboard.snoozeResumeFailed':
    'Não foi possível retomar a sincronização. O adiamento continua salvo — tente novamente.',
  // Issue #736: the chip's live region announces entry/exit once; the
  // per-second countdown is no longer in the live region.
  'dashboard.snoozeStatusEnd': 'Sincronização retomada',

  // 4.7.0 — S7 notifications (#675): one toggle per desktop-notification
  // class, plus the copy for the three classes the always-mounted layout
  // dispatches (track changes keep dispatching from the Dashboard card).
  'settings.notificationsTrackChange': 'Avise-me quando a faixa mudar',
  'settings.notificationsSyncStopped': 'Avise-me quando a sincronização parar sozinha',
  'settings.notificationsAuthRequired': 'Avise-me quando eu precisar entrar no Teams de novo',
  'settings.notificationsUpdateStaged': 'Avise-me quando uma atualização for instalar ao sair',
  'notifications.syncStoppedTitle': 'PresenceJam parou de sincronizar',
  'notifications.syncStoppedBody':
    'A sincronização de status parou sozinha. Abra o PresenceJam para reiniciá-la.',
  'notifications.authRequiredTitle': 'Login do Teams necessário',
  'notifications.authRequiredBody':
    'Sua sessão do Teams expirou. Entre novamente para seu status continuar sincronizando.',
  'notifications.updateStagedTitle': 'Atualização pronta',
  'notifications.updateStagedBody': 'O PresenceJam {version} será instalado quando você sair.',
  // 4.7.0 — update channel (#678)
  'settings.sectionUpdates': 'Atualizações',
  'settings.updateChannelLabel': 'Canal de versão',
  'settings.updateChannelStable': 'Estável',
  'settings.updateChannelBeta': 'Beta',
  'settings.updateChannelHint':
    'Versões beta usam o feed beta contínuo quando disponível; se ele estiver ausente ou sem versão mais recente, o Beta recorre à versão estável. Versões beta instalam apenas ao sair.',
  'update.betaOnQuitOnly':
    'Canal beta: atualizações instalam quando você sai — não há caminho de baixar e reiniciar no Beta.',
  // 4.7.0 — S8 global hotkeys
  'settings.sectionShortcuts': 'Atalhos globais',
  'settings.shortcutsHint':
    'Funcionam com a janela oculta. Clique em um campo e pressione a combinação desejada — o campo registra o que você pressiona, não o que você digita.',
  'settings.shortcutTogglePlayback': 'Alternar reprodução',
  'settings.shortcutToggleSync': 'Pausar ou retomar sincronização',
  'settings.shortcutUnbound': 'Não definido — clique e pressione uma combinação',
  'settings.shortcutClear': 'Limpar',
  'settings.shortcutRegistered': 'Ativo',
  'settings.shortcutNotRegistered': 'Não registrado nesta área de trabalho',
  'settings.shortcutCaptureReleased':
    'Solto durante a gravação — a vinculação atual dispararia em vez de ser gravada',
  'settings.shortcutRejected': 'Não pode ser usado: {reason}',
  'settings.shortcutRegistrationFailed': 'Falha no registro nesta área de trabalho: {reason}',
  // Issue #968: typed reason codes from the Rust validator. The Settings
  // card maps each `kind` to a dictionary entry so the rejection copy is
  // localized; `shortcutReasonUnknown` renders the backend's free-form text
  // for genuinely foreign refusals (compositor / app-owned combos).
  'settings.shortcutReasonNotAKey': '“{accelerator}” não é um atalho reconhecido',
  'settings.shortcutReasonConflict':
    'Conflita com o atalho {other} — um acelerador não pode comandar ambas as ações',
  // Issue #810: a bare key would be grabbed system-wide. Function keys
  // (F1–F24) and media keys are exempt and bind without a modifier.
  'settings.shortcutReasonNeedsModifier':
    '“{accelerator}” precisa de ao menos um modificador — uma tecla solta seria capturada em todos os aplicativos',
  'settings.shortcutReasonAutostart':
    'Falha ao iniciar com o login: {cause}',
  'settings.shortcutReasonUnknown': '{message}',
  'settings.shortcutReasonX11Unavailable':
    'Atalhos globais precisam de um display X11 acessível nesta área de trabalho',
  'settings.shortcutReasonWorkerUnavailable':
    'O worker de atalhos globais não pôde ser verificado; atalhos indisponíveis',
  // 4.7.0 — S12 hygiene (theme/density)
  'settings.themeSystem': 'Sistema',
  'settings.themeHint':
    '“Sistema” segue a aparência do seu sistema operacional; Escuro e Claro ficam fixos.',
  'settings.densityCompactLabel': 'Espaçamento compacto',
  'settings.densityHint': 'Aperta o espaçamento e a escala tipográfica. Independente do tema.',
  // --- 5.0 wave1 i18n-lib ---
  // Key requests routed through this slice's dictionaries (the owning slice
  // cannot edit them).
  'onboarding.pollIntervalClamped':
    'Seu intervalo salvo é {stored}s, fora da faixa {min}–{max}s desta etapa — {seconds}s será usado.',
  'update.stagingProgressLabel': 'Atualização preparando',
  'rules.presenceAvailable': 'Disponível',
  'rules.presenceBusyCall': 'Ocupado — Em chamada',
  'rules.presenceBusyConference': 'Ocupado — Em audioconferência',
  'rules.presenceAway': 'Ausente',
  'rules.presenceDndPresenting': 'Não perturbe — Apresentando',
  // --- 5.0 wave3 features-presence ---
  'dashboard.manualStatusTitle': 'Status manual',
  'dashboard.manualStatusPlaceholder': 'Defina um status que sua equipe verá por um tempo',
  'dashboard.manualStatusExpiryLabel': 'Expira após',
  'dashboard.manualStatusExpiry15': '15 minutos',
  'dashboard.manualStatusExpiry30': '30 minutos',
  'dashboard.manualStatusExpiry60': '1 hora',
  'dashboard.manualStatusExpiry120': '2 horas',
  'dashboard.manualStatusSet': 'Definir status',
  'dashboard.manualStatusClear': 'Limpar status',
  'dashboard.manualStatusActive': 'Ativo até {expiry}',
  'dashboard.manualStatusActiveEmpty': 'Ativo (expira em breve)',
  'dashboard.manualStatusRecentTitle': 'Status recentes',
  'dashboard.manualStatusRecentEmpty': 'Sem status recentes ainda',
  'dashboard.manualStatusFiltered': 'O status foi reescrito pelo seu filtro de palavrões',
  'dashboard.activityTitle': 'Atividade',
  'dashboard.activityEmpty': 'Sem decisões ainda — inicie a sincronização para ver o que seu app escolheu',
  'dashboard.volumeLabel': 'Volume',
  'dashboard.volumeAria': 'Controle de volume do Spotify',
  'dashboard.seekAria': 'Clique para buscar',
  'dashboard.seekUnavailableAria': 'Este dispositivo não suporta busca',

  // --- 5.0 wave3 features-outlook ---
  'rules.importWorkingHours': 'Importar horário de trabalho do Outlook',
  'rules.importWorkingHoursHint': 'Lê sua aba de horário de trabalho e transforma cada bloco livre em regra de horário silencioso. Confira antes de aplicar.',
  'rules.importWorkingHoursPreviewTitle': 'Prévia do horário de trabalho do Outlook',
  'rules.importWorkingHoursApply': 'Aplicar estas regras',
  'rules.importWorkingHoursReplace': 'Substituir regras de horário silencioso existentes',
  'rules.importWorkingHoursCancel': 'Cancelar',
  'rules.importWorkingHoursReplaceHint': 'Substitui suas regras de horário silencioso existentes pelo conjunto importado. Desmarque para manter ambos.',
  'rules.importWorkingHoursDaysLabel': 'O Outlook informa {days} trabalhando em {start}–{end}',
  'rules.importWorkingHoursDaysAllOff': 'O Outlook não informa dias de trabalho — nada para importar',

  // --- 5.0 wave3 features-gating ---

  // --- 5.0 wave3 features-profiles ---
  'rules.testTitle': 'Testar estas regras',
  'rules.testHint':
    'Digite uma faixa de exemplo, escolha um minuto do dia + dia da semana e veja exatamente qual regra (se houver) dispararia.',
  'rules.testArtistLabel': 'Artista',
  'rules.testTrackLabel': 'Título da faixa',
  'rules.testAlbumLabel': 'Álbum (opcional)',
  'rules.testShowLabel': 'Programa / podcast (opcional)',
  'rules.testDeviceLabel': 'Dispositivo do Spotify (opcional)',
  'rules.testPlaylistLabel': 'URI da playlist (opcional)',
  'rules.testDurationLabel': 'Duração (mm:ss, opcional)',
  'rules.testWeekdayLabel': 'Dia da semana',
  'rules.testMinuteLabel': 'Minuto do dia (HH:MM)',
  'rules.testRun': 'Executar teste',
  'rules.testRunning': 'Executando…',
  'rules.testSummaryNoMatch': 'Nenhuma regra coincidiria com esta faixa.',
  'rules.testSummaryRuleMatched': 'A regra {index} dispararia: {summary}',
  'rules.testStepMatched': 'coincidiu',
  'rules.testStepNotMatched': 'NÃO coincidiu',
  'rules.testStepReason': 'motivo: {reason}',
  'rules.testStepDisabled': 'regra está desativada',
  'rules.testStepScheduleOutside': 'agenda não contém este minuto do dia',
  'rules.testStepNegated': 'negação inverteu as condições',
  'rules.matchKindLabel': 'Estilo de coincidência',
  'rules.matchKindSubstring': 'Trecho (padrão)',
  'rules.matchKindExact': 'Coincidência exata',
  'rules.matchKindGlob': 'Padrão glob',
  'rules.albumSubstringLabel': 'Álbum contém…',
  'rules.showSubstringLabel': 'Programa contém…',
  'rules.deviceSubstringLabel': 'Dispositivo contém…',
  'rules.playlistUriLabel': 'URI da playlist contém…',
  'rules.minDurationLabel': 'Duração mínima (segundos)',
  'rules.negateLabel': 'Negar (coincidir quando as condições NÃO valem)',
  'rules.actionLabel': 'Ação',
  'rules.actionSuppress': 'Suprimir status',
  'rules.actionReplace': 'Substituir status por…',
  'rules.actionSnoozeMinutes': 'Adiar por minutos…',
  'rules.actionProfile': 'Trocar para o perfil…',
  'rules.actionPresence': 'Definir par de presença…',
  'rules.actionReplaceStatusPlaceholder': 'Texto do status',
  'rules.actionSnoozePlaceholder': 'Minutos',
  'rules.actionProfilePlaceholder': 'Nome do perfil',
  'rules.actionAvailabilityPlaceholder': 'Disponibilidade',
  'rules.actionActivityPlaceholder': 'Atividade',
  'dashboard.gateWhyTitle': 'Por que o status está pausado?',
  'dashboard.gateWhyShow': 'Mostrar motivo',
  'dashboard.gateWhyHide': 'Ocultar motivo',
  'dashboard.gateWhyEmpty': 'Nenhum bloqueio ativo agora — o status está sincronizando normalmente.',

  'profiles.sectionTitle': 'Perfis de presença',
  'profiles.sectionHint':
    'Salve uma sobreposição nomeada (formato de status, bloqueios, regras) e alterne com a bandeja, uma tecla de atalho ou a CLI.',
  'profiles.empty': 'Nenhum perfil definido.',
  'profiles.addProfile': 'Adicionar perfil',
  'profiles.removeProfile': 'Remover',
  'profiles.activeProfileLabel': 'Perfil ativo',
  'profiles.activeProfileNone': 'Nenhum — usar a configuração base',
  'profiles.overlayStatusFormatLabel': 'Formato de status',
  'profiles.overlayClearOnPauseLabel': 'Limpar status ao pausar',
  'profiles.overlayAvailabilitySyncLabel': 'Sincronizar disponibilidade com o Teams',
  'profiles.overlayGateOutOfOfficeLabel': 'Bloquear fora do expediente',
  'profiles.overlayGatePresentingLabel': 'Bloquear em apps de tela cheia',
  'profiles.overlayIdleAwayLabel': 'Parar de anunciar após ocioso (segundos)',
  'profiles.overlayPreferredPresenceLabel': 'Presença preferida',
  'profiles.overlayRulesLabel': 'Subconjunto de regras',
  'profiles.overlayRulesHint': 'Substitui a lista base de regras enquanto este perfil está ativo.',
  'profiles.overlayNotificationsLabel': 'Notificações',
  'profiles.profileNameLabel': 'Nome do perfil',
  'profiles.profileNamePlaceholder': 'ex.: Foco, Treino',
  'profiles.profileNameDuplicate': 'Já existe um perfil com este nome.',
  'profiles.profileNameTooLong': 'Nomes de perfil devem ter 32 caracteres ou menos.',
  'profiles.profileNameMissing': 'O nome do perfil não pode estar vazio.',
  'profiles.activeProfileUnknown': 'Perfil desconhecido — usando a configuração base.',
  'tray.profilesMenu': 'Perfis de presença',
  'tray.profilesMenuNone': 'Nenhum perfil definido',
  'tray.profilesMenuActivateBase': 'Usar configuração base',
  'cli.profileFlag': 'Trocar para um perfil de presença nomeado e sair.',
  'cli.profileActive': 'O perfil ativo agora é "{name}".',
  'cli.profileUnknown': 'Sem perfil chamado "{name}" — a configuração base continua ativa.',
  'settings.shortcutToggleProfile': 'Alternar perfis de presença',

  // --- 5.0 wave3 playback-source ---
  'onboarding.playbackSourceMacNote':
    'Você está no macOS — apenas a fonte de reprodução do Spotify está disponível aqui. A fonte de sessão de mídia do sistema (SMTC do Windows / MPRIS do Linux) não está disponível no macOS, então o PresenceJam recorre ao Spotify automaticamente. Troque para o Spotify se o assistente relatar "sem faixa" enquanto música toca em outro app.',

  // --- #1120: the snooze entry announcement is count-aware. A one-minute
  // snooze used to announce "Sync paused for 1 minutes"; German needs
  // "Minute" and French "minute" in the singular.
  'dashboard.snoozeStatusStart_one': 'Sincronização pausada por {minutes} minuto',
  'dashboard.snoozeStatusStart_other': 'Sincronização pausada por {minutes} minutos',
  // #1154: como acima — sem `few` CLDR; espelha `_other`.
  'dashboard.snoozeStatusStart_few': 'Sincronização pausada por {minutes} minutos',

  // --- #739: the wizard's view name, announced on navigation (there is no
  // heading bar above the step, so the step title cannot stand in for it).
  'onboarding.title': 'Configuração',

  // --- #954 / P7: the compact header's abbreviated sync badge. The full
  // "Synchronisierung" / "Synchronisation" label ellipsises into an unreadable
  // fragment at the 400px minimum, and a second badge row is worse. "Sync" is
  // the established short form in all three locales.
  'dashboard.syncingShort': 'Sync',
  // --- #966 / #981 Settings draft actions ---
  'settings.revertChanges': 'Reverter alterações',
  'rules.undoRemove': 'Desfazer remoção',
};
