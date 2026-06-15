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
  permissions: readFileSync(
    resolve(root, 'src-tauri', 'permissions', 'stem-desktop-commands.toml'),
    'utf8',
  ),
};

const requiredCommands = [
  'pick_music_directory',
  'scan_music_directory',
  'read_music_file',
];

const checks = [
  ['Cargo.toml', files.cargo, 'tauri-plugin-dialog'],
  ['lib.rs', files.lib, 'use tauri_plugin_dialog::DialogExt;'],
  ['lib.rs', files.lib, 'tauri_plugin_dialog::init()'],
  ['lib.rs', files.lib, 'DESKTOP_MUSIC_FOLDERS_FILE'],
  ['lib.rs', files.lib, 'DesktopMusicDirectory'],
  ['ACL schema', files.aclSchema, '"dialog"'],
  ['desktop schema', files.desktopSchema, 'dialog:default'],
  ...requiredCommands.map((command) => ['lib.rs', files.lib, `fn ${command}`]),
  ...requiredCommands.map((command) => ['invoke handler', files.lib, command]),
  ...requiredCommands.map((command) => ['permissions', files.permissions, command]),
  ...requiredCommands.map((command) => ['ACL schema', files.aclSchema, command]),
];

const missing = checks.filter(([, content, token]) => !content.includes(token));

if (missing.length > 0) {
  for (const [label, , token] of missing) {
    console.error(`Missing music runtime token in ${label}: ${token}`);
  }
  process.exit(1);
}

console.log('Music runtime parity check passed');
