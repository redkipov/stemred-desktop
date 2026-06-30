use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::{engine::general_purpose, Engine as _};
use semver::Version;
use serde::{Deserialize, Serialize};
use tauri::menu::{CheckMenuItem, IsMenuItem, Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{
    AppHandle, Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent,
};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_opener::OpenerExt;
use tauri_plugin_updater::UpdaterExt;
use url::Url;

const DEFAULT_REMOTE_URL: &str = "https://chat-stem.ru/messages";
const DEFAULT_CONFIG_URL: &str = "https://chat-stem.ru/api/client/config";
const REMOTE_HOST: &str = "chat-stem.ru";
const AUTOSTART_REG_VALUE: &str = "StemRed";
const DESKTOP_NOTIFICATION_RECENT_LIMIT: usize = 128;
const DESKTOP_MUSIC_FOLDERS_FILE: &str = "music-folders.json";
const DESKTOP_MUSIC_MAX_FILES: usize = 600;
const DESKTOP_MUSIC_MAX_DEPTH: usize = 4;
#[allow(dead_code)]
const DESKTOP_CHROME_INITIALIZATION_SCRIPT: &str = r#"
(() => {
  const TITLEBAR_HEIGHT = 0;
  const STYLE_ID = 'stem-desktop-native-chrome-fix';
  const CONTROLS_ID = 'stem-native-window-controls';
  const DRAG_LAYER_ID = 'stem-native-window-drag-layer';
  const CONTROLS_OFFSET_X = 2;
  const CONTROLS_OFFSET_Y = -2;
  const CONTROLS_HIDE_DELAY_MS = 1000;
  const CONTROLS_REVEAL_WIDTH = 190;
  const CONTROLS_REVEAL_HEIGHT = 110;
  const CONTROL_SELECTOR = '.stem-native-window-button,.stem-desktop-window-button,.window-control,[data-stem-window-command]';
  const DRAG_REGION_SELECTOR = '[data-stem-desktop-drag-region],.stem-native-window-drag-zone';
  const NO_DRAG_SELECTOR = '[data-stem-desktop-no-drag],[data-tauri-drag-region="false"]';
  const INTERACTIVE_SELECTOR = 'a,button,input,select,textarea,label,summary,[contenteditable]:not([contenteditable="false"]),[role="button"],[role="link"],[role="menuitem"],[role="tab"],[role="checkbox"],[role="radio"],[role="switch"],[role="option"]';
  let patchScheduled = false;
  let controlsRevealInstalled = false;
  let controlsRevealArmed = false;
  let controlsHideTimer = 0;
  let lastPointerX = Number.NaN;
  let lastPointerY = Number.NaN;
  const css = `
    html.stem-desktop-frameless,
    html.stem-desktop-frameless body {
      width: 100% !important;
      height: 100% !important;
      overflow: hidden !important;
    }

    html.stem-desktop-frameless body {
      min-height: 100dvh !important;
      padding-top: 0 !important;
    }

    html.stem-desktop-frameless .stem-app-window-content {
      height: 100dvh !important;
      margin-top: 0 !important;
      overflow: hidden !important;
    }

    html.stem-desktop-frameless .stem-web-shell {
      overflow: hidden !important;
    }

    html.stem-desktop-frameless .stem-app-window-content .stem-web-shell {
      height: 100% !important;
      min-height: 100% !important;
    }

    html.stem-desktop-frameless body:not(:has(.stem-app-window-content)) .stem-web-shell {
      height: 100dvh !important;
      min-height: 100dvh !important;
      margin-top: 0 !important;
    }

    html.stem-desktop-frameless .stem-web-shell__viewport,
    html.stem-desktop-frameless body:not(:has(.stem-app-window-content)) .stem-web-shell > .relative {
      height: 100% !important;
      min-height: 0 !important;
    }

    html.stem-desktop-frameless .stem-app-window-content > .min-h-screen {
      min-height: 100% !important;
    }

    @supports not (height: 100dvh) {
      html.stem-desktop-frameless body {
        min-height: 100vh !important;
      }

      html.stem-desktop-frameless .stem-app-window-content {
        height: 100vh !important;
      }

      html.stem-desktop-frameless body:not(:has(.stem-app-window-content)) .stem-web-shell {
        height: 100vh !important;
        min-height: 100vh !important;
      }
    }

    .stem-desktop-titlebar {
      display: none !important;
    }

    .stem-desktop-titlebar__drag,
    .titlebar__drag {
      position: absolute !important;
      inset: 0 112px 0 0 !important;
      z-index: 0 !important;
    }

    .stem-desktop-window-controls,
    .window-controls {
      position: relative !important;
      z-index: 2 !important;
      pointer-events: auto !important;
    }

    .stem-desktop-window-button,
    .window-control,
    .stem-native-window-button,
    .stem-desktop-update-button {
      pointer-events: auto !important;
    }

    .stem-native-window-controls {
      display: flex !important;
      align-items: center !important;
      gap: 6px !important;
      pointer-events: auto !important;
      transform: translate(${CONTROLS_OFFSET_X}px, ${CONTROLS_OFFSET_Y}px) !important;
      transition: opacity 160ms ease, visibility 160ms ease !important;
    }

    .stem-native-window-controls--visible {
      opacity: 1 !important;
      visibility: visible !important;
    }

    .stem-native-window-controls--hidden {
      opacity: 0 !important;
      visibility: hidden !important;
      pointer-events: none !important;
    }

    .stem-native-window-controls--fallback {
      position: fixed !important;
      top: 0 !important;
      right: 0 !important;
      z-index: 2147483000 !important;
      border: 1px solid color-mix(in srgb, var(--stem-border, rgba(255,255,255,0.18)) 70%, transparent) !important;
      border-radius: 16px !important;
      padding: 4px !important;
      background: color-mix(in srgb, var(--stem-bg, #061014) 74%, transparent) !important;
      box-shadow: 0 14px 40px rgba(0, 0, 0, 0.28) !important;
      backdrop-filter: blur(18px) !important;
    }

    .stem-native-window-controls--inline {
      margin-left: 4px !important;
    }

    .stem-native-window-button {
      display: grid !important;
      width: 34px !important;
      height: 34px !important;
      min-height: 34px !important;
      place-items: center !important;
      border: 1px solid color-mix(in srgb, var(--stem-border, rgba(255,255,255,0.18)) 76%, transparent) !important;
      border-radius: 999px !important;
      padding: 0 !important;
      color: var(--stem-text, #edf7f7) !important;
      background: color-mix(in srgb, var(--stem-surface-soft, rgba(255,255,255,0.1)) 78%, transparent) !important;
      font: inherit !important;
      font-size: 15px !important;
      line-height: 1 !important;
      cursor: pointer !important;
    }

    .stem-desktop-update-button {
      position: relative !important;
      display: grid !important;
      width: 34px !important;
      height: 34px !important;
      min-height: 34px !important;
      place-items: center !important;
      overflow: visible !important;
      border: 1px solid color-mix(in srgb, #f59e0b 72%, var(--stem-border, rgba(255,255,255,0.18))) !important;
      border-radius: 999px !important;
      padding: 0 !important;
      color: #ffd56a !important;
      background:
        radial-gradient(circle at 34% 22%, rgba(255,255,255,0.24), transparent 32%),
        linear-gradient(135deg, #3a2606, #a66305 48%, #f59e0b) !important;
      box-shadow: 0 0 18px rgba(245, 158, 11, 0.28) !important;
      font: inherit !important;
      font-size: 18px !important;
      font-weight: 900 !important;
      line-height: 1 !important;
      cursor: pointer !important;
    }

    .stem-desktop-update-button::before {
      content: '' !important;
      position: absolute !important;
      inset: -10px 3px -15px !important;
      pointer-events: none !important;
      background-image:
        radial-gradient(circle, rgba(255, 239, 184, 0.95) 0 1px, transparent 1.4px),
        radial-gradient(circle, rgba(251, 191, 36, 0.86) 0 1.6px, transparent 2px),
        radial-gradient(circle, rgba(217, 119, 6, 0.72) 0 2.4px, transparent 3px) !important;
      background-position:
        2px 0,
        12px -36px,
        5px -44px !important;
      background-size:
        18px 28px,
        23px 36px,
        31px 44px !important;
      animation: stem-desktop-gold-dust 2.6s linear infinite !important;
    }

    .stem-desktop-update-button__icon {
      position: relative !important;
      z-index: 1 !important;
      color: #ffd56a !important;
      text-shadow:
        0 1px 0 rgba(255, 255, 255, 0.28),
        0 0 10px rgba(251, 191, 36, 0.7) !important;
      transform: translateY(-1px) !important;
    }

    .stem-desktop-update-button::after {
      content: none !important;
      display: none !important;
    }

    .stem-desktop-update-button:not([data-stem-update-count='0'])::after {
      display: none !important;
    }

    .stem-desktop-update-button[hidden] {
      display: none !important;
    }

    .stem-desktop-update-button:hover {
      filter: brightness(1.08) saturate(1.08) !important;
    }

    .stem-desktop-update-button:disabled {
      cursor: wait !important;
      opacity: 0.78 !important;
    }

    @keyframes stem-desktop-gold-dust {
      0% {
        background-position:
          2px 0,
          12px -36px,
          5px -44px;
      }
      100% {
        background-position:
          2px 28px,
          12px 0,
          5px 0;
      }
    }

    .stem-native-window-button:hover {
      border-color: color-mix(in srgb, var(--stem-cyan, #18b9a7) 48%, var(--stem-border, rgba(255,255,255,0.18))) !important;
      background: color-mix(in srgb, var(--stem-cyan, #18b9a7) 18%, var(--stem-surface-soft, rgba(255,255,255,0.1))) !important;
    }

    .stem-native-window-button--close:hover {
      border-color: color-mix(in srgb, #ff5c7a 56%, var(--stem-border, rgba(255,255,255,0.18))) !important;
      background: color-mix(in srgb, #ff5c7a 24%, var(--stem-surface-soft, rgba(255,255,255,0.1))) !important;
    }

    .stem-native-window-drag-layer {
      position: fixed !important;
      inset: 0 !important;
      z-index: 2147482500 !important;
      pointer-events: none !important;
      user-select: none !important;
    }

    .stem-native-window-drag-zone {
      position: absolute !important;
      pointer-events: auto !important;
      user-select: none !important;
    }

    .stem-native-window-drag-zone--top {
      top: 0 !important;
      right: 0 !important;
      left: 0 !important;
      height: 32px !important;
    }

    .stem-native-window-drag-zone--left {
      top: 32px !important;
      bottom: 10px !important;
      left: 0 !important;
      width: 10px !important;
    }

    .stem-native-window-drag-zone--right {
      top: 58px !important;
      right: 0 !important;
      bottom: 10px !important;
      width: 10px !important;
    }

    .stem-native-window-drag-zone--bottom {
      right: 0 !important;
      bottom: 0 !important;
      left: 0 !important;
      height: 10px !important;
    }

    .stem-native-window-obsolete {
      display: none !important;
    }
  `;

  function getInvoke() {
    const invoke = window.__TAURI_INTERNALS__?.invoke || window.__TAURI__?.core?.invoke || window.__TAURI_INVOKE__;
    return typeof invoke === 'function' ? invoke : null;
  }

  function invokeWindowPlugin(name) {
    const invoke = getInvoke();
    if (!invoke) return Promise.reject(new Error('Tauri invoke is unavailable'));
    return Promise.resolve(invoke(name, { label: 'main' })).catch(() => Promise.resolve(invoke(name, {})));
  }

  function invokeNative(command) {
    const pluginName =
      command === 'minimize'
        ? 'plugin:window|minimize'
        : command === 'toggle-maximize'
          ? 'plugin:window|toggle_maximize'
          : command === 'close-to-tray'
            ? 'plugin:window|hide'
            : '';
    const fallbackName =
      command === 'minimize'
        ? 'minimize_desktop_window'
        : command === 'toggle-maximize'
          ? 'toggle_desktop_window_maximized'
          : command === 'close-to-tray'
            ? 'close_desktop_window_to_tray'
            : '';
    if (!pluginName) return;
    invokeWindowPlugin(pluginName).catch(() => {
      const invoke = getInvoke();
      if (fallbackName && invoke) Promise.resolve(invoke(fallbackName, {})).catch(() => {});
    });
  }

  function commandFor(button) {
    const explicit = button.getAttribute('data-stem-window-command');
    if (explicit) return explicit;
    if (
      button.classList.contains('stem-native-window-button--close') ||
      button.classList.contains('stem-desktop-window-button--close') ||
      button.classList.contains('window-control--close')
    ) {
      return 'close-to-tray';
    }

    const controls = button.closest('.stem-native-window-controls,.stem-desktop-window-controls,.window-controls');
    if (!controls) return '';
    const buttons = Array.from(controls.querySelectorAll('button'));
    const index = buttons.indexOf(button);
    return index === 0 ? 'minimize' : index === 1 ? 'toggle-maximize' : index === 2 ? 'close-to-tray' : '';
  }

  function createNativeControls() {
    const controls = document.createElement('div');
    controls.id = CONTROLS_ID;
    controls.className = 'stem-native-window-controls stem-native-window-controls--visible';
    controls.innerHTML = `
      <button aria-label="Свернуть" class="stem-native-window-button" data-stem-window-command="minimize" type="button"><span aria-hidden="true">&#8722;</span></button>
      <button aria-label="Развернуть" class="stem-native-window-button" data-stem-window-command="toggle-maximize" type="button"><span aria-hidden="true">&#9633;</span></button>
      <button aria-label="Скрыть в трей" class="stem-native-window-button stem-native-window-button--close" data-stem-window-command="close-to-tray" type="button"><span aria-hidden="true">&#215;</span></button>
    `;
    return controls;
  }

  function nativeControlsElement() {
    return document.getElementById(CONTROLS_ID) || createNativeControls();
  }

  function setNativeControlsVisible(visible) {
    const controls = document.getElementById(CONTROLS_ID);
    if (!controls) return;
    controls.classList.toggle('stem-native-window-controls--visible', visible);
    controls.classList.toggle('stem-native-window-controls--hidden', !visible);
  }

  function isPointerNearControlsCorner() {
    return (
      Number.isFinite(lastPointerX) &&
      Number.isFinite(lastPointerY) &&
      lastPointerX >= window.innerWidth - CONTROLS_REVEAL_WIDTH &&
      lastPointerY <= CONTROLS_REVEAL_HEIGHT
    );
  }

  function showNativeControlsTemporarily() {
    window.clearTimeout(controlsHideTimer);
    controlsRevealArmed = false;
    setNativeControlsVisible(true);
    controlsHideTimer = window.setTimeout(() => {
      controlsRevealArmed = true;
      setNativeControlsVisible(isPointerNearControlsCorner());
    }, CONTROLS_HIDE_DELAY_MS);
  }

  function installNativeControlsAutoReveal() {
    if (controlsRevealInstalled) return;
    controlsRevealInstalled = true;
    window.addEventListener(
      'pointermove',
      (event) => {
        lastPointerX = event.clientX;
        lastPointerY = event.clientY;
        if (controlsRevealArmed) setNativeControlsVisible(isPointerNearControlsCorner());
      },
      { passive: true }
    );
    window.addEventListener('focus', showNativeControlsTemporarily);
    window.addEventListener('pageshow', showNativeControlsTemporarily);
    document.addEventListener('visibilitychange', () => {
      if (!document.hidden) showNativeControlsTemporarily();
    });
    showNativeControlsTemporarily();
  }

  function createDragLayer() {
    const layer = document.createElement('div');
    layer.id = DRAG_LAYER_ID;
    layer.className = 'stem-native-window-drag-layer';
    layer.setAttribute('aria-hidden', 'true');
    layer.innerHTML = `
      <div class="stem-native-window-drag-zone stem-native-window-drag-zone--top" data-stem-desktop-drag-region data-tauri-drag-region="deep"></div>
      <div class="stem-native-window-drag-zone stem-native-window-drag-zone--left" data-stem-desktop-drag-region data-tauri-drag-region="deep"></div>
      <div class="stem-native-window-drag-zone stem-native-window-drag-zone--right" data-stem-desktop-drag-region data-tauri-drag-region="deep"></div>
      <div class="stem-native-window-drag-zone stem-native-window-drag-zone--bottom" data-stem-desktop-drag-region data-tauri-drag-region="deep"></div>
    `;
    return layer;
  }

  function installDragLayer() {
    if (!document.body) return;
    const layer = document.getElementById(DRAG_LAYER_ID) || createDragLayer();
    if (layer.parentElement !== document.body) document.body.appendChild(layer);
  }

  function isVisible(element) {
    const rect = element.getBoundingClientRect();
    const style = window.getComputedStyle(element);
    return rect.width > 0 && rect.height > 0 && style.display !== 'none' && style.visibility !== 'hidden';
  }

  function firstVisible(selectors) {
    for (const selector of selectors) {
      for (const element of document.querySelectorAll(selector)) {
        if (element instanceof HTMLElement && isVisible(element)) return element;
      }
    }
    return null;
  }

  function findControlsHost() {
    return firstVisible([
      '[data-stem-window-controls-host]',
      '.stem-glass-panel-strong > .relative.z-30 .absolute.right-2.top-2',
      '.stem-glass-panel-strong > .relative.z-30 .absolute.right-4.top-2',
      'header.stem-topbar > div:last-child'
    ]);
  }

  function buttonLabel(button) {
    return [
      button.getAttribute('aria-label') || '',
      button.getAttribute('title') || '',
      button.textContent || ''
    ].join(' ').trim().toLowerCase();
  }

  function hideObsoleteHostButtons(host) {
    const obsolete = ['поиск', 'закреп', 'откреп', 'настрой', 'синхрон', 'обнов'];
    for (const button of host.querySelectorAll('button')) {
      if (!(button instanceof HTMLButtonElement)) continue;
      if (button.closest('.stem-native-window-controls')) continue;
      const label = buttonLabel(button);
      if (obsolete.some((word) => label.includes(word))) {
        button.classList.add('stem-native-window-obsolete');
      }
    }
  }

  function installNativeControls() {
    const existingLocalControls = firstVisible(['.window-controls']);
    if (existingLocalControls?.querySelector('.window-control')) {
      document.getElementById(CONTROLS_ID)?.remove();
      existingLocalControls.classList.add('stem-native-window-host');
      return;
    }

    const controls = nativeControlsElement();
    const host = findControlsHost();

    controls.classList.remove('stem-native-window-controls--inline', 'stem-native-window-controls--fallback');
    controls.querySelectorAll('[data-tauri-drag-region]').forEach((element) => element.removeAttribute('data-tauri-drag-region'));

    if (host) {
      hideObsoleteHostButtons(host);
      host.classList.add('stem-native-window-host');
      if (controls.parentElement !== host) host.appendChild(controls);
      controls.classList.add('stem-native-window-controls--inline');
      return;
    }

    if (controls.parentElement !== document.body) document.body.appendChild(controls);
    controls.classList.add('stem-native-window-controls--fallback');
  }

  function installDragRegions() {
    const surfaces = [
      '.stem-app-window-content',
      '.stem-web-shell',
      '.stem-web-shell__viewport'
    ];
    const blockers = [
      '.glass-subtle',
      '.stem-glass-panel',
      '.stem-glass-panel-strong',
      '.messenger-scroll',
      '.stem-message-field',
      'form'
    ];
    const candidates = [
      'header.stem-topbar',
      '.stem-glass-panel-strong > .relative.z-30',
      '.glass-subtle > aside > div:first-child'
    ];

    for (const selector of surfaces) {
      for (const element of document.querySelectorAll(selector)) {
        if (element instanceof HTMLElement && element.getAttribute('data-tauri-drag-region') !== 'deep') {
          element.setAttribute('data-stem-desktop-drag-region', '');
          element.setAttribute('data-tauri-drag-region', 'deep');
        }
      }
    }

    for (const selector of blockers) {
      for (const element of document.querySelectorAll(selector)) {
        if (element instanceof HTMLElement) {
          if (!element.hasAttribute('data-stem-desktop-no-drag')) element.setAttribute('data-stem-desktop-no-drag', '');
          if (element.getAttribute('data-tauri-drag-region') !== 'false') element.setAttribute('data-tauri-drag-region', 'false');
        }
      }
    }

    for (const selector of candidates) {
      for (const element of document.querySelectorAll(selector)) {
        if (element instanceof HTMLElement) {
          if (element.hasAttribute('data-stem-desktop-no-drag')) element.removeAttribute('data-stem-desktop-no-drag');
          if (!element.hasAttribute('data-stem-desktop-drag-region')) element.setAttribute('data-stem-desktop-drag-region', '');
          if (element.getAttribute('data-tauri-drag-region') !== 'deep') element.setAttribute('data-tauri-drag-region', 'deep');
        }
      }
    }
  }

  function installStyle() {
    if (document.getElementById(STYLE_ID)) return;
    const style = document.createElement('style');
    style.id = STYLE_ID;
    style.textContent = css;
    (document.head || document.documentElement).appendChild(style);
  }

  function ensureDragZone(titlebar, className) {
    titlebar.removeAttribute('data-tauri-drag-region');
    if (titlebar.querySelector('.stem-desktop-titlebar__drag,.titlebar__drag')) return;
    const dragZone = document.createElement('div');
    dragZone.className = className;
    dragZone.setAttribute('aria-hidden', 'true');
    dragZone.setAttribute('data-tauri-drag-region', '');
    titlebar.insertBefore(dragZone, titlebar.firstChild);
  }

  function patchChrome() {
    document.documentElement.classList.add('stem-desktop-frameless');
    document.documentElement.style.setProperty('--stem-desktop-titlebar-height', `${TITLEBAR_HEIGHT}px`);
    document.documentElement.style.setProperty('--app-top', `${TITLEBAR_HEIGHT}px`);
    document.documentElement.style.setProperty('--app-height', '100dvh');

    installStyle();
    installNativeControls();
    installNativeControlsAutoReveal();
    installDragLayer();
  }

  function schedulePatchChrome() {
    if (patchScheduled) return;
    patchScheduled = true;
    const run = () => {
      patchScheduled = false;
      patchChrome();
    };
    if (typeof window.requestAnimationFrame === 'function') window.requestAnimationFrame(run);
    else window.setTimeout(run, 16);
  }

  function eventElementPath(event) {
    const path = typeof event.composedPath === 'function' ? event.composedPath() : [];
    if (path.length) return path.filter((element) => element instanceof HTMLElement);
    const target = event.target;
    return target instanceof HTMLElement ? [target] : [];
  }

  function shouldStartWindowDrag(event) {
    if (event.defaultPrevented || event.button !== 0 || (event.detail !== 1 && event.detail !== 2)) return false;

    for (const element of eventElementPath(event)) {
      if (element.matches(CONTROL_SELECTOR)) return false;
      const hasDragRegion = element.matches(DRAG_REGION_SELECTOR) || element.getAttribute('data-tauri-drag-region') === 'deep';
      if (element.matches(INTERACTIVE_SELECTOR) && !hasDragRegion) return false;
      if (element.matches(NO_DRAG_SELECTOR)) return false;
      if (hasDragRegion) return true;
    }

    return false;
  }

  document.addEventListener(
    'mousedown',
    (event) => {
      if (!shouldStartWindowDrag(event)) return;
      event.preventDefault();
      event.stopPropagation();
      if (typeof event.stopImmediatePropagation === 'function') event.stopImmediatePropagation();
      const command = event.detail === 2 ? 'plugin:window|internal_toggle_maximize' : 'plugin:window|start_dragging';
      invokeWindowPlugin(command).catch(() => {});
    },
    true
  );

  function stopControlPress(event) {
    const target = event.target;
    if (!(target instanceof Element)) return;
    if (target.closest(CONTROL_SELECTOR)) {
      event.stopPropagation();
      if (typeof event.stopImmediatePropagation === 'function') event.stopImmediatePropagation();
    }
  }

  document.addEventListener('pointerdown', stopControlPress, true);
  document.addEventListener('mousedown', stopControlPress, true);

  document.addEventListener(
    'click',
    (event) => {
      const target = event.target;
      if (!(target instanceof Element)) return;
      const button = target.closest(CONTROL_SELECTOR);
      if (!(button instanceof HTMLButtonElement)) return;
      const command = commandFor(button);
      if (!command) return;
      event.preventDefault();
      event.stopPropagation();
      if (typeof event.stopImmediatePropagation === 'function') event.stopImmediatePropagation();
      invokeNative(command);
    },
    true
  );

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', patchChrome, { once: true });
  } else {
    patchChrome();
  }

  window.addEventListener('load', schedulePatchChrome, { once: true });
})();
"#;

struct DesktopState {
    pending_deep_link: Mutex<Option<String>>,
    unread_count: Mutex<u32>,
    microphone_access_enabled: Mutex<bool>,
    recent_notifications: Mutex<Vec<(String, u128)>>,
    pending_downloads: Mutex<HashMap<String, DesktopPendingDownload>>,
}

impl Default for DesktopState {
    fn default() -> Self {
        Self {
            pending_deep_link: Mutex::new(None),
            unread_count: Mutex::new(0),
            microphone_access_enabled: Mutex::new(true),
            recent_notifications: Mutex::new(Vec::new()),
            pending_downloads: Mutex::new(HashMap::new()),
        }
    }
}

#[derive(Debug, Deserialize)]
struct ClientConfig {
    remote_url: String,
    web_build_id: String,
    api_build_id: String,
    min_shell_version: String,
    recommended_shell_version: String,
}

#[derive(Debug, Serialize)]
struct BootstrapResult {
    state: String,
    remote_url: String,
    current_shell_version: String,
    min_shell_version: String,
    recommended_shell_version: String,
    web_build_id: String,
    api_build_id: String,
    message: Option<String>,
}

#[derive(Debug, Serialize)]
struct DesktopShellUpdateStatus {
    available: bool,
    current_version: String,
    version: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DesktopNotificationRequest {
    title: String,
    body: Option<String>,
    dedupe_key: Option<String>,
    dedupe_ms: Option<u64>,
    sound: Option<String>,
}

#[derive(Debug, Serialize)]
struct DesktopDownloadSaveResult {
    filename: String,
    directory: String,
    path: String,
}

#[derive(Debug, Serialize)]
struct DesktopDownloadBeginResult {
    transfer_id: String,
    filename: String,
    directory: String,
    path: String,
}

#[derive(Debug, Clone)]
struct DesktopPendingDownload {
    filename: String,
    directory: PathBuf,
    path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DesktopMusicDirectory {
    id: String,
    name: String,
    path: String,
}

#[derive(Debug, Clone, Serialize)]
struct DesktopMusicTrack {
    id: String,
    title: String,
    filename: String,
    path: String,
    relative_path: String,
    mime: String,
    size: u64,
    modified_ms: u64,
}

#[derive(Debug, Serialize)]
struct DesktopMusicScanResult {
    folder: DesktopMusicDirectory,
    tracks: Vec<DesktopMusicTrack>,
    truncated: bool,
}

#[derive(Debug, Serialize)]
struct DesktopMusicFile {
    filename: String,
    mime: String,
    size: u64,
    modified_ms: u64,
    content_base64: String,
}

#[derive(Debug, Clone, Serialize)]
struct MicrophoneAccessChanged {
    enabled: bool,
}

#[derive(Debug, Serialize)]
struct DesktopActivitySnapshot {
    idle_seconds: u64,
    active_app_category: Option<String>,
}

#[tauri::command]
async fn bootstrap(app: AppHandle) -> Result<BootstrapResult, String> {
    let current_shell_version = app.package_info().version.to_string();

    match fetch_client_config().await {
        Ok(config) => {
            let state = update_state(
                &current_shell_version,
                &config.min_shell_version,
                &config.recommended_shell_version,
            );

            Ok(BootstrapResult {
                state,
                remote_url: normalize_remote_url(&config.remote_url),
                current_shell_version,
                min_shell_version: config.min_shell_version,
                recommended_shell_version: config.recommended_shell_version,
                web_build_id: config.web_build_id,
                api_build_id: config.api_build_id,
                message: None,
            })
        }
        Err(error) => {
            if remote_is_available(DEFAULT_REMOTE_URL).await {
                Ok(BootstrapResult {
                    state: "ready".to_string(),
                    remote_url: DEFAULT_REMOTE_URL.to_string(),
                    current_shell_version,
                    min_shell_version: "0.1.0".to_string(),
                    recommended_shell_version: "0.1.0".to_string(),
                    web_build_id: "unknown".to_string(),
                    api_build_id: "unknown".to_string(),
                    message: Some(format!(
                        "Config endpoint unavailable, opening default domain: {error}"
                    )),
                })
            } else {
                Ok(BootstrapResult {
                    state: "offline".to_string(),
                    remote_url: DEFAULT_REMOTE_URL.to_string(),
                    current_shell_version,
                    min_shell_version: "0.1.0".to_string(),
                    recommended_shell_version: "0.1.0".to_string(),
                    web_build_id: "unknown".to_string(),
                    api_build_id: "unknown".to_string(),
                    message: Some(error),
                })
            }
        }
    }
}

#[tauri::command]
fn resolve_deep_link(url: String) -> Option<String> {
    resolve_stem_deep_link(&url, DEFAULT_REMOTE_URL)
}

#[tauri::command]
fn take_pending_deep_link(state: tauri::State<'_, DesktopState>) -> Option<String> {
    state
        .pending_deep_link
        .lock()
        .ok()
        .and_then(|mut pending| pending.take())
}

#[tauri::command]
fn open_external_url(app: AppHandle, url: String) -> Result<(), String> {
    let parsed = Url::parse(&url).map_err(|_| "invalid external URL".to_string())?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err("only http and https URLs can be opened externally".to_string());
    }

    app.opener()
        .open_url(url, None::<String>)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn set_unread_count(
    app: AppHandle,
    state: tauri::State<'_, DesktopState>,
    count: u32,
) -> Result<(), String> {
    {
        let mut unread = state
            .unread_count
            .lock()
            .map_err(|_| "unread badge state is unavailable".to_string())?;
        *unread = count;
    }

    if let Some(tray) = app.tray_by_id("stem-main") {
        let tooltip = if count == 0 {
            "StemRed".to_string()
        } else {
            format!("StemRed - {} unread", count)
        };
        tray.set_tooltip(Some(&tooltip))
            .map_err(|error| error.to_string())?;
    }

    Ok(())
}

#[tauri::command]
fn get_autostart_enabled(app: AppHandle) -> Result<bool, String> {
    platform_autostart::is_enabled(&app)
}

#[tauri::command]
fn set_autostart_enabled(app: AppHandle, enabled: bool) -> Result<bool, String> {
    platform_autostart::set_enabled(&app, enabled)?;
    let enabled = platform_autostart::is_enabled(&app)?;
    let _ = refresh_tray_menu(&app);
    Ok(enabled)
}

#[tauri::command]
fn get_microphone_access_enabled(state: tauri::State<'_, DesktopState>) -> Result<bool, String> {
    microphone_access_enabled(&state)
}

#[tauri::command]
fn set_microphone_access_enabled(app: AppHandle, enabled: bool) -> Result<bool, String> {
    set_microphone_access(&app, enabled)
}

#[tauri::command]
fn get_desktop_activity_snapshot() -> Result<DesktopActivitySnapshot, String> {
    platform_activity::snapshot()
}

#[tauri::command]
fn minimize_desktop_window(app: AppHandle) -> Result<(), String> {
    main_window(&app)?
        .minimize()
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn toggle_desktop_window_maximized(app: AppHandle) -> Result<bool, String> {
    let window = main_window(&app)?;
    if window.is_maximized().map_err(|error| error.to_string())? {
        window.unmaximize().map_err(|error| error.to_string())?;
    } else {
        window.maximize().map_err(|error| error.to_string())?;
    }

    window.is_maximized().map_err(|error| error.to_string())
}

#[tauri::command]
fn close_desktop_window_to_tray(app: AppHandle) -> Result<(), String> {
    main_window(&app)?.hide().map_err(|error| error.to_string())
}

#[tauri::command]
async fn check_desktop_shell_update(app: AppHandle) -> Result<DesktopShellUpdateStatus, String> {
    let current_version = app.package_info().version.to_string();
    let update = app
        .updater()
        .map_err(|error| error.to_string())?
        .check()
        .await
        .map_err(|error| error.to_string())?;

    Ok(match update {
        Some(update) => DesktopShellUpdateStatus {
            available: true,
            current_version: update.current_version,
            version: Some(update.version),
        },
        None => DesktopShellUpdateStatus {
            available: false,
            current_version,
            version: None,
        },
    })
}

#[tauri::command]
async fn install_desktop_shell_update(app: AppHandle) -> Result<bool, String> {
    let update = app
        .updater()
        .map_err(|error| error.to_string())?
        .check()
        .await
        .map_err(|error| error.to_string())?;

    let Some(update) = update else {
        return Ok(false);
    };

    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(|error| error.to_string())?;
    app.restart()
}

#[tauri::command]
fn show_desktop_notification(
    app: AppHandle,
    state: tauri::State<'_, DesktopState>,
    payload: DesktopNotificationRequest,
) -> Result<(), String> {
    let title = payload.title.trim();
    if title.is_empty() {
        return Ok(());
    }
    if is_duplicate_desktop_notification(&state, payload.dedupe_key.as_deref(), payload.dedupe_ms)?
    {
        return Ok(());
    }

    let mut notification = app
        .notification()
        .builder()
        .title(title.chars().take(80).collect::<String>());
    if let Some(body) = payload.body {
        let body = body.trim();
        if !body.is_empty() {
            notification = notification.body(body.chars().take(240).collect::<String>());
        }
    }
    if let Some(sound) = payload.sound {
        let sound = sound.trim();
        if !sound.is_empty() {
            notification = notification.sound(sound.chars().take(260).collect::<String>());
        }
    }

    notification.show().map_err(|error| error.to_string())
}

#[tauri::command]
fn save_file_to_downloads_stem(
    app: AppHandle,
    filename: String,
    content_base64: String,
) -> Result<DesktopDownloadSaveResult, String> {
    let bytes = general_purpose::STANDARD
        .decode(content_base64.trim())
        .map_err(|error| format!("Не удалось прочитать файл: {error}"))?;
    if bytes.is_empty() {
        return Err("Файл пустой".to_string());
    }

    let directory = stemred_download_directory(&app)?;
    let safe_filename = sanitize_download_filename(&filename);
    let target = unique_download_path(&directory, &safe_filename);
    fs::write(&target, bytes).map_err(|error| format!("Не удалось сохранить файл: {error}"))?;

    Ok(desktop_download_result(&directory, &target, &safe_filename))
}

#[tauri::command]
fn begin_file_save_to_downloads_stemred(
    app: AppHandle,
    state: tauri::State<'_, DesktopState>,
    filename: String,
) -> Result<DesktopDownloadBeginResult, String> {
    let directory = stemred_download_directory(&app)?;
    let safe_filename = sanitize_download_filename(&filename);
    let target = unique_download_path(&directory, &safe_filename);
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&target)
        .map_err(|error| format!("Не удалось создать файл: {error}"))?;

    let transfer_id = format!(
        "{}-{}",
        system_time_ms(Some(SystemTime::now())),
        stable_hash(target.to_string_lossy().as_bytes())
    );
    let result = desktop_download_result(&directory, &target, &safe_filename);
    let pending = DesktopPendingDownload {
        filename: result.filename.clone(),
        directory,
        path: target,
    };
    state
        .pending_downloads
        .lock()
        .map_err(|_| "Не удалось подготовить сохранение файла".to_string())?
        .insert(transfer_id.clone(), pending);

    Ok(DesktopDownloadBeginResult {
        transfer_id,
        filename: result.filename,
        directory: result.directory,
        path: result.path,
    })
}

#[tauri::command]
fn write_file_save_chunk_to_downloads_stemred(
    state: tauri::State<'_, DesktopState>,
    transfer_id: String,
    chunk_base64: String,
) -> Result<(), String> {
    let transfer_id = transfer_id.trim();
    if transfer_id.is_empty() {
        return Err("Не найден идентификатор сохранения".to_string());
    }
    let path = state
        .pending_downloads
        .lock()
        .map_err(|_| "Не удалось продолжить сохранение файла".to_string())?
        .get(transfer_id)
        .map(|pending| pending.path.clone())
        .ok_or_else(|| "Сохранение файла уже не активно".to_string())?;

    let bytes = general_purpose::STANDARD
        .decode(chunk_base64.trim())
        .map_err(|error| format!("Не удалось прочитать часть файла: {error}"))?;
    if bytes.is_empty() {
        return Ok(());
    }

    let mut file = OpenOptions::new()
        .append(true)
        .open(&path)
        .map_err(|error| format!("Не удалось открыть файл для записи: {error}"))?;
    file.write_all(&bytes)
        .map_err(|error| format!("Не удалось записать часть файла: {error}"))
}

#[tauri::command]
fn finish_file_save_to_downloads_stemred(
    state: tauri::State<'_, DesktopState>,
    transfer_id: String,
) -> Result<DesktopDownloadSaveResult, String> {
    let transfer_id = transfer_id.trim();
    if transfer_id.is_empty() {
        return Err("Не найден идентификатор сохранения".to_string());
    }
    let pending = state
        .pending_downloads
        .lock()
        .map_err(|_| "Не удалось завершить сохранение файла".to_string())?
        .remove(transfer_id)
        .ok_or_else(|| "Сохранение файла уже не активно".to_string())?;

    let size = fs::metadata(&pending.path)
        .map_err(|error| format!("Не удалось проверить сохранённый файл: {error}"))?
        .len();
    if size == 0 {
        let _ = fs::remove_file(&pending.path);
        return Err("Файл пустой".to_string());
    }

    Ok(desktop_download_result(
        &pending.directory,
        &pending.path,
        &pending.filename,
    ))
}

#[tauri::command]
fn cancel_file_save_to_downloads_stemred(
    state: tauri::State<'_, DesktopState>,
    transfer_id: String,
) -> Result<(), String> {
    let transfer_id = transfer_id.trim();
    if transfer_id.is_empty() {
        return Ok(());
    }
    let pending = state
        .pending_downloads
        .lock()
        .map_err(|_| "Не удалось отменить сохранение файла".to_string())?
        .remove(transfer_id);
    if let Some(pending) = pending {
        let _ = fs::remove_file(pending.path);
    }
    Ok(())
}

#[tauri::command]
fn find_file_in_downloads_stemred(
    app: AppHandle,
    filename: String,
    size: u64,
) -> Result<Option<DesktopDownloadSaveResult>, String> {
    if size == 0 {
        return Ok(None);
    }

    let directory = stemred_download_directory(&app)?;
    let safe_filename = sanitize_download_filename(&filename);
    let target = directory.join(&safe_filename);
    let metadata = match fs::metadata(&target) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("Не удалось проверить файл: {error}")),
    };
    if !metadata.is_file() || metadata.len() != size {
        return Ok(None);
    }

    Ok(Some(desktop_download_result(
        &directory,
        &target,
        &safe_filename,
    )))
}

#[tauri::command]
fn desktop_path_exists(path: String) -> bool {
    let value = path.trim();
    if value.is_empty() {
        return false;
    }

    Path::new(value).exists()
}

#[tauri::command]
fn open_downloaded_file(app: AppHandle, path: String) -> Result<(), String> {
    let value = path.trim();
    if value.is_empty() {
        return Err("Не указан путь к файлу".to_string());
    }

    let target = fs::canonicalize(Path::new(value))
        .map_err(|error| format!("Файл недоступен: {error}"))?;
    if !target.is_file() {
        return Err("Можно открыть только файл".to_string());
    }

    let directory = fs::canonicalize(stemred_download_directory(&app)?)
        .map_err(|error| format!("Папка загрузок недоступна: {error}"))?;
    if !target.starts_with(&directory) {
        return Err("Файл находится вне папки загрузок StemRed".to_string());
    }

    app.opener()
        .open_path(target.to_string_lossy().to_string(), None::<String>)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn pick_music_directory(app: AppHandle) -> Result<Option<DesktopMusicDirectory>, String> {
    let Some(path) = app.dialog().file().blocking_pick_folder() else {
        return Ok(None);
    };
    let path = path
        .into_path()
        .map_err(|_| "Можно выбрать только локальную папку Windows".to_string())?;
    let path = canonicalize_existing_dir(&path)?;
    remember_music_directory(&app, &path)?;
    Ok(Some(desktop_music_directory(&path)))
}

#[tauri::command]
fn scan_music_directory(app: AppHandle, path: String) -> Result<DesktopMusicScanResult, String> {
    let path = canonicalize_existing_dir(Path::new(path.trim()))?;
    ensure_music_directory_allowed(&app, &path)?;

    let mut tracks = Vec::new();
    let mut truncated = false;
    collect_desktop_music_tracks(&path, &path, 0, &mut tracks, &mut truncated)?;
    tracks.sort_by(|left, right| left.title.to_lowercase().cmp(&right.title.to_lowercase()));

    Ok(DesktopMusicScanResult {
        folder: desktop_music_directory(&path),
        tracks,
        truncated,
    })
}

#[tauri::command]
fn read_music_file(app: AppHandle, path: String) -> Result<DesktopMusicFile, String> {
    let path = fs::canonicalize(Path::new(path.trim()))
        .map_err(|error| format!("Музыкальный файл недоступен: {error}"))?;
    ensure_music_file_allowed(&app, &path)?;
    if !is_desktop_music_file(&path) {
        return Err("Файл не является поддерживаемым аудио".to_string());
    }

    let metadata = fs::metadata(&path)
        .map_err(|error| format!("Не удалось прочитать музыкальный файл: {error}"))?;
    if !metadata.is_file() {
        return Err("Это не файл".to_string());
    }

    let bytes =
        fs::read(&path).map_err(|error| format!("Не удалось открыть музыкальный файл: {error}"))?;
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("music")
        .to_string();

    Ok(DesktopMusicFile {
        filename,
        mime: desktop_music_mime(&path).to_string(),
        size: metadata.len(),
        modified_ms: system_time_ms(metadata.modified().ok()),
        content_base64: general_purpose::STANDARD.encode(bytes),
    })
}

fn canonicalize_existing_dir(path: &Path) -> Result<PathBuf, String> {
    let path =
        fs::canonicalize(path).map_err(|error| format!("Локальная папка недоступна: {error}"))?;
    if path.is_dir() {
        Ok(path)
    } else {
        Err("Выбранный путь не является папкой".to_string())
    }
}

fn desktop_music_directory(path: &Path) -> DesktopMusicDirectory {
    DesktopMusicDirectory {
        id: desktop_music_directory_id(path),
        name: path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Локальная музыка")
            .to_string(),
        path: path.to_string_lossy().to_string(),
    }
}

fn desktop_music_directory_id(path: &Path) -> String {
    format!(
        "desktop-folder:{}",
        stable_hash(path.to_string_lossy().as_bytes())
    )
}

fn remember_music_directory(app: &AppHandle, path: &Path) -> Result<(), String> {
    let mut folders = read_remembered_music_directories(app);
    let path_string = path.to_string_lossy().to_string();
    if !folders.iter().any(|folder| folder.path == path_string) {
        folders.push(desktop_music_directory(path));
    }
    write_remembered_music_directories(app, &folders)
}

fn read_remembered_music_directories(app: &AppHandle) -> Vec<DesktopMusicDirectory> {
    let Ok(path) = music_directories_config_path(app) else {
        return Vec::new();
    };
    let Ok(raw) = fs::read_to_string(path) else {
        return Vec::new();
    };
    serde_json::from_str::<Vec<DesktopMusicDirectory>>(&raw).unwrap_or_default()
}

fn write_remembered_music_directories(
    app: &AppHandle,
    folders: &[DesktopMusicDirectory],
) -> Result<(), String> {
    let path = music_directories_config_path(app)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("Не удалось сохранить список папок: {error}"))?;
    }
    let raw = serde_json::to_string_pretty(folders)
        .map_err(|error| format!("Не удалось сериализовать список папок: {error}"))?;
    fs::write(path, raw).map_err(|error| format!("Не удалось записать список папок: {error}"))
}

fn music_directories_config_path(app: &AppHandle) -> Result<PathBuf, String> {
    let mut dir = app
        .path()
        .app_config_dir()
        .map_err(|error| format!("Не удалось найти config-папку приложения: {error}"))?;
    dir.push(DESKTOP_MUSIC_FOLDERS_FILE);
    Ok(dir)
}

fn ensure_music_directory_allowed(app: &AppHandle, path: &Path) -> Result<(), String> {
    let allowed = read_remembered_music_directories(app)
        .into_iter()
        .filter_map(|folder| fs::canonicalize(folder.path).ok())
        .any(|folder| path.starts_with(folder));

    if allowed {
        Ok(())
    } else {
        Err("Папка не была выбрана в Windows-приложении".to_string())
    }
}

fn ensure_music_file_allowed(app: &AppHandle, path: &Path) -> Result<(), String> {
    let allowed = read_remembered_music_directories(app)
        .into_iter()
        .filter_map(|folder| fs::canonicalize(folder.path).ok())
        .any(|folder| path.starts_with(folder));

    if allowed {
        Ok(())
    } else {
        Err("Файл находится вне выбранных музыкальных папок".to_string())
    }
}

fn collect_desktop_music_tracks(
    root: &Path,
    directory: &Path,
    depth: usize,
    tracks: &mut Vec<DesktopMusicTrack>,
    truncated: &mut bool,
) -> Result<(), String> {
    if depth > DESKTOP_MUSIC_MAX_DEPTH || tracks.len() >= DESKTOP_MUSIC_MAX_FILES {
        *truncated = true;
        return Ok(());
    }

    let entries = fs::read_dir(directory)
        .map_err(|error| format!("Не удалось прочитать папку с музыкой: {error}"))?;
    for entry in entries {
        if tracks.len() >= DESKTOP_MUSIC_MAX_FILES {
            *truncated = true;
            break;
        }

        let entry = entry.map_err(|error| format!("Не удалось прочитать файл: {error}"))?;
        let path = entry.path();
        let metadata = entry
            .metadata()
            .map_err(|error| format!("Не удалось прочитать metadata файла: {error}"))?;

        if metadata.is_dir() {
            collect_desktop_music_tracks(root, &path, depth + 1, tracks, truncated)?;
            continue;
        }

        if !metadata.is_file() || !is_desktop_music_file(&path) {
            continue;
        }

        let filename = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("music")
            .to_string();
        let relative_path = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        let modified_ms = system_time_ms(metadata.modified().ok());

        tracks.push(DesktopMusicTrack {
            id: format!(
                "desktop-track:{}:{}:{}",
                stable_hash(path.to_string_lossy().as_bytes()),
                metadata.len(),
                modified_ms
            ),
            title: desktop_music_title(&filename),
            filename,
            path: path.to_string_lossy().to_string(),
            relative_path,
            mime: desktop_music_mime(&path).to_string(),
            size: metadata.len(),
            modified_ms,
        });
    }

    Ok(())
}

fn is_desktop_music_file(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase())
            .as_deref(),
        Some("aac" | "flac" | "m4a" | "mp3" | "ogg" | "opus" | "wav")
    )
}

fn desktop_music_mime(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .as_deref()
    {
        Some("aac") => "audio/aac",
        Some("flac") => "audio/flac",
        Some("m4a") => "audio/mp4",
        Some("ogg" | "opus") => "audio/ogg",
        Some("wav") => "audio/wav",
        _ => "audio/mpeg",
    }
}

fn desktop_music_title(filename: &str) -> String {
    Path::new(filename)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.trim().is_empty())
        .unwrap_or(filename)
        .to_string()
}

fn system_time_ms(time: Option<SystemTime>) -> u64 {
    time.and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(0)
}

fn stable_hash(bytes: &[u8]) -> String {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

fn stemred_download_directory(app: &AppHandle) -> Result<PathBuf, String> {
    let mut directory = app
        .path()
        .download_dir()
        .map_err(|error| format!("Не удалось найти папку загрузок Windows: {error}"))?;
    directory.push("StemRed");
    fs::create_dir_all(&directory)
        .map_err(|error| format!("Не удалось создать папку StemRed: {error}"))?;
    Ok(directory)
}

fn desktop_download_result(
    directory: &Path,
    target: &Path,
    fallback_filename: &str,
) -> DesktopDownloadSaveResult {
    DesktopDownloadSaveResult {
        filename: target
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(fallback_filename)
            .to_string(),
        directory: directory.to_string_lossy().to_string(),
        path: target.to_string_lossy().to_string(),
    }
}

fn sanitize_download_filename(filename: &str) -> String {
    let cleaned = filename
        .trim()
        .chars()
        .map(|ch| match ch {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            ch if ch.is_control() => '_',
            ch => ch,
        })
        .collect::<String>()
        .trim_matches(['.', ' '])
        .to_string();

    let fallback = if cleaned.is_empty() {
        "stem-file".to_string()
    } else {
        cleaned
    };

    let reserved = [
        "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
        "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
    ];
    let stem = Path::new(&fallback)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or(&fallback)
        .to_ascii_lowercase();

    if reserved.contains(&stem.as_str()) {
        format!("_{fallback}")
    } else {
        fallback
    }
}

fn unique_download_path(directory: &Path, filename: &str) -> PathBuf {
    let candidate = directory.join(filename);
    if !candidate.exists() {
        return candidate;
    }

    let path = Path::new(filename);
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("stem-file");
    let extension = path.extension().and_then(|value| value.to_str());

    for index in 1..10_000 {
        let next_filename = match extension {
            Some(extension) if !extension.is_empty() => {
                format!("{stem} ({index}).{extension}")
            }
            _ => format!("{stem} ({index})"),
        };
        let next = directory.join(next_filename);
        if !next.exists() {
            return next;
        }
    }

    directory.join(format!("{stem}-{}", desktop_start_counter()))
}

async fn fetch_client_config() -> Result<ClientConfig, String> {
    let config_url = option_env!("STEM_CLIENT_CONFIG_URL").unwrap_or(DEFAULT_CONFIG_URL);
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(8))
        .build()
        .map_err(|error| error.to_string())?;

    let response = client
        .get(config_url)
        .send()
        .await
        .map_err(|error| format!("Сервер конфигурации недоступен: {error}"))?;

    if !response.status().is_success() {
        return Err(format!(
            "Сервер конфигурации вернул HTTP {}",
            response.status()
        ));
    }

    response
        .json::<ClientConfig>()
        .await
        .map_err(|error| format!("Не удалось прочитать конфигурацию: {error}"))
}

async fn remote_is_available(url: &str) -> bool {
    let Ok(client) = reqwest::Client::builder()
        .timeout(Duration::from_secs(8))
        .build()
    else {
        return false;
    };

    client
        .get(url)
        .send()
        .await
        .map(|response| response.status().is_success() || response.status().is_redirection())
        .unwrap_or(false)
}

fn update_state(current: &str, min_shell: &str, recommended_shell: &str) -> String {
    let current = parse_version(current);
    let min_shell = parse_version(min_shell);
    let recommended_shell = parse_version(recommended_shell);

    if current < min_shell {
        "update_required".to_string()
    } else if current < recommended_shell {
        "update_available".to_string()
    } else {
        "ready".to_string()
    }
}

fn parse_version(value: &str) -> Version {
    Version::parse(value.trim().trim_start_matches('v')).unwrap_or_else(|_| Version::new(0, 0, 0))
}

fn normalize_remote_url(value: &str) -> String {
    let trimmed = value.trim();
    if is_allowed_url(trimmed) {
        trimmed.to_string()
    } else {
        DEFAULT_REMOTE_URL.to_string()
    }
}

fn is_allowed_url(value: &str) -> bool {
    Url::parse(value)
        .map(|url| is_allowed_navigation_url(&url))
        .unwrap_or(false)
}

fn is_allowed_navigation_url(url: &Url) -> bool {
    match url.scheme() {
        "https" | "wss" => url.host_str() == Some(REMOTE_HOST),
        "http" | "ws" if cfg!(debug_assertions) => {
            matches!(url.host_str(), Some("localhost") | Some("127.0.0.1"))
                && matches!(url.port(), Some(3010 | 4000 | 1420))
        }
        "tauri" => true,
        _ => false,
    }
}

fn resolve_stem_deep_link(raw: &str, default_remote_url: &str) -> Option<String> {
    let deep_link = Url::parse(raw).ok()?;
    if deep_link.scheme() != "stem" {
        return None;
    }

    let base = Url::parse(default_remote_url).ok()?;
    let host = deep_link.host_str().unwrap_or("");
    let path = deep_link.path().trim_matches('/');

    let mut target = base;
    target.set_path("/messages");
    target.set_query(None);

    match host {
        "messages" => {
            if let Some(query) = deep_link.query() {
                target.set_query(Some(query));
            }
        }
        "chat" => {
            let id = path.split('/').next().unwrap_or("");
            if id.parse::<u64>().is_ok() {
                target.set_query(Some(&format!("user={id}")));
            }
        }
        "room" => {
            let id = path.split('/').next().unwrap_or("");
            if id.parse::<u64>().is_ok() {
                target.set_query(Some(&format!("room={id}")));
            }
        }
        "auth" if matches!(path, "social/callback" | "social/complete") => {
            target.set_path(&format!("/auth/{path}"));
            if let Some(query) = deep_link.query() {
                target.set_query(Some(query));
            }
        }
        _ => return None,
    }

    Some(target.to_string())
}

fn create_main_window(app: &mut tauri::App) -> tauri::Result<WebviewWindow> {
    let args: Vec<String> = std::env::args().collect();
    let initial_url = initial_remote_url(&args);

    WebviewWindowBuilder::new(app, "main", WebviewUrl::External(initial_url))
        .title("StemRed")
        .inner_size(1280.0, 820.0)
        .min_inner_size(390.0, 560.0)
        .resizable(true)
        .decorations(false)
        .shadow(true)
        .disable_drag_drop_handler()
        .initialization_script("document.documentElement.classList.add('stem-desktop-frameless');")
        .on_navigation(|url| is_allowed_navigation_url(url))
        .build()
}

fn initial_remote_url(args: &[String]) -> Url {
    let target = capture_argv_deep_link(args)
        .and_then(|raw| resolve_stem_deep_link(&raw, DEFAULT_REMOTE_URL))
        .unwrap_or_else(|| DEFAULT_REMOTE_URL.to_string());

    let mut url = Url::parse(&target).unwrap_or_else(|_| {
        Url::parse(DEFAULT_REMOTE_URL).expect("default remote URL must be valid")
    });
    url.query_pairs_mut()
        .append_pair("_stem_desktop_shell", env!("CARGO_PKG_VERSION"))
        .append_pair("_stem_desktop_start", &desktop_start_counter());
    url
}

fn desktop_start_counter() -> String {
    desktop_now_millis().to_string()
}

fn desktop_now_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

fn is_duplicate_desktop_notification(
    state: &tauri::State<'_, DesktopState>,
    dedupe_key: Option<&str>,
    dedupe_ms: Option<u64>,
) -> Result<bool, String> {
    let Some(key) = dedupe_key.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(false);
    };
    let window_ms = u128::from(dedupe_ms.unwrap_or(15_000).max(1_000));
    let now = desktop_now_millis();
    let mut recent = state
        .recent_notifications
        .lock()
        .map_err(|_| "notification dedupe state is unavailable".to_string())?;

    recent.retain(|(_, seen_at)| now.saturating_sub(*seen_at) <= window_ms);
    if recent.iter().any(|(seen_key, _)| seen_key == key) {
        return Ok(true);
    }

    recent.push((key.to_string(), now));
    if recent.len() > DESKTOP_NOTIFICATION_RECENT_LIMIT {
        let overflow = recent.len() - DESKTOP_NOTIFICATION_RECENT_LIMIT;
        recent.drain(0..overflow);
    }

    Ok(false)
}

fn create_tray(app: &mut tauri::App) -> tauri::Result<()> {
    let app_handle = app.handle().clone();
    let menu = build_tray_menu(&app_handle)?;

    let mut tray = TrayIconBuilder::with_id("stem-main")
        .tooltip("StemRed")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| match event {
            TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            }
            | TrayIconEvent::DoubleClick {
                button: MouseButton::Left,
                ..
            } => show_main_window(tray.app_handle()),
            _ => {}
        })
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show_main_window(app),
            "retry" => retry_main_window(app),
            "microphone_access" => {
                let next_enabled = app
                    .try_state::<DesktopState>()
                    .and_then(|state| microphone_access_enabled(&state).ok())
                    .map(|enabled| !enabled)
                    .unwrap_or(true);
                let _ = set_microphone_access(app, next_enabled);
            }
            "autostart" => {
                let next_enabled = !platform_autostart::is_enabled(app).unwrap_or(false);
                if platform_autostart::set_enabled(app, next_enabled).is_ok() {
                    let _ = refresh_tray_menu(app);
                }
            }
            "quit" => app.exit(0),
            _ => {}
        });

    if let Some(icon) = app.default_window_icon().cloned() {
        tray = tray.icon(icon);
    }

    tray.build(app)?;

    Ok(())
}

fn build_tray_menu(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let microphone_enabled = app
        .try_state::<DesktopState>()
        .and_then(|state| microphone_access_enabled(&state).ok())
        .unwrap_or(true);
    let autostart_enabled = platform_autostart::is_enabled(app).unwrap_or(false);

    let open = MenuItem::with_id(app, "open", "Открыть StemRed", true, None::<&str>)?;
    let retry = MenuItem::with_id(app, "retry", "Повторить подключение", true, None::<&str>)?;
    let microphone = CheckMenuItem::with_id(
        app,
        "microphone_access",
        "Микрофон разрешён",
        true,
        microphone_enabled,
        None::<&str>,
    )?;
    let autostart = CheckMenuItem::with_id(
        app,
        "autostart",
        "Запускать с Windows",
        true,
        autostart_enabled,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, "quit", "Выход", true, None::<&str>)?;

    let items: [&dyn IsMenuItem<tauri::Wry>; 5] = [&open, &retry, &microphone, &autostart, &quit];
    Menu::with_items(app, &items)
}

fn refresh_tray_menu(app: &AppHandle) -> Result<(), String> {
    let menu = build_tray_menu(app).map_err(|error| error.to_string())?;
    if let Some(tray) = app.tray_by_id("stem-main") {
        tray.set_menu(Some(menu))
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn retry_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.eval("window.location.replace('https://chat-stem.ru/')");
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn main_window(app: &AppHandle) -> Result<WebviewWindow, String> {
    app.get_webview_window("main")
        .ok_or_else(|| "main window is unavailable".to_string())
}

fn microphone_access_enabled(state: &tauri::State<'_, DesktopState>) -> Result<bool, String> {
    state
        .microphone_access_enabled
        .lock()
        .map(|enabled| *enabled)
        .map_err(|_| "microphone access state is unavailable".to_string())
}

fn set_microphone_access(app: &AppHandle, enabled: bool) -> Result<bool, String> {
    let state = app.state::<DesktopState>();
    {
        let mut current = state
            .microphone_access_enabled
            .lock()
            .map_err(|_| "microphone access state is unavailable".to_string())?;
        *current = enabled;
    }

    app.emit(
        "stem://microphone-access-changed",
        MicrophoneAccessChanged { enabled },
    )
    .ok();
    let _ = refresh_tray_menu(app);
    Ok(enabled)
}

fn handle_deep_link(app: &AppHandle, raw: &str) {
    let Some(target) = resolve_stem_deep_link(raw, DEFAULT_REMOTE_URL) else {
        return;
    };

    if let Some(window) = app.get_webview_window("main") {
        let escaped = serde_json::to_string(&target).unwrap_or_else(|_| "\"/\"".to_string());
        let _ = window.eval(&format!("window.location.replace({escaped})"));
        let _ = window.show();
        let _ = window.set_focus();
    }

    app.emit("stem://open-url", raw.to_string()).ok();
}

fn capture_argv_deep_link(args: &[String]) -> Option<String> {
    args.iter().find(|arg| arg.starts_with("stem://")).cloned()
}

#[cfg(windows)]
mod platform_activity {
    use std::mem::size_of;

    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::SystemInformation::GetTickCount64;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowThreadProcessId,
    };

    use super::DesktopActivitySnapshot;

    pub fn snapshot() -> Result<DesktopActivitySnapshot, String> {
        Ok(DesktopActivitySnapshot {
            idle_seconds: idle_seconds()?,
            active_app_category: active_process_name().and_then(|name| category_for_process(&name)),
        })
    }

    fn idle_seconds() -> Result<u64, String> {
        let mut input = LASTINPUTINFO {
            cbSize: size_of::<LASTINPUTINFO>() as u32,
            dwTime: 0,
        };

        let ok = unsafe { GetLastInputInfo(&mut input) };
        if ok == 0 {
            return Err("last input state is unavailable".to_string());
        }

        let now = unsafe { GetTickCount64() };
        Ok(now.saturating_sub(u64::from(input.dwTime)) / 1000)
    }

    fn active_process_name() -> Option<String> {
        let hwnd = unsafe { GetForegroundWindow() };
        if hwnd.is_null() {
            return None;
        }

        let mut process_id = 0u32;
        unsafe { GetWindowThreadProcessId(hwnd, &mut process_id) };
        if process_id == 0 {
            return None;
        }

        let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process_id) };
        if handle.is_null() {
            return None;
        }

        let mut buffer = vec![0u16; 32_768];
        let mut size = buffer.len() as u32;
        let ok = unsafe { QueryFullProcessImageNameW(handle, 0, buffer.as_mut_ptr(), &mut size) };
        unsafe { CloseHandle(handle) };

        if ok == 0 || size == 0 {
            return None;
        }

        let path = String::from_utf16_lossy(&buffer[..size as usize]);
        path.rsplit(|ch| ch == '\\' || ch == '/')
            .next()
            .map(|name| name.to_ascii_lowercase())
    }

    fn category_for_process(process_name: &str) -> Option<String> {
        if is_game_process(process_name) {
            return Some("game".to_string());
        }
        if is_work_process(process_name) {
            return Some("work".to_string());
        }
        None
    }

    fn is_game_process(name: &str) -> bool {
        matches!(
            name,
            "steam.exe"
                | "steamwebhelper.exe"
                | "epicgameslauncher.exe"
                | "battle.net.exe"
                | "riotclientservices.exe"
                | "leagueclientux.exe"
                | "valorant.exe"
                | "fortniteclient-win64-shipping.exe"
                | "cs2.exe"
                | "dota2.exe"
                | "gta5.exe"
                | "minecraft.exe"
                | "minecraftlauncher.exe"
                | "minecraft.windows.exe"
                | "robloxplayerbeta.exe"
                | "cyberpunk2077.exe"
        )
    }

    fn is_work_process(name: &str) -> bool {
        matches!(
            name,
            "code.exe"
                | "cursor.exe"
                | "devenv.exe"
                | "rider64.exe"
                | "idea64.exe"
                | "pycharm64.exe"
                | "webstorm64.exe"
                | "phpstorm64.exe"
                | "datagrip64.exe"
                | "studio64.exe"
                | "figma.exe"
                | "winword.exe"
                | "excel.exe"
                | "powerpnt.exe"
                | "onenote.exe"
                | "outlook.exe"
                | "teams.exe"
                | "slack.exe"
                | "zoom.exe"
                | "notion.exe"
                | "obsidian.exe"
                | "postman.exe"
                | "docker desktop.exe"
        )
    }
}

#[cfg(not(windows))]
mod platform_activity {
    use super::DesktopActivitySnapshot;

    pub fn snapshot() -> Result<DesktopActivitySnapshot, String> {
        Ok(DesktopActivitySnapshot {
            idle_seconds: 0,
            active_app_category: None,
        })
    }
}

#[cfg(windows)]
mod platform_autostart {
    use std::io;

    use tauri::AppHandle;
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    use super::AUTOSTART_REG_VALUE;

    const AUTOSTART_REG_PATH: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    const AUTOSTART_STATE_REG_PATH: &str = r"Software\STEM\Messenger\Desktop";
    const AUTOSTART_INITIALIZED_VALUE: &str = "AutostartInitialized";

    pub fn ensure_default_enabled_once(app: &AppHandle) -> Result<(), String> {
        if is_initialized()? {
            return Ok(());
        }

        if !is_enabled(app)? {
            write_enabled(app, true)?;
        }

        set_initialized()
    }

    pub fn is_enabled(app: &AppHandle) -> Result<bool, String> {
        let run_key = open_run_key(false)?;
        let value = match run_key.get_value::<String, _>(AUTOSTART_REG_VALUE) {
            Ok(value) => value,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error.to_string()),
        };

        if value.trim().is_empty() {
            return Ok(false);
        }

        let expected = autostart_command(app)?;
        Ok(value.eq_ignore_ascii_case(&expected))
    }

    pub fn set_enabled(app: &AppHandle, enabled: bool) -> Result<(), String> {
        write_enabled(app, enabled)?;
        set_initialized()
    }

    fn write_enabled(app: &AppHandle, enabled: bool) -> Result<(), String> {
        let run_key = open_run_key(true)?;
        if enabled {
            run_key
                .set_value(AUTOSTART_REG_VALUE, &autostart_command(app)?)
                .map_err(|error| error.to_string())?;
        } else {
            match run_key.delete_value(AUTOSTART_REG_VALUE) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.to_string()),
            }
        }

        Ok(())
    }

    fn open_run_key(write: bool) -> Result<RegKey, String> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        if write {
            hkcu.create_subkey(AUTOSTART_REG_PATH)
                .map(|(key, _)| key)
                .map_err(|error| error.to_string())
        } else {
            hkcu.open_subkey(AUTOSTART_REG_PATH)
                .map_err(|error| error.to_string())
        }
    }

    fn autostart_command(_app: &AppHandle) -> Result<String, String> {
        let exe = std::env::current_exe().map_err(|error| error.to_string())?;
        Ok(format!("\"{}\"", exe.display()))
    }

    fn is_initialized() -> Result<bool, String> {
        let state_key = match open_state_key(false) {
            Ok(key) => key,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error.to_string()),
        };

        match state_key.get_value::<u32, _>(AUTOSTART_INITIALIZED_VALUE) {
            Ok(value) => Ok(value != 0),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error.to_string()),
        }
    }

    fn set_initialized() -> Result<(), String> {
        open_state_key(true)
            .map_err(|error| error.to_string())?
            .set_value(AUTOSTART_INITIALIZED_VALUE, &1u32)
            .map_err(|error| error.to_string())
    }

    fn open_state_key(write: bool) -> Result<RegKey, io::Error> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        if write {
            hkcu.create_subkey(AUTOSTART_STATE_REG_PATH)
                .map(|(key, _)| key)
        } else {
            hkcu.open_subkey(AUTOSTART_STATE_REG_PATH)
        }
    }
}

#[cfg(not(windows))]
mod platform_autostart {
    use tauri::AppHandle;

    pub fn ensure_default_enabled_once(_app: &AppHandle) -> Result<(), String> {
        Ok(())
    }

    pub fn is_enabled(_app: &AppHandle) -> Result<bool, String> {
        Ok(false)
    }

    pub fn set_enabled(_app: &AppHandle, _enabled: bool) -> Result<(), String> {
        Err("Autostart is only supported on Windows".to_string())
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(DesktopState::default())
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            if let Some(raw) = capture_argv_deep_link(&argv) {
                if let Some(state) = app.try_state::<DesktopState>() {
                    if let Ok(mut pending) = state.pending_deep_link.lock() {
                        *pending = Some(raw.clone());
                    }
                }
                handle_deep_link(app, &raw);
            }
        }))
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            bootstrap,
            resolve_deep_link,
            take_pending_deep_link,
            open_external_url,
            set_unread_count,
            get_autostart_enabled,
            set_autostart_enabled,
            get_microphone_access_enabled,
            set_microphone_access_enabled,
            get_desktop_activity_snapshot,
            minimize_desktop_window,
            toggle_desktop_window_maximized,
            close_desktop_window_to_tray,
            check_desktop_shell_update,
            install_desktop_shell_update,
            show_desktop_notification,
            save_file_to_downloads_stem,
            begin_file_save_to_downloads_stemred,
            write_file_save_chunk_to_downloads_stemred,
            finish_file_save_to_downloads_stemred,
            cancel_file_save_to_downloads_stemred,
            find_file_in_downloads_stemred,
            desktop_path_exists,
            open_downloaded_file,
            pick_music_directory,
            scan_music_directory,
            read_music_file
        ])
        .on_window_event(|window, event| {
            if window.label() == "main" {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .setup(|app| {
            let app_handle = app.handle().clone();
            let _ = platform_autostart::ensure_default_enabled_once(&app_handle);

            create_main_window(app)?;
            create_tray(app)?;

            #[cfg(any(windows, target_os = "linux"))]
            {
                use tauri_plugin_deep_link::DeepLinkExt;
                app.deep_link().register_all()?;
            }

            let args: Vec<String> = std::env::args().collect();
            if let Some(raw) = capture_argv_deep_link(&args) {
                if let Some(state) = app.try_state::<DesktopState>() {
                    if let Ok(mut pending) = state.pending_deep_link.lock() {
                        *pending = Some(raw);
                    }
                }
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running STEM desktop shell");
}
