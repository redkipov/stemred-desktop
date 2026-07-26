# StemRed Desktop

Open-source Tauri v2 desktop shell for StemRed. Windows 10-11 is the primary target; the same shell path can be reused for macOS later.

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

## Update compatibility

- The audited direct-upgrade floor is `0.1.8`, the oldest public source release. Every public `0.1.x` shell uses the same HTTPS updater endpoint, Tauri 2 updater plugin and pinned minisign public key.
- `0.1.35` is a universal stable target: the update feed has no minimum-current-version restriction, so `0.1.8–0.1.34` can update directly without intermediate installers.
- Shells `0.1.8–0.1.11` expose the updater plugin but their legacy install command returns `false`. The hosted UI treats that as a compatibility signal and completes the signed update through the plugin.
- Builds before `0.1.8` are supported on a best-effort basis when they use the same endpoint, public key and updater plugin. Otherwise install the latest immutable GitHub Release manually.
- A user-initiated retry may bypass the one-minute background check guard. Focus and scheduler checks remain rate-limited.

The updater never falls back to an unsigned arbitrary URL. Direct Windows releases may lack Authenticode while the Tauri signature, SHA256 and versioned GitHub asset remain mandatory.

## Windows Secret Beta 0.1.37

The current private Windows Preview adds DPAPI-protected persistent device identity, MLS create/invite/accept/send/decrypt, durable offline outbox and restart recovery, safety-number verification, explicit identity-change confirmation, and plaintext cleanup on lock, sleep, and logout. Transparency commitments use a private Tessera log with pinned origin and three Preview witness keys plus RFC 6962 proof verification.

This is not a public stable release. The installer is distributed only through cohort-private storage for the two-account/two-device beta and is never advertised by the ordinary updater or attached to a public GitHub Release. Windows Authenticode is currently absent; the Tauri updater signature and SHA-256 release manifest remain mandatory. VBS/TPM, independent witness operators, production attestation, and general rollout are a later milestone.

Source can be built from the main monorepo with `npm run release:secret-beta`; all Rust/Tauri build outputs must stay under `F:\STEM-build` on the Windows release machine. The final two-client vertical acceptance test remains required before beta activation.

## Windows build

```powershell
npm install
npm run check
npm run build
```

`npm run check` runs TypeScript and Vite validation. `npm run build` creates the Windows installer through Tauri.

For a production desktop update, bump the patch version during the build:

```powershell
npm run release:win
```

After that, commit the version files and update `deploy\.env.release` in the main repository so `DESKTOP_RECOMMENDED_SHELL_VERSION`, `DESKTOP_UPDATE_WINDOWS_X86_64_VERSION`, URL and signature all point to the same new version. Deploy refuses mismatched metadata.

### Release profiles

The public `ordinary` profile is the current unsigned Windows release. It is compiled without the `secret-crypto` feature, keeps the managed messenger behavior unchanged and is the only profile that may be published through the ordinary updater:

```powershell
npm run release:win
```

The private `secret-beta` command uses the already aligned desktop version, enables `secret-crypto` and permits the Preview milestone to run without Authenticode. Rust/Tauri sources are mirrored to `F:\STEM-build\source-current\stem-messenger`, Cargo artifacts go to `F:\STEM-build\cargo-target`, and Tauri therefore generates `src-tauri\gen\schemas` outside `Documents`. Configure `STEM_SECRET_WINDOWS_RELEASE_ID` and the four transparency pins in the main repository's `deploy\.env.release`; the build script loads them from that single release source and rejects a version or environment mismatch:

```powershell
npm run release:secret-beta
```

The four transparency keys are compiled into the beta runtime; missing, duplicate or mismatched pins stop the build/preflight. Authenticode remains deferred, while the Tauri updater signature is still mandatory. Output is isolated under `release\secret-beta\windows\x64\<version>` and must only be distributed to the two-account Preview allowlist; it never replaces the ordinary `release\Setup STEM.exe` artifact. The full order is documented in `docs\secret-windows-deploy-readiness-plan.md`.

Generated folders and release artifacts are not part of the source release:

- `node_modules/`
- `dist/`
- `release/`
- `src-tauri/target/`

## Code signing

Public Windows installers should be signed with a certificate that chains to a public trusted CA, Microsoft Artifact Signing, or an approved SignPath release policy. Until that is configured, GitHub release installers are built unsigned.

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

Official open-source release signing is planned through SignPath Foundation after project approval. See `CODE_SIGNING_POLICY.md`.

Microsoft Store build uses a separate offline installer configuration and requires public code signing:

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
