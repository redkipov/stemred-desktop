use serde::Serialize;
use tauri::WebviewWindow;

use crate::secret_auth_ticket::{SecretAuthContext, SecretAuthTicketStore};
use crate::secret_state::SecretRuntimeState;
use crate::secret_window::show_secret_window;

#[derive(Serialize)]
pub(crate) struct SecretCryptoStatus {
    protocol_version: u16,
    reason: Option<&'static str>,
    status: &'static str,
}

#[tauri::command]
pub(crate) fn secret_crypto_status(
    window: WebviewWindow,
    tickets: tauri::State<'_, SecretAuthTicketStore>,
    state: tauri::State<'_, SecretRuntimeState>,
    ticket: String,
    account_id: String,
    crypto_public_key: String,
) -> Result<SecretCryptoStatus, String> {
    if window.label() != "secret" {
        return Err("secret_chat_native_only".to_string());
    }
    let context = SecretAuthContext::new(account_id, crypto_public_key)?;
    if !tickets.consume(&ticket, &context)? {
        return Err("crypto_device_required".to_string());
    }

    show_secret_window(&window)?;
    let snapshot = state.snapshot();
    Ok(SecretCryptoStatus {
        protocol_version: snapshot.protocol_version,
        reason: snapshot.reason,
        status: snapshot.status,
    })
}
