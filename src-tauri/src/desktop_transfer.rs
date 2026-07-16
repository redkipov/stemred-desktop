use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const DESKTOP_TRANSFER_MAX_ACTIVE: usize = 8;
pub const DESKTOP_TRANSFER_MAX_CHUNK_BYTES: usize = 1024 * 1024;
pub const DESKTOP_TRANSFER_LEGACY_MAX_BYTES: usize = 16 * 1024 * 1024;
pub const DESKTOP_TRANSFER_MAX_PENDING_BYTES: u64 = 4 * 1024 * 1024 * 1024;
pub const DESKTOP_TRANSFER_IDLE_TIMEOUT: Duration = Duration::from_secs(5 * 60);
pub const DESKTOP_TRANSFER_CAPACITY_EXCEEDED: &str = "desktop_transfer_capacity_exceeded";
pub const DESKTOP_TRANSFER_PAYLOAD_TOO_LARGE: &str = "desktop_transfer_payload_too_large";
const DESKTOP_TRANSFER_STATE_UNAVAILABLE: &str = "desktop_transfer_state_unavailable";

#[derive(Debug, Default)]
pub struct DesktopTransferBudget {
    state: Mutex<DesktopTransferBudgetState>,
}

#[derive(Debug, Default)]
struct DesktopTransferBudgetState {
    active: usize,
    pending_bytes: u64,
}

impl DesktopTransferBudget {
    pub fn acquire(self: &Arc<Self>) -> Result<DesktopTransferPermit, &'static str> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| DESKTOP_TRANSFER_STATE_UNAVAILABLE)?;
        if state.active >= DESKTOP_TRANSFER_MAX_ACTIVE {
            return Err(DESKTOP_TRANSFER_CAPACITY_EXCEEDED);
        }
        state.active += 1;

        Ok(DesktopTransferPermit {
            budget: Arc::clone(self),
            reserved_bytes: 0,
        })
    }
}

#[derive(Debug)]
pub struct DesktopTransferPermit {
    budget: Arc<DesktopTransferBudget>,
    reserved_bytes: u64,
}

impl DesktopTransferPermit {
    pub fn reserve_bytes(&mut self, bytes: u64) -> Result<(), &'static str> {
        let next_reserved = self
            .reserved_bytes
            .checked_add(bytes)
            .ok_or(DESKTOP_TRANSFER_PAYLOAD_TOO_LARGE)?;
        let mut state = self
            .budget
            .state
            .lock()
            .map_err(|_| DESKTOP_TRANSFER_STATE_UNAVAILABLE)?;
        let next_pending = state
            .pending_bytes
            .checked_add(bytes)
            .ok_or(DESKTOP_TRANSFER_PAYLOAD_TOO_LARGE)?;

        if next_reserved > DESKTOP_TRANSFER_MAX_PENDING_BYTES
            || next_pending > DESKTOP_TRANSFER_MAX_PENDING_BYTES
        {
            return Err(DESKTOP_TRANSFER_PAYLOAD_TOO_LARGE);
        }

        self.reserved_bytes = next_reserved;
        state.pending_bytes = next_pending;
        Ok(())
    }

    pub fn reserved_bytes(&self) -> u64 {
        self.reserved_bytes
    }
}

impl Drop for DesktopTransferPermit {
    fn drop(&mut self) {
        if let Ok(mut state) = self.budget.state.lock() {
            state.active = state.active.saturating_sub(1);
            state.pending_bytes = state.pending_bytes.saturating_sub(self.reserved_bytes);
        }
    }
}

#[derive(Debug)]
pub struct DesktopPartialFile {
    path: PathBuf,
    preserve: bool,
}

impl DesktopPartialFile {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            preserve: false,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn preserve(&mut self) {
        self.preserve = true;
    }
}

impl Drop for DesktopPartialFile {
    fn drop(&mut self) {
        if !self.preserve {
            let _ = fs::remove_file(&self.path);
        }
    }
}

pub fn ensure_base64_input_size(
    value: &str,
    max_decoded_bytes: usize,
) -> Result<&str, &'static str> {
    let value = value.trim();
    let max_encoded_bytes = max_decoded_bytes.div_ceil(3).saturating_mul(4);
    if value.len() > max_encoded_bytes {
        return Err(DESKTOP_TRANSFER_PAYLOAD_TOO_LARGE);
    }
    Ok(value)
}

pub fn ensure_decoded_size(
    decoded_bytes: usize,
    max_decoded_bytes: usize,
) -> Result<(), &'static str> {
    if decoded_bytes > max_decoded_bytes {
        return Err(DESKTOP_TRANSFER_PAYLOAD_TOO_LARGE);
    }
    Ok(())
}

pub fn bounded_read_length(requested: usize, remaining: u64) -> usize {
    usize::try_from(remaining.min(requested.min(DESKTOP_TRANSFER_MAX_CHUNK_BYTES) as u64))
        .unwrap_or(DESKTOP_TRANSFER_MAX_CHUNK_BYTES)
}

pub fn transfer_is_stale(last_activity: Instant, now: Instant) -> bool {
    now.checked_duration_since(last_activity)
        .is_some_and(|idle| idle >= DESKTOP_TRANSFER_IDLE_TIMEOUT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    #[test]
    fn caps_active_transfers_and_releases_capacity() {
        let budget = Arc::new(DesktopTransferBudget::default());
        let permits = (0..DESKTOP_TRANSFER_MAX_ACTIVE)
            .map(|_| budget.acquire().expect("permit"))
            .collect::<Vec<_>>();

        assert_eq!(
            budget.acquire().expect_err("active transfer limit"),
            DESKTOP_TRANSFER_CAPACITY_EXCEEDED
        );

        drop(permits);
        assert!(budget.acquire().is_ok());
    }

    #[test]
    fn caps_aggregate_pending_bytes_and_releases_them() {
        let budget = Arc::new(DesktopTransferBudget::default());
        let mut first = budget.acquire().expect("first permit");
        let mut second = budget.acquire().expect("second permit");

        first
            .reserve_bytes(DESKTOP_TRANSFER_MAX_PENDING_BYTES)
            .expect("full budget");
        assert_eq!(
            second.reserve_bytes(1),
            Err(DESKTOP_TRANSFER_PAYLOAD_TOO_LARGE)
        );

        drop(first);
        assert!(second
            .reserve_bytes(DESKTOP_TRANSFER_MAX_PENDING_BYTES)
            .is_ok());
    }

    #[test]
    fn bounds_base64_and_decoded_chunk_payloads() {
        let max_encoded = DESKTOP_TRANSFER_MAX_CHUNK_BYTES.div_ceil(3) * 4;
        let valid = "A".repeat(max_encoded);
        let oversized = "A".repeat(max_encoded + 1);

        assert!(ensure_base64_input_size(&valid, DESKTOP_TRANSFER_MAX_CHUNK_BYTES).is_ok());
        assert_eq!(
            ensure_base64_input_size(&oversized, DESKTOP_TRANSFER_MAX_CHUNK_BYTES),
            Err(DESKTOP_TRANSFER_PAYLOAD_TOO_LARGE)
        );
        assert_eq!(
            ensure_decoded_size(
                DESKTOP_TRANSFER_MAX_CHUNK_BYTES + 1,
                DESKTOP_TRANSFER_MAX_CHUNK_BYTES
            ),
            Err(DESKTOP_TRANSFER_PAYLOAD_TOO_LARGE)
        );
    }

    #[test]
    fn clamps_native_read_allocation() {
        assert_eq!(
            bounded_read_length(usize::MAX, u64::MAX),
            DESKTOP_TRANSFER_MAX_CHUNK_BYTES
        );
        assert_eq!(bounded_read_length(32, 12), 12);
    }

    #[test]
    fn expires_idle_transfers() {
        let now = Instant::now();

        assert!(transfer_is_stale(
            now - DESKTOP_TRANSFER_IDLE_TIMEOUT - Duration::from_secs(1),
            now
        ));
        assert!(!transfer_is_stale(
            now - DESKTOP_TRANSFER_IDLE_TIMEOUT + Duration::from_secs(1),
            now
        ));
        assert!(!transfer_is_stale(now, now - Duration::from_secs(1)));
    }

    #[test]
    fn removes_partial_files_unless_preserved() {
        let directory = tempfile::tempdir().expect("tempdir");
        let partial = directory.path().join("partial.bin");
        fs::write(&partial, b"partial").expect("partial file");
        drop(DesktopPartialFile::new(partial.clone()));
        assert!(!partial.exists());

        let completed = directory.path().join("completed.bin");
        fs::write(&completed, b"completed").expect("completed file");
        let mut guard = DesktopPartialFile::new(completed.clone());
        guard.preserve();
        drop(guard);
        assert!(completed.exists());
    }
}
