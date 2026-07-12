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
  muted: boolean;
  pinned: boolean;
  playing: boolean;
  positionSec: number;
  sourceLabel: string;
  title: string;
  trackKey: string | null;
  volume: number;
};

type DesktopMusicPlayerCommand =
  | { command: 'playPause' }
  | { command: 'previous' }
  | { command: 'next' }
  | { command: 'favorite' }
  | { command: 'closeMini' }
  | { command: 'toggleMute' }
  | { command: 'setVolume'; volume: number }
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
  muted: false,
  pinned: false,
  playing: false,
  positionSec: 0,
  sourceLabel: 'Избранное',
  title: 'StemRed Music',
  trackKey: null,
  volume: 1,
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
const muteButton = document.querySelector<HTMLButtonElement>('#mini-mute')!;
const volumeEl = document.querySelector<HTMLInputElement>('#mini-volume')!;

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
    muted: Boolean(state.muted),
    pinned: Boolean(state.pinned),
    playing: Boolean(state.playing),
    positionSec: finiteSeconds(state.positionSec),
    sourceLabel: String(state.sourceLabel || 'StemRed Music'),
    title: String(state.title || 'StemRed Music'),
    trackKey: state.trackKey ? String(state.trackKey) : null,
    volume: finiteUnit(state.volume, 1),
  };
}

function finiteUnit(value: unknown, fallback: number): number {
  const number = Number(value);
  return Number.isFinite(number) ? Math.max(0, Math.min(1, number)) : fallback;
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
  const silent = playerState.muted || playerState.volume <= 0.001;
  muteButton.textContent = silent ? '🔇' : '🔊';
  muteButton.setAttribute('aria-label', silent ? 'Включить звук' : 'Отключить звук');
  muteButton.setAttribute('aria-pressed', String(playerState.muted));
  volumeEl.value = String(Math.round(playerState.volume * 100));
  volumeEl.setAttribute('aria-valuenow', volumeEl.value);
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

function seekTo(positionSec: number) {
  if (!(playerState.durationSec > 0)) return;
  void sendCommand({
    command: 'seek',
    positionSec: Math.max(0, Math.min(playerState.durationSec, positionSec)),
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
muteButton.addEventListener('click', () => void sendCommand({ command: 'toggleMute' }));
volumeEl.addEventListener('input', () => {
  void sendCommand({
    command: 'setVolume',
    volume: finiteUnit(Number(volumeEl.value) / 100, playerState.volume),
  });
});

progressEl.addEventListener('pointerdown', (event) => {
  progressEl.setPointerCapture?.(event.pointerId);
  seekFromClientX(event.clientX);
});
progressEl.addEventListener('pointermove', (event) => {
  if (event.buttons !== 1) return;
  seekFromClientX(event.clientX);
});
progressEl.addEventListener('keydown', (event) => {
  const nextPosition =
    event.key === 'ArrowLeft' || event.key === 'ArrowDown'
      ? playerState.positionSec - 5
      : event.key === 'ArrowRight' || event.key === 'ArrowUp'
        ? playerState.positionSec + 5
        : event.key === 'Home'
          ? 0
          : event.key === 'End'
            ? playerState.durationSec
            : null;
  if (nextPosition === null) return;
  event.preventDefault();
  seekTo(nextPosition);
});

installDragRegions();
render();

void invoke<DesktopMusicPlayerState>('get_desktop_music_player_state')
  .then(applyState)
  .catch(() => undefined);

void listen<DesktopMusicPlayerState>('stem://music-player-state-changed', (event) => {
  applyState(event.payload);
});
