use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use crate::secret_auth_ticket::{
    SecretAuthContext, SecretAuthTicketPayload, SecretAuthTicketStore,
};

pub(crate) fn create_secret_window(app: &mut tauri::App) -> tauri::Result<WebviewWindow> {
    WebviewWindowBuilder::new(app, "secret", WebviewUrl::App("secret.html".into()))
        .title("StemRed — секретный чат")
        .inner_size(960.0, 700.0)
        .min_inner_size(480.0, 560.0)
        .visible(false)
        .disable_drag_drop_handler()
        .build()
}

#[allow(dead_code)]
pub(crate) fn begin_secret_auth(
    app: &AppHandle,
    tickets: &SecretAuthTicketStore,
    context: SecretAuthContext,
) -> Result<(), String> {
    let ticket = tickets.issue(context.clone())?;
    let window = app
        .get_webview_window("secret")
        .ok_or_else(|| "secret_window_unavailable".to_string())?;
    window
        .emit(
            "stem://secret-auth-ticket",
            SecretAuthTicketPayload::new(context, ticket),
        )
        .map_err(|error| error.to_string())
}

pub(crate) fn show_secret_window(window: &WebviewWindow) -> Result<(), String> {
    window.show().map_err(|error| error.to_string())?;
    window.set_focus().map_err(|error| error.to_string())
}
