import './mini-player.css';

import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';

type DesktopMusicPlayerState = {
  artist: string;
  buffering: boolean;
  canNext: boolean;
  canPlay: boolean;
  canPrevious: boolean;
  durationSec: number;
  failed: boolean;
  favorite: boolean;
  pinned: boolean;
  playing: boolean;
  positionSec: number;
  sourceLabel: string;
  title: string;
  trackKey: string | null;
};

type DesktopMusicPlayerCommand =
  | { command: 'playPause' }
  | { command: 'previous' }
  | { command: 'next' }
  | { command: 'favorite' }
  | { command: 'closeMini' }
  | { command: 'seek'; positionSec: number };

const DEFAULT_STATE: DesktopMusicPlayerState = {
  artist: '',
  buffering: false,
  canNext: false,
  canPlay: false,
  canPrevious: false,
  durationSec: 0,
  failed: false,
  favorite: false,
  pinned: false,
  playing: false,
  positionSec: 0,
  sourceLabel: 'Избранное',
  title: 'StemRed Music',
  trackKey: null,
};

const root = document.querySelector<HTMLElement>('.mini-player')!;
const titleEl = document.querySelector<HTMLElement>('#mini-title')!;
const sourceEl = document.querySelector<HTMLElement>('#mini-source')!;
const positionEl = document.querySelector<HTMLElement>('#mini-position')!;
const durationEl = document.querySelector<HTMLElement>('#mini-duration')!;
const progressEl = document.querySelector<HTMLElement>('#mini-progress')!;
const progressFillEl = document.querySelector<HTMLElement>('#mini-progress-fill')!;
const closeButton = document.querySelector<HTMLButtonElement>('#mini-close')!;
const favoriteButton = document.querySelector<HTMLButtonElement>('#mini-favorite')!;
const previousButton = document.querySelector<HTMLButtonElement>('#mini-prev')!;
const playButton = document.querySelector<HTMLButtonElement>('#mini-play')!;
const nextButton = document.querySelector<HTMLButtonElement>('#mini-next')!;

let playerState = DEFAULT_STATE;

function normalizeState(value: unknown): DesktopMusicPlayerState {
  if (!value || typeof value !== 'object') return DEFAULT_STATE;
  const state = value as Partial<DesktopMusicPlayerState>;
  return {
    artist: String(state.artist || ''),
    buffering: Boolean(state.buffering),
    canNext: Boolean(state.canNext),
    canPlay: Boolean(state.canPlay),
    canPrevious: Boolean(state.canPrevious),
    durationSec: finiteSeconds(state.durationSec),
    failed: Boolean(state.failed),
    favorite: Boolean(state.favorite),
    pinned: Boolean(state.pinned),
    playing: Boolean(state.playing),
    positionSec: finiteSeconds(state.positionSec),
    sourceLabel: String(state.sourceLabel || 'StemRed Music'),
    title: String(state.title || 'StemRed Music'),
    trackKey: state.trackKey ? String(state.trackKey) : null,
  };
}

function finiteSeconds(value: unknown): number {
  const seconds = Number(value);
  return Number.isFinite(seconds) ? Math.max(0, seconds) : 0;
}

function formatTime(seconds: number): string {
  const total = Math.max(0, Math.floor(seconds || 0));
  const minutes = Math.floor(total / 60);
  const rest = String(total % 60).padStart(2, '0');
  return `${minutes}:${rest}`;
}

function render() {
  const duration = playerState.durationSec;
  const position = Math.min(duration || playerState.positionSec, playerState.positionSec);
  const progress = duration > 0 ? Math.max(0, Math.min(1, position / duration)) : 0;

  root.classList.toggle('is-playing', playerState.playing);
  titleEl.textContent = playerState.title || 'StemRed Music';
  sourceEl.textContent = playerState.artist || playerState.sourceLabel || 'StemRed Music';
  positionEl.textContent = formatTime(position);
  durationEl.textContent = formatTime(duration);
  progressFillEl.style.width = `${progress * 100}%`;
  progressEl.setAttribute('aria-valuemax', String(Math.max(0, Math.floor(duration))));
  progressEl.setAttribute('aria-valuenow', String(Math.max(0, Math.floor(position))));

  favoriteButton.disabled = !playerState.trackKey;
  favoriteButton.classList.toggle('is-active', playerState.favorite);
  favoriteButton.textContent = playerState.favorite ? '★' : '＋';
  favoriteButton.setAttribute(
    'aria-label',
    playerState.favorite ? 'Убрать из избранного' : 'Добавить в избранное',
  );
  previousButton.disabled = !playerState.canPrevious;
  nextButton.disabled = !playerState.canNext;
  playButton.disabled = !playerState.canPlay && !playerState.trackKey;
  playButton.textContent = playerState.buffering ? '…' : playerState.playing ? 'Ⅱ' : '▶';
  playButton.setAttribute('aria-label', playerState.playing ? 'Пауза' : 'Проиграть');
}

function applyState(value: unknown) {
  playerState = normalizeState(value);
  render();
}

async function sendCommand(command: DesktopMusicPlayerCommand) {
  try {
    await invoke('desktop_music_player_command', { command });
  } catch {
    // Старая оболочка или закрывающееся окно: действие просто игнорируется.
  }
}

function seekFromClientX(clientX: number) {
  if (!(playerState.durationSec > 0)) return;
  const rect = progressEl.getBoundingClientRect();
  const x = Math.max(0, Math.min(rect.width, clientX - rect.left));
  void sendCommand({
    command: 'seek',
    positionSec: (x / Math.max(1, rect.width)) * playerState.durationSec,
  });
}

function installDragRegions() {
  document.querySelectorAll<HTMLElement>('[data-drag-region]').forEach((element) => {
    element.addEventListener('pointerdown', (event) => {
      const target = event.target;
      if (target instanceof HTMLElement && target.closest('button')) return;
      void getCurrentWindow().startDragging();
    });
  });
}

closeButton.addEventListener('click', () => void sendCommand({ command: 'closeMini' }));
favoriteButton.addEventListener('click', () => void sendCommand({ command: 'favorite' }));
previousButton.addEventListener('click', () => void sendCommand({ command: 'previous' }));
playButton.addEventListener('click', () => void sendCommand({ command: 'playPause' }));
nextButton.addEventListener('click', () => void sendCommand({ command: 'next' }));

progressEl.addEventListener('pointerdown', (event) => {
  progressEl.setPointerCapture?.(event.pointerId);
  seekFromClientX(event.clientX);
});
progressEl.addEventListener('pointermove', (event) => {
  if (event.buttons !== 1) return;
  seekFromClientX(event.clientX);
});

installDragRegions();
render();

void invoke<DesktopMusicPlayerState>('get_desktop_music_player_state')
  .then(applyState)
  .catch(() => undefined);

void listen<DesktopMusicPlayerState>('stem://music-player-state-changed', (event) => {
  applyState(event.payload);
});
