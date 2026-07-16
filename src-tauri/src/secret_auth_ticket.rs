use std::sync::Mutex;
use std::time::{Duration, Instant};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use serde::Serialize;
use uuid::Uuid;

const TICKET_TTL: Duration = Duration::from_secs(60);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SecretAuthContext {
    pub(crate) account_id: String,
    pub(crate) crypto_public_key: String,
}

impl SecretAuthContext {
    pub(crate) fn new(account_id: String, crypto_public_key: String) -> Result<Self, String> {
        let account_id = Uuid::parse_str(account_id.trim())
            .map_err(|_| "crypto_device_required")?
            .to_string();
        let public_key = URL_SAFE_NO_PAD
            .decode(crypto_public_key.as_bytes())
            .map_err(|_| "crypto_device_required")?;
        if public_key.len() != 32 || URL_SAFE_NO_PAD.encode(&public_key) != crypto_public_key {
            return Err("crypto_device_required".to_string());
        }

        Ok(Self {
            account_id,
            crypto_public_key,
        })
    }
}

#[derive(Clone, Serialize)]
pub(crate) struct SecretAuthTicketPayload {
    account_id: String,
    crypto_public_key: String,
    ticket: String,
}

impl SecretAuthTicketPayload {
    pub(crate) fn new(context: SecretAuthContext, ticket: String) -> Self {
        Self {
            account_id: context.account_id,
            crypto_public_key: context.crypto_public_key,
            ticket,
        }
    }
}

struct SecretAuthTicket {
    context: SecretAuthContext,
    expires_at: Instant,
    value: String,
}

#[derive(Default)]
pub(crate) struct SecretAuthTicketStore {
    active: Mutex<Option<SecretAuthTicket>>,
}

impl SecretAuthTicketStore {
    pub(crate) fn issue(&self, context: SecretAuthContext) -> Result<String, String> {
        let value = Uuid::new_v4().to_string();
        let ticket = SecretAuthTicket {
            context,
            expires_at: Instant::now() + TICKET_TTL,
            value: value.clone(),
        };
        *self
            .active
            .lock()
            .map_err(|_| "secret_ticket_lock_failed")? = Some(ticket);
        Ok(value)
    }

    pub(crate) fn consume(
        &self,
        value: &str,
        expected: &SecretAuthContext,
    ) -> Result<bool, String> {
        let ticket = self
            .active
            .lock()
            .map_err(|_| "secret_ticket_lock_failed")?
            .take();
        let Some(ticket) = ticket else {
            return Ok(false);
        };
        Ok(ticket.expires_at >= Instant::now()
            && constant_time_eq(ticket.value.as_bytes(), value.as_bytes())
            && ticket.context == *expected)
    }
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0_u8, |diff, (a, b)| diff | (a ^ b))
        == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context(account_id: &str, key_byte: u8) -> SecretAuthContext {
        SecretAuthContext::new(
            account_id.to_string(),
            URL_SAFE_NO_PAD.encode([key_byte; 32]),
        )
        .unwrap()
    }

    #[test]
    fn ticket_is_single_use_and_bound_to_crypto_identity() {
        let store = SecretAuthTicketStore::default();
        let expected = context("00000000-0000-4000-8000-000000000001", 7);
        let value = store.issue(expected.clone()).unwrap();
        assert!(!store
            .consume(&value, &context("00000000-0000-4000-8000-000000000002", 7))
            .unwrap());
        assert!(!store.consume(&value, &expected).unwrap());

        let value = store.issue(expected.clone()).unwrap();
        assert!(!store
            .consume(&value, &context("00000000-0000-4000-8000-000000000001", 8))
            .unwrap());

        let value = store.issue(expected.clone()).unwrap();
        assert!(store.consume(&value, &expected).unwrap());
        assert!(!store.consume(&value, &expected).unwrap());
    }

    #[test]
    fn rejects_non_canonical_crypto_identity() {
        assert!(SecretAuthContext::new("not-a-uuid".to_string(), "a".repeat(43),).is_err());
    }
}
