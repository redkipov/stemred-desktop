import './styles.css';

import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrent, onOpenUrl } from '@tauri-apps/plugin-deep-link';

declare const __APP_VERSION__: string;

type BootstrapState = 'ready' | 'update_available' | 'update_required' | 'offline';

type BootstrapResult = {
  state: BootstrapState;
  remote_url: string;
  current_shell_version: string;
  min_shell_version: string;
  recommended_shell_version: string;
  web_build_id: string;
  api_build_id: string;
  message?: string;
};

type DesktopUpdateSnapshot = {
  phase: string;
  current_version: string;
  target_version?: string;
  install_ready: boolean;
  mandatory: boolean;
  error_code?: string;
};

type ShellLocale = 'en' | 'ru' | 'es' | 'ar' | 'fr';

const SHELL_LOCALES = ['en', 'ru', 'es', 'ar', 'fr'] as const;
const SHELL_INTL_LOCALES: Record<ShellLocale, string> = {
  en: 'en-US',
  ru: 'ru-RU',
  es: 'es-ES',
  ar: 'ar-SA',
  fr: 'fr-FR',
};
const SHELL_TRANSLATIONS: Record<string, Record<Exclude<ShellLocale, 'ru'>, string>> = {
  'Доступно обновление': {
    en: 'Update available',
    es: 'Actualización disponible',
    ar: 'يتوفر تحديث',
    fr: 'Mise à jour disponible',
  },
  'Загрузка обновления': {
    en: 'Downloading update',
    es: 'Descargando actualización',
    ar: 'جار تنزيل التحديث',
    fr: 'Téléchargement de la mise à jour',
  },
  'Можно продолжить работу сейчас или установить новую версию оболочки.': {
    en: 'You can continue now or install the new shell version.',
    es: 'Puedes continuar ahora o instalar la nueva versión de la shell.',
    ar: 'يمكنك المتابعة الآن أو تثبيت إصدار الغلاف الجديد.',
    fr: 'Vous pouvez continuer maintenant ou installer la nouvelle version de l’enveloppe.',
  },
  'Не удалось обновить': {
    en: 'Update failed',
    es: 'No se pudo actualizar',
    ar: 'تعذر التحديث',
    fr: 'Échec de la mise à jour',
  },
  'Не удалось получить конфигурацию приложения. Проверьте сеть и повторите попытку.': {
    en: 'Could not load app configuration. Check your network and try again.',
    es: 'No se pudo cargar la configuración de la app. Comprueba la red e inténtalo de nuevo.',
    ar: 'تعذر تحميل إعدادات التطبيق. تحقق من الشبكة وحاول مرة أخرى.',
    fr: 'Impossible de charger la configuration de l’application. Vérifiez le réseau puis réessayez.',
  },
  'Обновление не найдено': {
    en: 'Update not found',
    es: 'Actualización no encontrada',
    ar: 'لم يتم العثور على تحديث',
    fr: 'Mise à jour introuvable',
  },
  'Обновление установлено. Перезапускаем приложение.': {
    en: 'Update installed. Restarting the app.',
    es: 'Actualización instalada. Reiniciando la app.',
    ar: 'تم تثبيت التحديث. جار إعادة تشغيل التطبيق.',
    fr: 'Mise à jour installée. Redémarrage de l’application.',
  },
  'Оболочка StemRed': {
    en: 'StemRed shell',
    es: 'Shell de StemRed',
    ar: 'غلاف StemRed',
    fr: 'Enveloppe StemRed',
  },
  'Обновить': {
    en: 'Update',
    es: 'Actualizar',
    ar: 'تحديث',
    fr: 'Mettre à jour',
  },
  'Открываем StemRed': {
    en: 'Opening StemRed',
    es: 'Abriendo StemRed',
    ar: 'جار فتح StemRed',
    fr: 'Ouverture de StemRed',
  },
  'Открыть приложение': {
    en: 'Open app',
    es: 'Abrir aplicación',
    ar: 'فتح التطبيق',
    fr: 'Ouvrir l’application',
  },
  'Подключение к StemRed': {
    en: 'Connecting to StemRed',
    es: 'Conectando a StemRed',
    ar: 'جار الاتصال بـ StemRed',
    fr: 'Connexion à StemRed',
  },
  'Повторить': {
    en: 'Retry',
    es: 'Reintentar',
    ar: 'إعادة المحاولة',
    fr: 'Réessayer',
  },
  'Проверяем конфигурацию сервера и версию оболочки.': {
    en: 'Checking server configuration and shell version.',
    es: 'Comprobando la configuración del servidor y la versión de la shell.',
    ar: 'جار التحقق من إعدادات الخادم وإصدار الغلاف.',
    fr: 'Vérification de la configuration du serveur et de la version de l’enveloppe.',
  },
  'Свернуть': {
    en: 'Minimize',
    es: 'Minimizar',
    ar: 'تصغير',
    fr: 'Réduire',
  },
  'Развернуть': {
    en: 'Maximize',
    es: 'Maximizar',
    ar: 'تكبير',
    fr: 'Agrandir',
  },
  'Сервер доступен. Сейчас откроется актуальная веб-версия.': {
    en: 'Server is available. The current web version will open now.',
    es: 'El servidor está disponible. Ahora se abrirá la versión web actual.',
    ar: 'الخادم متاح. سيتم فتح إصدار الويب الحالي الآن.',
    fr: 'Le serveur est disponible. La version web actuelle va s’ouvrir.',
  },
  'Сервер недоступен': {
    en: 'Server unavailable',
    es: 'Servidor no disponible',
    ar: 'الخادم غير متاح',
    fr: 'Serveur indisponible',
  },
  'Сервер недоступен. Открываем сохранённые данные.': {
    en: 'Server unavailable. Opening saved data.',
    es: 'Servidor no disponible. Abriendo datos guardados.',
    ar: 'الخادم غير متاح. جار فتح البيانات المحفوظة.',
    fr: 'Serveur indisponible. Ouverture des données enregistrées.',
  },
  'Скрыть в трей': {
    en: 'Hide to tray',
    es: 'Ocultar en bandeja',
    ar: 'إخفاء إلى علبة النظام',
    fr: 'Masquer dans la zone de notification',
  },
  'Скачиваем и устанавливаем новую версию оболочки.': {
    en: 'Downloading and installing the new shell version.',
    es: 'Descargando e instalando la nueva versión de la shell.',
    ar: 'جار تنزيل وتثبيت إصدار الغلاف الجديد.',
    fr: 'Téléchargement et installation de la nouvelle version de l’enveloppe.',
  },
  'Скачиваем обновление.': {
    en: 'Downloading update.',
    es: 'Descargando actualización.',
    ar: 'جار تنزيل التحديث.',
    fr: 'Téléchargement de la mise à jour.',
  },
  'Сервер обновлений не вернул новую версию для этой платформы.': {
    en: 'The update server did not return a new version for this platform.',
    es: 'El servidor de actualizaciones no devolvió una nueva versión para esta plataforma.',
    ar: 'لم يرجع خادم التحديثات إصدارًا جديدًا لهذه المنصة.',
    fr: 'Le serveur de mises à jour n’a renvoyé aucune nouvelle version pour cette plateforme.',
  },
  'Требуется обновление': {
    en: 'Update required',
    es: 'Actualización requerida',
    ar: 'التحديث مطلوب',
    fr: 'Mise à jour requise',
  },
  'Эта версия оболочки устарела и должна быть обновлена перед запуском.': {
    en: 'This shell version is outdated and must be updated before launch.',
    es: 'Esta versión de la shell está obsoleta y debe actualizarse antes de iniciar.',
    ar: 'إصدار الغلاف هذا قديم ويجب تحديثه قبل التشغيل.',
    fr: 'Cette version de l’enveloppe est obsolète et doit être mise à jour avant le lancement.',
  },
};

const titleEl = document.querySelector<HTMLHeadingElement>('#title')!;
const messageEl = document.querySelector<HTMLParagraphElement>('#message')!;
const detailsEl = document.querySelector<HTMLParagraphElement>('#details')!;
const progressEl = document.querySelector<HTMLDivElement>('#progress')!;
const retryButton = document.querySelector<HTMLButtonElement>('#retry')!;
const openButton = document.querySelector<HTMLButtonElement>('#open')!;
const updateButton = document.querySelector<HTMLButtonElement>('#update')!;
const safeButton = document.querySelector<HTMLButtonElement>('#safe')!;
const disableIntegrationButton = document.querySelector<HTMLButtonElement>('#disable-integration')!;
const exportDiagnosticsButton = document.querySelector<HTMLButtonElement>('#export-diagnostics')!;
const minimizeButton = document.querySelector<HTMLButtonElement>('#window-minimize')!;
const maximizeButton = document.querySelector<HTMLButtonElement>('#window-maximize')!;
const closeButton = document.querySelector<HTMLButtonElement>('#window-close')!;
const windowControls = document.querySelector<HTMLElement>('.window-controls')!;

const WINDOW_CONTROLS_HIDE_DELAY_MS = 1000;
const WINDOW_CONTROLS_REVEAL_WIDTH = 190;
const WINDOW_CONTROLS_REVEAL_HEIGHT = 110;
const FALLBACK_REMOTE_URL = 'https://chat-stem.ru/messages';
const SHELL_APP_VERSION = typeof __APP_VERSION__ === 'string' && __APP_VERSION__ ? __APP_VERSION__ : 'unknown';
const LOADING_WORDS = [
  'Загрузка',
  'Loading',
  'Cargando',
  'Chargement',
  'Wird geladen',
  'Caricamento',
  'Yukleniyor',
  'تحميل',
  '加载中',
];

let latestBootstrap: BootstrapResult | null = null;
let pendingDeepLinkUrl = '';
let windowControlsArmed = false;
let windowControlsHideTimer = 0;
let lastPointerX = Number.NaN;
let lastPointerY = Number.NaN;
let shellLocale = detectShellLocale();
let loadingWordIndex = 0;

function detectShellLocale(): ShellLocale {
  const candidates = [...(navigator.languages || []), navigator.language].filter(Boolean);
  for (const candidate of candidates) {
    const locale = String(candidate).trim().toLowerCase().replace('_', '-').split('-')[0];
    if ((SHELL_LOCALES as readonly string[]).includes(locale)) return locale as ShellLocale;
  }
  return 'en';
}

function t(source: string): string {
  if (shellLocale === 'ru') return source;
  return SHELL_TRANSLATIONS[source]?.[shellLocale] || source;
}

function applyShellLanguage() {
  document.documentElement.lang = SHELL_INTL_LOCALES[shellLocale];
  document.documentElement.dir = shellLocale === 'ar' ? 'rtl' : 'ltr';
  retryButton.textContent = t('Повторить');
  openButton.textContent = t('Открыть приложение');
  updateButton.textContent = t('Обновить');
  minimizeButton.setAttribute('aria-label', t('Свернуть'));
  maximizeButton.setAttribute('aria-label', t('Развернуть'));
  closeButton.setAttribute('aria-label', t('Скрыть в трей'));
}

function shellVersionText(version = SHELL_APP_VERSION): string {
  return `v${version || 'unknown'}`;
}

function setShellVersion(version = SHELL_APP_VERSION) {
  detailsEl.textContent = shellVersionText(version);
}

function rotateLoadingWord() {
  messageEl.classList.add('loading-word--changing');
  window.setTimeout(() => {
    loadingWordIndex = (loadingWordIndex + 1) % LOADING_WORDS.length;
    messageEl.textContent = LOADING_WORDS[loadingWordIndex];
    messageEl.classList.remove('loading-word--changing');
  }, 180);
}

function installLoadingWordRotation() {
  messageEl.textContent = LOADING_WORDS[loadingWordIndex];
  window.setInterval(rotateLoadingWord, 1400);
}

function downloadMbMessage(megabytes: number): string {
  if (shellLocale === 'ru') return `Скачиваем ${megabytes} МБ.`;
  if (shellLocale === 'es') return `Descargando ${megabytes} MB.`;
  if (shellLocale === 'ar') return `جار تنزيل ${megabytes} م.ب.`;
  if (shellLocale === 'fr') return `Téléchargement de ${megabytes} Mo.`;
  return `Downloading ${megabytes} MB.`;
}

function downloadedKbMessage(kilobytes: number): string {
  if (shellLocale === 'ru') return `Скачано ${kilobytes} КБ.`;
  if (shellLocale === 'es') return `Descargado ${kilobytes} KB.`;
  if (shellLocale === 'ar') return `تم تنزيل ${kilobytes} ك.ب.`;
  if (shellLocale === 'fr') return `${kilobytes} Ko téléchargés.`;
  return `Downloaded ${kilobytes} KB.`;
}

function setBusy(isBusy: boolean) {
  progressEl.hidden = !isBusy;
  retryButton.disabled = isBusy;
  openButton.disabled = isBusy;
  updateButton.disabled = isBusy;
  safeButton.disabled = isBusy;
  disableIntegrationButton.disabled = isBusy;
  exportDiagnosticsButton.disabled = isBusy;
}

function setButtons(
  ...visible: Array<'retry' | 'open' | 'update' | 'safe' | 'disable' | 'export'>
) {
  retryButton.hidden = !visible.includes('retry');
  openButton.hidden = !visible.includes('open');
  updateButton.hidden = !visible.includes('update');
  safeButton.hidden = !visible.includes('safe');
  disableIntegrationButton.hidden = !visible.includes('disable');
  exportDiagnosticsButton.hidden = !visible.includes('export');
}

async function installStartupUpdateIfAvailable(result: BootstrapResult): Promise<boolean> {
  try {
    const snapshot = await invoke<DesktopUpdateSnapshot>('desktop_update_snapshot');
    if (snapshot.phase === 'quarantined') {
      setButtons('safe', 'disable', 'export', 'retry');
      return true;
    }
    if (result.state === 'update_required' || snapshot.mandatory) {
      setButtons('update', 'retry');
      return true;
    }
  } catch {
    if (result.state === 'update_required') {
      setButtons('update', 'retry');
      return true;
    }
  }

  return false;
}

function isPointerNearWindowControls() {
  return (
    Number.isFinite(lastPointerX) &&
    Number.isFinite(lastPointerY) &&
    lastPointerX >= window.innerWidth - WINDOW_CONTROLS_REVEAL_WIDTH &&
    lastPointerY <= WINDOW_CONTROLS_REVEAL_HEIGHT
  );
}

function setWindowControlsVisible(visible: boolean) {
  windowControls.classList.toggle('window-controls--visible', visible);
  windowControls.classList.toggle('window-controls--hidden', !visible);
}

function showWindowControlsTemporarily() {
  window.clearTimeout(windowControlsHideTimer);
  windowControlsArmed = false;
  setWindowControlsVisible(true);
  windowControlsHideTimer = window.setTimeout(() => {
    windowControlsArmed = true;
    setWindowControlsVisible(isPointerNearWindowControls());
  }, WINDOW_CONTROLS_HIDE_DELAY_MS);
}

function installWindowControlsAutoReveal() {
  window.addEventListener(
    'pointermove',
    (event) => {
      lastPointerX = event.clientX;
      lastPointerY = event.clientY;
      if (windowControlsArmed) setWindowControlsVisible(isPointerNearWindowControls());
    },
    { passive: true }
  );
  window.addEventListener('focus', showWindowControlsTemporarily);
  window.addEventListener('pageshow', showWindowControlsTemporarily);
  document.addEventListener('visibilitychange', () => {
    if (!document.hidden) showWindowControlsTemporarily();
  });
  showWindowControlsTemporarily();
}

function describeBuild(result: BootstrapResult): string {
  return shellVersionText(result.current_shell_version);
}

async function resolveInitialDeepLink() {
  try {
    const urls = await getCurrent();
    const first = Array.isArray(urls) ? String(urls[0] || '') : '';
    if (first) pendingDeepLinkUrl = first;
  } catch {
    pendingDeepLinkUrl = '';
  }

  if (!pendingDeepLinkUrl) {
    try {
      pendingDeepLinkUrl = (await invoke<string | null>('take_pending_deep_link')) || '';
    } catch {
      pendingDeepLinkUrl = '';
    }
  }
}

async function remoteUrlFor(result: BootstrapResult): Promise<string> {
  if (!pendingDeepLinkUrl) return result.remote_url;

  try {
    const resolved = await invoke<string | null>('resolve_deep_link', { url: pendingDeepLinkUrl });
    return resolved || result.remote_url;
  } catch {
    return result.remote_url;
  }
}

async function navigateToRemote(result: BootstrapResult) {
  const url = await remoteUrlFor(result);
  navigateToRemoteUrl(url);
}

function navigateToRemoteUrl(url: string) {
  window.location.replace(url);
}

async function bootstrap() {
  setBusy(true);
  setButtons();
  titleEl.textContent = 'StemRED';
  setShellVersion();

  try {
    const result = await invoke<BootstrapResult>('bootstrap');
    latestBootstrap = result;
    detailsEl.textContent = describeBuild(result);

    if (await installStartupUpdateIfAvailable(result)) {
      return;
    }

    if (result.state === 'ready') {
      await navigateToRemote(result);
      return;
    }

    if (result.state === 'update_available') {
      await navigateToRemote(result);
      return;
    }

    if (result.state === 'update_required') {
      setButtons('update', 'retry');
      return;
    }

    setButtons('safe', 'disable', 'export', 'retry');
  } catch (error) {
    latestBootstrap = null;
    setShellVersion();
    setButtons('safe', 'disable', 'export', 'retry');
  } finally {
    setBusy(false);
  }
}

async function installUpdate() {
  setBusy(true);
  setButtons();
  titleEl.textContent = 'StemRED';
  setShellVersion(latestBootstrap?.current_shell_version || SHELL_APP_VERSION);

  try {
    const snapshot = await invoke<DesktopUpdateSnapshot>('desktop_update_request_check', {
      force: true,
    });
    if (!snapshot.install_ready) {
      setButtons(latestBootstrap?.state === 'update_required' ? 'retry' : 'open', 'retry');
      return;
    }
    await invoke('desktop_update_apply', {
      userInitiated: true,
    });
  } catch (error) {
    setButtons(latestBootstrap?.state === 'update_required' ? 'retry' : 'open', 'retry', 'update');
  } finally {
    setBusy(false);
  }
}

retryButton.addEventListener('click', () => void bootstrap());
openButton.addEventListener('click', () => {
  if (latestBootstrap) void navigateToRemote(latestBootstrap);
});
updateButton.addEventListener('click', () => void installUpdate());
safeButton.addEventListener('click', () => {
  navigateToRemoteUrl(`${FALLBACK_REMOTE_URL}?stem-recovery=lkg`);
});
disableIntegrationButton.addEventListener('click', () => {
  void invoke('set_microphone_access_enabled', { enabled: false }).then(() => {
    detailsEl.textContent = 'Микрофон отключён';
  });
});
exportDiagnosticsButton.addEventListener('click', () => {
  void invoke<string>('export_desktop_diagnostics', { includeDump: false }).then((path) => {
    detailsEl.textContent = path;
  });
});
minimizeButton.addEventListener('click', (event) => {
  event.stopPropagation();
  void invoke('minimize_desktop_window');
});
maximizeButton.addEventListener('click', (event) => {
  event.stopPropagation();
  void invoke('toggle_desktop_window_maximized');
});
closeButton.addEventListener('click', (event) => {
  event.stopPropagation();
  void invoke('close_desktop_window_to_tray');
});

window.addEventListener('online', () => void bootstrap());

void listen<string>('stem://open-url', (event) => {
  pendingDeepLinkUrl = String(event.payload || '');
  if (latestBootstrap) void navigateToRemote(latestBootstrap);
});

void onOpenUrl((urls) => {
  pendingDeepLinkUrl = String(urls[0] || '');
  if (latestBootstrap) void navigateToRemote(latestBootstrap);
});

applyShellLanguage();
setShellVersion();
installLoadingWordRotation();
installWindowControlsAutoReveal();
void resolveInitialDeepLink().then(bootstrap);
