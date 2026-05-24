# Code Signing Policy

Free code signing for official open-source releases is intended to be provided through SignPath.io and the SignPath Foundation after project approval.

## Project

- Project name: StemRed Desktop
- Repository: https://github.com/redkipov/stemred-desktop
- License: MIT
- Privacy policy: `PRIVACY.md` and https://chat-stem.ru/privacy

## Scope

Official signatures apply only to release artifacts built from this repository's source code.

The signed artifact is the Windows desktop shell installer produced by:

```powershell
npm install
npm run check
npm run build
```

Unsigned third-party binaries are not intentionally bundled except components downloaded by the standard Tauri/WebView2 installer flow.

## Roles

- Committers and reviewers: repository maintainers with write access.
- Release approvers: repository maintainers with owner permissions.

## Release Rules

- Releases must be built from tagged commits.
- Each release needs manual approval before signing.
- Signing keys must remain outside the repository.
- Release artifacts must have consistent product name and product version metadata.
- The project must not sign binaries that were not built from this repository.
- Security or privacy-sensitive changes require review before release.

## User Data and System Changes

The installer must provide a standard uninstall path.

The desktop shell may create shortcuts, register the `stem://` deep link scheme, enable tray behavior and optionally register launch-on-startup behavior through user-visible installer options.

The application must not modify unrelated system settings.
