# Contributing

Contributions are welcome for the open-source desktop shell.

## Requirements

- Node.js 24.x
- npm
- Rust stable toolchain
- Windows 11 for Windows installer builds

## Local Check

```powershell
npm install
npm run check
```

## Pull Requests

- Keep changes scoped to the desktop shell.
- Do not commit generated folders: `node_modules`, `dist`, `release`, `src-tauri/target`.
- Do not commit signing keys, certificates with private material, `.pfx`, `.p12`, `.key`, tokens or logs.
- Describe user-visible behavior changes.

Every signing request for official releases must be reviewed by a maintainer listed in `CODE_SIGNING_POLICY.md`.
