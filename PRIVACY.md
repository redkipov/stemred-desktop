# Privacy Policy

STEM Messenger Desktop is a native shell for the STEM web messenger at `https://chat-stem.ru/`.

The desktop shell itself does not operate a separate user database and does not sell personal data. It loads the STEM web client, stores normal WebView session data on the user's device, and communicates with `chat-stem.ru` for messenger features.

The application may request operating system permissions for notifications, microphone access, geolocation and network access when the corresponding messenger feature is used.

The desktop shell contacts these endpoints:

- `https://chat-stem.ru/api/client/config` to read runtime configuration and minimum shell version.
- `https://chat-stem.ru/` and related HTTPS/WSS endpoints to run the messenger.
- `https://chat-stem.ru/api/desktop/update/...` to check for native shell updates.

Account data, messages, files, calls and support requests are processed by the STEM service according to the public privacy policy:

- https://chat-stem.ru/privacy

Users can uninstall the desktop shell with the standard Windows uninstall flow.
