# STEM Messenger Desktop

Open-source Tauri v2 desktop shell for STEM Messenger. Windows 10-11 is the primary target; the same shell path can be reused for macOS later.

License: MIT. See `LICENSE`.

Project policies:

- Privacy: `PRIVACY.md`
- Security reports: `SECURITY.md`
- Contributions: `CONTRIBUTING.md`
- Code signing: `CODE_SIGNING_POLICY.md`

## Runtime model

- The app starts on a local fallback/bootstrap page.
- The bootstrap command fetches `https://chat-stem.ru/api/client/config`.
- If the current shell version satisfies `min_shell_version`, the WebView navigates to `https://chat-stem.ru/`.
- If `/api/client/config` is not deployed yet but `https://chat-stem.ru/` is reachable, the shell still opens the default domain.
- Normal web changes ship through the server deploy. Native updates are only for wrapper changes.

## Windows build

```powershell
npm install
npm run check
npm run build
```

`npm run check` runs TypeScript and Vite validation. `npm run build` creates the Windows installer through Tauri.

Generated folders and release artifacts are not part of the source release:

- `node_modules/`
- `dist/`
- `release/`
- `src-tauri/target/`

## Code signing

Public Windows installers must be signed with a certificate that chains to a public trusted CA or Microsoft Artifact Signing. The local self-signed certificate is only for development; it does not build SmartScreen reputation for public users.

Recommended low-cost public signing path when the publisher is eligible for Microsoft Artifact Signing:

```powershell
$env:STEM_CODESIGN_ARTIFACT_DLIB = "C:\path\to\Azure.CodeSigning.Dlib.dll"
$env:STEM_CODESIGN_ARTIFACT_METADATA = "C:\path\to\metadata.json"
npm run build
Get-AuthenticodeSignature ".\release\Setup STEM.exe" | Format-List Status,SignerCertificate,TimeStamperCertificate
```

Traditional OV/PFX signing is also supported:

```powershell
$env:STEM_CODESIGN_PFX = "C:\path\to\codesign.pfx"
$env:STEM_CODESIGN_PFX_PASSWORD = "..."
npm run build
```

Local test signing is explicit:

```powershell
$env:STEM_ALLOW_LOCAL_CODESIGN = "1"
npm run build
```

Official open-source release signing is planned through SignPath Foundation after project approval. See `CODE_SIGNING_POLICY.md`.

Microsoft Store build uses a separate offline installer configuration and refuses local self-signed certificates:

```powershell
npm run build:store
```

The Store artifact is published locally as:

```text
release\microsoft-store\windows\x64\<version>\Setup-STEM-<version>-x64.exe
```

The Windows build script signs updater artifacts when the private updater key is available in one of these places:

```powershell
$env:TAURI_SIGNING_PRIVATE_KEY_PATH
$env:TAURI_SIGNING_PRIVATE_KEY
$env:USERPROFILE\.ssh\stem-tauri-updater.key
```

The private updater key must stay outside the repository. Tauri publishes native updates through the backend `/api/desktop/update/:target/:arch/:current_version` feed.

## Privacy summary

The desktop shell does not run a separate messaging backend. It loads `https://chat-stem.ru/`, checks runtime configuration, handles native shell features and delegates account data, messages, media and calls to the STEM service. See `PRIVACY.md` and https://chat-stem.ru/privacy.

## Deep links

Supported desktop routes:

- `stem://messages`
- `stem://chat/{user_id}`
- `stem://room/{room_id}`

Windows deep links are registered by the installed application and routed through the single-instance plugin.
