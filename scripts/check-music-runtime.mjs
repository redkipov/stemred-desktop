import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const root = resolve(import.meta.dirname, '..');
const files = {
  cargo: readFileSync(resolve(root, 'src-tauri', 'Cargo.toml'), 'utf8'),
  aclSchema: readFileSync(
    resolve(root, 'src-tauri', 'gen', 'schemas', 'acl-manifests.json'),
    'utf8',
  ),
  desktopSchema: readFileSync(
    resolve(root, 'src-tauri', 'gen', 'schemas', 'desktop-schema.json'),
    'utf8',
  ),
  lib: readFileSync(resolve(root, 'src-tauri', 'src', 'lib.rs'), 'utf8'),
  miniPlayer: readFileSync(resolve(root, 'src', 'mini-player.ts'), 'utf8'),
  miniPlayerHtml: readFileSync(resolve(root, 'mini-player.html'), 'utf8'),
  permissions: readFileSync(
    resolve(root, 'src-tauri', 'permissions', 'stem-desktop-commands.toml'),
    'utf8',
  ),
};

const requiredCommands = [
  'get_desktop_music_runtime_capabilities',
  'pick_music_directory',
  'scan_music_directory',
  'cancel_music_directory_scan',
  'read_music_artwork',
  'open_music_range_source',
  'close_music_range_source',
  'revoke_music_directory',
  'read_music_file',
];

const requiredFileTransferCommands = [
  'begin_file_save_to_downloads_stemred',
  'write_file_save_chunk_to_downloads_stemred',
  'finish_file_save_to_downloads_stemred',
  'cancel_file_save_to_downloads_stemred',
  'find_file_in_downloads_stemred',
  'desktop_path_exists',
];

const checks = [
  ['Cargo.toml', files.cargo, 'tauri-plugin-dialog'],
  ['Cargo.toml', files.cargo, 'lofty = "0.24"'],
  ['lib.rs', files.lib, 'use tauri_plugin_dialog::DialogExt;'],
  ['lib.rs', files.lib, 'tauri_plugin_dialog::init()'],
  ['lib.rs', files.lib, 'DESKTOP_MUSIC_FOLDERS_FILE'],
  ['lib.rs', files.lib, 'DesktopMusicDirectory'],
  ['lib.rs', files.lib, 'music_range_source_v1: true'],
  ['lib.rs', files.lib, 'register_asynchronous_uri_scheme_protocol'],
  ['lib.rs', files.lib, '.use_https_scheme(true)'],
  ['lib.rs', files.lib, 'DESKTOP_MUSIC_RANGE_MAX_BYTES'],
  ['lib.rs', files.lib, 'DESKTOP_MUSIC_LEGACY_MAX_BYTES'],
  ['lib.rs', files.lib, 'desktop_shell_update_required'],
  ['lib.rs', files.lib, 'music_metadata_v2: true'],
  ['lib.rs', files.lib, 'music_artwork_v1: true'],
  ['lib.rs', files.lib, 'cancellable_music_scan_v1: true'],
  ['lib.rs', files.lib, 'read_cover_art(false)'],
  ['lib.rs', files.lib, 'stem://music-scan-progress'],
  ['lib.rs', files.lib, 'muted: bool'],
  ['lib.rs', files.lib, 'volume: f64'],
  ['mini player', files.miniPlayer, "command: 'toggleMute'"],
  ['mini player', files.miniPlayer, "command: 'setVolume'"],
  ['mini player HTML', files.miniPlayerHtml, 'id="mini-mute"'],
  ['mini player HTML', files.miniPlayerHtml, 'id="mini-volume"'],
  ['ACL schema', files.aclSchema, '"dialog"'],
  ['desktop schema', files.desktopSchema, 'dialog:default'],
  ...requiredCommands.map((command) => ['lib.rs', files.lib, `fn ${command}`]),
  ...requiredCommands.map((command) => ['invoke handler', files.lib, command]),
  ...requiredCommands.map((command) => ['permissions', files.permissions, command]),
  ...requiredCommands.map((command) => ['ACL schema', files.aclSchema, command]),
  ...requiredFileTransferCommands.map((command) => [
    'lib.rs',
    files.lib,
    `fn ${command}`,
  ]),
  ...requiredFileTransferCommands.map((command) => [
    'invoke handler',
    files.lib,
    command,
  ]),
  ...requiredFileTransferCommands.map((command) => [
    'permissions',
    files.permissions,
    command,
  ]),
];

const missing = checks.filter(([, content, token]) => !content.includes(token));

if (missing.length > 0) {
  for (const [label, , token] of missing) {
    console.error(`Missing music runtime token in ${label}: ${token}`);
  }
  process.exit(1);
}

console.log('Desktop runtime parity check passed');
