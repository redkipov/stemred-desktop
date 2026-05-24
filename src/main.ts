import './styles.css';

import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrent, onOpenUrl } from '@tauri-apps/plugin-deep-link';
import { relaunch } from '@tauri-apps/plugin-process';
import { check } from '@tauri-apps/plugin-updater';

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

const titleEl = document.querySelector<HTMLHeadingElement>('#title')!;
const messageEl = document.querySelector<HTMLParagraphElement>('#message')!;
const detailsEl = document.querySelector<HTMLParagraphElement>('#details')!;
const progressEl = document.querySelector<HTMLDivElement>('#progress')!;
const retryButton = document.querySelector<HTMLButtonElement>('#retry')!;
const openButton = document.querySelector<HTMLButtonElement>('#open')!;
const updateButton = document.querySelector<HTMLButtonElement>('#update')!;
const minimizeButton = document.querySelector<HTMLButtonElement>('#window-minimize')!;
const maximizeButton = document.querySelector<HTMLButtonElement>('#window-maximize')!;
const closeButton = document.querySelector<HTMLButtonElement>('#window-close')!;
const windowControls = document.querySelector<HTMLElement>('.window-controls')!;

const WINDOW_CONTROLS_HIDE_DELAY_MS = 1000;
const WINDOW_CONTROLS_REVEAL_WIDTH = 190;
const WINDOW_CONTROLS_REVEAL_HEIGHT = 110;

let latestBootstrap: BootstrapResult | null = null;
let pendingDeepLinkUrl = '';
let windowControlsArmed = false;
let windowControlsHideTimer = 0;
let lastPointerX = Number.NaN;
let lastPointerY = Number.NaN;

function setBusy(isBusy: boolean) {
  progressEl.hidden = !isBusy;
  retryButton.disabled = isBusy;
  openButton.disabled = isBusy;
  updateButton.disabled = isBusy;
}

function setButtons(...visible: Array<'retry' | 'open' | 'update'>) {
  retryButton.hidden = !visible.includes('retry');
  openButton.hidden = !visible.includes('open');
  updateButton.hidden = !visible.includes('update');
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
  return [
    `Оболочка ${result.current_shell_version}`,
    `минимум ${result.min_shell_version}`,
    `рекомендовано ${result.recommended_shell_version}`,
    `web ${result.web_build_id}`,
    `api ${result.api_build_id}`,
  ].join(' · ');
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
  window.location.replace(url);
}

async function bootstrap() {
  setBusy(true);
  setButtons();
  titleEl.textContent = 'Подключение к stemred';
  messageEl.textContent = 'Проверяем конфигурацию сервера и версию оболочки.';
  detailsEl.textContent = '';

  try {
    const result = await invoke<BootstrapResult>('bootstrap');
    latestBootstrap = result;
    detailsEl.textContent = describeBuild(result);

    if (result.state === 'ready') {
      titleEl.textContent = 'Открываем stemred';
      messageEl.textContent = 'Сервер доступен. Сейчас откроется актуальная веб-версия.';
      await navigateToRemote(result);
      return;
    }

    if (result.state === 'update_available') {
      titleEl.textContent = 'Доступно обновление';
      messageEl.textContent = 'Можно продолжить работу сейчас или установить новую версию оболочки.';
      setButtons('open', 'update', 'retry');
      return;
    }

    if (result.state === 'update_required') {
      titleEl.textContent = 'Требуется обновление';
      messageEl.textContent = 'Эта версия оболочки устарела и должна быть обновлена перед запуском.';
      setButtons('update', 'retry');
      return;
    }

    titleEl.textContent = 'Сервер недоступен';
    messageEl.textContent = result.message || 'Не удалось получить конфигурацию приложения. Проверьте сеть и повторите попытку.';
    setButtons('retry');
  } catch (error) {
    latestBootstrap = null;
    titleEl.textContent = 'Сервер недоступен';
    messageEl.textContent = 'Не удалось получить конфигурацию приложения. Проверьте сеть и повторите попытку.';
    detailsEl.textContent = error instanceof Error ? error.message : String(error || '');
    setButtons('retry');
  } finally {
    setBusy(false);
  }
}

async function installUpdate() {
  setBusy(true);
  setButtons();
  titleEl.textContent = 'Загрузка обновления';
  messageEl.textContent = 'Скачиваем и устанавливаем новую версию оболочки.';

  try {
    const update = await check();
    if (!update) {
      titleEl.textContent = 'Обновление не найдено';
      messageEl.textContent = 'Сервер обновлений не вернул новую версию для этой платформы.';
      setButtons(latestBootstrap?.state === 'update_required' ? 'retry' : 'open', 'retry');
      return;
    }

    await update.downloadAndInstall((event) => {
      if (event.event === 'Started') {
        const contentLength = Number(event.data.contentLength || 0);
        messageEl.textContent = contentLength > 0 ? `Скачиваем ${Math.round(contentLength / 1024 / 1024)} МБ.` : 'Скачиваем обновление.';
      } else if (event.event === 'Progress') {
        messageEl.textContent = `Скачано ${Math.round(event.data.chunkLength / 1024)} КБ.`;
      } else if (event.event === 'Finished') {
        messageEl.textContent = 'Обновление установлено. Перезапускаем приложение.';
      }
    });

    await relaunch();
  } catch (error) {
    titleEl.textContent = 'Не удалось обновить';
    messageEl.textContent = error instanceof Error ? error.message : String(error || 'Ошибка обновления');
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

installWindowControlsAutoReveal();
void resolveInitialDeepLink().then(bootstrap);
