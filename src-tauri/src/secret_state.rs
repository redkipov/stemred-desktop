pub(crate) struct SecretRuntimeSnapshot {
    pub(crate) protocol_version: u16,
    pub(crate) reason: Option<&'static str>,
    pub(crate) status: &'static str,
}

#[derive(Default)]
pub(crate) struct SecretRuntimeState;

impl SecretRuntimeState {
    #[cfg(feature = "secret-crypto")]
    pub(crate) fn snapshot(&self) -> SecretRuntimeSnapshot {
        let readiness = stem_crypto_core::ffi::crypto_core_readiness();
        let ready = readiness.protocol_version == 2
            && readiness.native_initialized
            && readiness.recovery_primitives_compiled
            && readiness.sqlcipher_compiled
            && readiness.sqlcipher_runtime_verified
            && readiness.mls_application_messages_available
            && readiness.transparency_verifier_available
            && readiness.secret_chats_ready
            && readiness.blocking_reasons.is_empty();
        SecretRuntimeSnapshot {
            protocol_version: readiness.protocol_version,
            reason: (!ready).then_some("protocol_version_unsupported"),
            status: if ready { "ready" } else { "unavailable" },
        }
    }

    #[cfg(not(feature = "secret-crypto"))]
    pub(crate) fn snapshot(&self) -> SecretRuntimeSnapshot {
        SecretRuntimeSnapshot {
            protocol_version: 2,
            reason: Some("protocol_version_unsupported"),
            status: "unavailable",
        }
    }
}
