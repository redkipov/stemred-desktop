use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};
use tempfile::NamedTempFile;
use uuid::Uuid;

use crate::desktop_diagnostics::DesktopDiagnostics;

const UPDATE_EVENT: &str = "stem://desktop-update-state";
const CHECK_MIN_INTERVAL_MS: u64 = 60_000;
const CHECK_FOCUS_TTL_MS: u64 = 6 * 60 * 60 * 1_000;
const CHECK_FALLBACK_INTERVAL_MS: u64 = 15 * 60 * 1_000;
const CHECK_RETRY_MS: [u64; 4] = [60_000, 5 * 60_000, 30 * 60_000, 6 * 60 * 60 * 1_000];
const AUTO_INSTALL_IDLE_SECONDS: u64 = 10 * 60;
const BOOT_FAILURE_WINDOW_MS: u64 = 10 * 60 * 1_000;
const MAX_BOOT_FAILURES: u8 = 2;
const JOURNAL_FILE: &str = "desktop-update-state.json";
const TELEMETRY_ENDPOINT: &str = "https://chat-stem.ru/api/desktop/update-events";

const SAFETY_BLOCKERS: [&str; 7] = [
    "attachment_editor",
    "cache_migration",
    "call",
    "draft",
    "outbox",
    "recording",
    "transfer",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopUpdateSnapshot {
    pub phase: String,
    pub current_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_version: Option<String>,
    pub downloaded_bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress_percent: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mandatory_after: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    pub blockers: Vec<String>,
    pub install_ready: bool,
    pub quarantined: bool,
    pub channel: String,
    pub mandatory: bool,
}

impl DesktopUpdateSnapshot {
    fn idle(current_version: String, channel: String) -> Self {
        Self {
            phase: "idle".to_string(),
            current_version,
            target_version: None,
            downloaded_bytes: 0,
            total_bytes: None,
            progress_percent: None,
            mandatory_after: None,
            error_code: None,
            blockers: Vec::new(),
            install_ready: false,
            quarantined: false,
            channel,
            mandatory: false,
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct DesktopUpdateSafety {
    #[serde(default)]
    pub blockers: Vec<String>,
    #[serde(default)]
    pub window_hidden: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DesktopRuntimeReadiness {
    #[serde(default)]
    pub web_build_id: String,
    #[serde(default)]
    pub route: String,
    #[serde(default)]
    pub cache_ready: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct UpdateJournal {
    install_id: String,
    channel: String,
    pending_version: Option<String>,
    pending_from_version: Option<String>,
    pending_release_id: Option<String>,
    pending_cohort: Option<u64>,
    pending_started_at_ms: Option<u64>,
    boot_started_at_ms: Option<u64>,
    boot_failures: u8,
    quarantined_versions: Vec<String>,
    last_error_code: Option<String>,
    cached_runtime_ready: bool,
}

struct CoordinatorInner {
    snapshot: DesktopUpdateSnapshot,
    safety: DesktopUpdateSafety,
    pending_update: Option<Update>,
    downloaded_bytes: Option<Vec<u8>>,
    check_in_flight: bool,
    install_in_flight: bool,
    last_check_at_ms: u64,
    next_check_at_ms: u64,
    check_failures: usize,
    last_heartbeat_at_ms: u64,
    runtime_visible: bool,
    hang_dump_captured: bool,
    journal: UpdateJournal,
    release_id: Option<String>,
    cohort: Option<u64>,
}

pub struct DesktopUpdateCoordinator {
    inner: Mutex<CoordinatorInner>,
    journal_path: PathBuf,
}

impl DesktopUpdateCoordinator {
    pub fn load(app: &AppHandle) -> Result<Self, String> {
        let data_dir = app
            .path()
            .app_data_dir()
            .map_err(|error| error.to_string())?;
        fs::create_dir_all(&data_dir).map_err(|error| error.to_string())?;
        let journal_path = data_dir.join(JOURNAL_FILE);
        let mut journal = load_journal(&journal_path);

        if Uuid::parse_str(&journal.install_id).is_err() {
            journal.install_id = Uuid::new_v4().to_string();
        }
        if let Some(channel) = requested_channel() {
            journal.channel = channel;
        }
        if journal.channel != "canary" {
            journal.channel = "stable".to_string();
        }

        let current_version = app.package_info().version.to_string();
        let now = now_ms();
        let mut snapshot =
            DesktopUpdateSnapshot::idle(current_version.clone(), journal.channel.clone());

        if journal.pending_version.as_deref() == Some(current_version.as_str()) {
            if let Some(previous_boot) = journal.boot_started_at_ms {
                if now.saturating_sub(previous_boot) <= BOOT_FAILURE_WINDOW_MS {
                    journal.boot_failures = journal.boot_failures.saturating_add(1);
                } else {
                    journal.boot_failures = 1;
                }
            }

            journal.boot_started_at_ms = Some(now);
            snapshot.target_version = Some(current_version.clone());
            snapshot.phase = "verifying_boot".to_string();
            if journal.boot_failures >= MAX_BOOT_FAILURES {
                if !journal.quarantined_versions.contains(&current_version) {
                    journal.quarantined_versions.push(current_version.clone());
                }
                snapshot.phase = "quarantined".to_string();
                snapshot.quarantined = true;
                snapshot.error_code = Some("boot_verification_failed".to_string());
            }
        } else if journal.pending_version.is_some() {
            journal.last_error_code = Some("install_not_applied".to_string());
            journal.pending_version = None;
            journal.pending_from_version = None;
            journal.pending_release_id = None;
            journal.pending_cohort = None;
            journal.pending_started_at_ms = None;
            journal.boot_started_at_ms = None;
            journal.boot_failures = 0;
        }

        persist_journal(&journal_path, &journal)?;
        let fallback_interval = fallback_interval_ms(&journal.install_id);
        let release_id = journal.pending_release_id.clone();
        let cohort = journal.pending_cohort;

        Ok(Self {
            inner: Mutex::new(CoordinatorInner {
                snapshot,
                safety: DesktopUpdateSafety::default(),
                pending_update: None,
                downloaded_bytes: None,
                check_in_flight: false,
                install_in_flight: false,
                last_check_at_ms: 0,
                next_check_at_ms: now.saturating_add(fallback_interval),
                check_failures: 0,
                last_heartbeat_at_ms: now,
                runtime_visible: false,
                hang_dump_captured: false,
                journal,
                release_id,
                cohort,
            }),
            journal_path,
        })
    }

    pub fn snapshot(&self) -> DesktopUpdateSnapshot {
        self.inner
            .lock()
            .map(|inner| inner.snapshot.clone())
            .unwrap_or_else(|_| {
                DesktopUpdateSnapshot::idle("unknown".to_string(), "stable".to_string())
            })
    }

    pub fn set_safety(&self, safety: DesktopUpdateSafety) -> DesktopUpdateSnapshot {
        let blockers = normalize_blockers(safety.blockers);
        let mut inner = self
            .inner
            .lock()
            .expect("desktop update coordinator poisoned");
        inner.safety = DesktopUpdateSafety {
            blockers: blockers.clone(),
            window_hidden: safety.window_hidden,
        };
        inner.snapshot.blockers = blockers;
        inner.snapshot.install_ready = inner.downloaded_bytes.is_some()
            && inner.snapshot.blockers.is_empty()
            && !inner.snapshot.quarantined;
        if inner.downloaded_bytes.is_some() && !inner.snapshot.install_ready {
            inner.snapshot.phase = "waiting_safe_point".to_string();
        }
        inner.snapshot.clone()
    }

    pub fn heartbeat(&self, visible: bool) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.last_heartbeat_at_ms = now_ms();
            inner.runtime_visible = visible;
        }
    }

    pub fn runtime_ready(
        &self,
        app: &AppHandle,
        readiness: &DesktopRuntimeReadiness,
    ) -> Result<DesktopUpdateSnapshot, String> {
        if readiness.route.trim().is_empty() || !readiness.cache_ready {
            return Err("runtime_not_ready".to_string());
        }

        let mut inner = self
            .inner
            .lock()
            .map_err(|_| "update_state_unavailable".to_string())?;
        let current = inner.snapshot.current_version.clone();
        inner.journal.cached_runtime_ready = true;
        if inner.journal.pending_version.as_deref() == Some(current.as_str()) {
            queue_telemetry(app, &inner, "verifying_boot", "succeeded", None, 0);
            inner.journal.pending_version = None;
            inner.journal.pending_from_version = None;
            inner.journal.pending_release_id = None;
            inner.journal.pending_cohort = None;
            inner.journal.pending_started_at_ms = None;
            inner.journal.boot_started_at_ms = None;
            inner.journal.boot_failures = 0;
            inner
                .journal
                .quarantined_versions
                .retain(|version| version != &current);
            inner.journal.last_error_code = None;
            inner.snapshot = DesktopUpdateSnapshot::idle(current, inner.journal.channel.clone());
            inner.snapshot.phase = "complete".to_string();
            log::info!(
                target: "stem::runtime",
                "boot_ok web_build_id={} route={}",
                safe_token(&readiness.web_build_id),
                safe_route(&readiness.route)
            );
        }
        persist_journal(&self.journal_path, &inner.journal)?;
        inner.last_heartbeat_at_ms = now_ms();
        inner.runtime_visible = true;
        Ok(inner.snapshot.clone())
    }

    pub fn check_due(&self) -> bool {
        self.inner
            .lock()
            .map(|inner| !inner.check_in_flight && now_ms() >= inner.next_check_at_ms)
            .unwrap_or(false)
    }

    pub fn cached_runtime_ready(&self) -> bool {
        self.inner
            .lock()
            .map(|inner| inner.journal.cached_runtime_ready)
            .unwrap_or(false)
    }

    pub fn should_auto_install(&self, idle_seconds: u64, window_hidden: bool) -> bool {
        self.inner
            .lock()
            .map(|inner| {
                inner.downloaded_bytes.is_some()
                    && inner.snapshot.install_ready
                    && !inner.install_in_flight
                    && (inner.safety.window_hidden
                        || window_hidden
                        || idle_seconds >= AUTO_INSTALL_IDLE_SECONDS)
            })
            .unwrap_or(false)
    }

    pub fn heartbeat_overdue(&self) -> bool {
        self.inner
            .lock()
            .map(|inner| {
                inner.runtime_visible
                    && !inner.install_in_flight
                    && !inner.hang_dump_captured
                    && now_ms().saturating_sub(inner.last_heartbeat_at_ms) >= 30_000
            })
            .unwrap_or(false)
    }

    pub fn mark_hang_dump_captured(&self) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.hang_dump_captured = true;
        }
    }
}

pub async fn request_check(
    app: &AppHandle,
    coordinator: &DesktopUpdateCoordinator,
    force: bool,
    user_initiated: bool,
) -> Result<DesktopUpdateSnapshot, String> {
    let (install_id, channel) = {
        let mut inner = coordinator
            .inner
            .lock()
            .map_err(|_| "update_state_unavailable".to_string())?;
        let now = now_ms();
        if inner.check_in_flight || inner.install_in_flight {
            return Ok(inner.snapshot.clone());
        }
        if inner.downloaded_bytes.is_some() {
            return Ok(inner.snapshot.clone());
        }
        if force && check_rate_limited(inner.last_check_at_ms, now, user_initiated) {
            return Ok(inner.snapshot.clone());
        }
        if !force
            && now < inner.next_check_at_ms
            && (inner.last_check_at_ms == 0
                || now.saturating_sub(inner.last_check_at_ms) < CHECK_FOCUS_TTL_MS)
        {
            return Ok(inner.snapshot.clone());
        }

        inner.check_in_flight = true;
        inner.last_check_at_ms = now;
        inner.snapshot.phase = "checking".to_string();
        inner.snapshot.error_code = None;
        emit_snapshot(app, &inner.snapshot);
        (
            inner.journal.install_id.clone(),
            inner.journal.channel.clone(),
        )
    };

    log::info!(target: "stem::update", "check_started channel={channel}");
    let check_result = async {
        app.updater_builder()
            .header("X-Stem-Install-Id", install_id)
            .map_err(|_| "update_header_invalid".to_string())?
            .header("X-Stem-Update-Channel", channel)
            .map_err(|_| "update_header_invalid".to_string())?
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|_| "updater_unavailable".to_string())?
            .check()
            .await
            .map_err(|_| "check_failed".to_string())
    }
    .await;

    let update = match check_result {
        Ok(update) => update,
        Err(code) => {
            fail_check(app, coordinator, &code)?;
            return Err(code);
        }
    };

    let Some(update) = update else {
        let mut inner = coordinator
            .inner
            .lock()
            .map_err(|_| "update_state_unavailable".to_string())?;
        inner.check_in_flight = false;
        inner.check_failures = 0;
        inner.next_check_at_ms =
            now_ms().saturating_add(fallback_interval_ms(&inner.journal.install_id));
        let current = inner.snapshot.current_version.clone();
        inner.snapshot = DesktopUpdateSnapshot::idle(current, inner.journal.channel.clone());
        emit_snapshot(app, &inner.snapshot);
        log::info!(target: "stem::update", "check_complete result=no_update");
        return Ok(inner.snapshot.clone());
    };

    let policy = UpdatePolicy::from_update(&update);
    {
        let mut inner = coordinator
            .inner
            .lock()
            .map_err(|_| "update_state_unavailable".to_string())?;
        inner.snapshot.phase = "available".to_string();
        inner.snapshot.target_version = Some(update.version.clone());
        inner.snapshot.mandatory_after = policy.mandatory_after;
        inner.snapshot.mandatory = policy.mandatory;
        inner.snapshot.channel = policy
            .channel
            .unwrap_or_else(|| inner.journal.channel.clone());
        inner.release_id = policy.release_id;
        inner.cohort = policy.cohort;

        if inner.journal.quarantined_versions.contains(&update.version) {
            inner.check_in_flight = false;
            inner.snapshot.phase = "quarantined".to_string();
            inner.snapshot.quarantined = true;
            inner.snapshot.error_code = Some("update_quarantined".to_string());
            emit_snapshot(app, &inner.snapshot);
            return Ok(inner.snapshot.clone());
        }

        inner.snapshot.phase = "downloading".to_string();
        inner.snapshot.downloaded_bytes = 0;
        inner.snapshot.total_bytes = None;
        inner.snapshot.progress_percent = None;
        emit_snapshot(app, &inner.snapshot);
    }

    let mut downloaded = 0u64;
    let bytes = update
        .download(
            |chunk, total| {
                downloaded = downloaded.saturating_add(chunk as u64);
                record_progress(app, coordinator, downloaded, total);
            },
            || {},
        )
        .await;

    match bytes {
        Ok(bytes) => {
            let mut inner = coordinator
                .inner
                .lock()
                .map_err(|_| "update_state_unavailable".to_string())?;
            inner.check_in_flight = false;
            inner.check_failures = 0;
            inner.next_check_at_ms =
                now_ms().saturating_add(fallback_interval_ms(&inner.journal.install_id));
            inner.pending_update = Some(update);
            inner.downloaded_bytes = Some(bytes);
            inner.snapshot.install_ready = inner.snapshot.blockers.is_empty();
            inner.snapshot.error_code = None;
            inner.snapshot.phase = "ready".to_string();
            emit_snapshot(app, &inner.snapshot);
            inner.snapshot.phase = "waiting_safe_point".to_string();
            emit_snapshot(app, &inner.snapshot);
            log::info!(
                target: "stem::update",
                "download_complete target_version={}",
                safe_token(inner.snapshot.target_version.as_deref().unwrap_or("unknown"))
            );
            Ok(inner.snapshot.clone())
        }
        Err(_) => {
            fail_check(app, coordinator, "download_failed")?;
            Err("download_failed".to_string())
        }
    }
}

pub fn apply_update(
    app: &AppHandle,
    coordinator: &DesktopUpdateCoordinator,
    user_initiated: bool,
) -> Result<DesktopUpdateSnapshot, String> {
    let (update, bytes, target_version) = {
        let mut inner = coordinator
            .inner
            .lock()
            .map_err(|_| "update_state_unavailable".to_string())?;
        if inner.install_in_flight {
            return Ok(inner.snapshot.clone());
        }
        if !inner.snapshot.blockers.is_empty() {
            inner.snapshot.phase = "waiting_safe_point".to_string();
            inner.snapshot.error_code = Some("update_blocked".to_string());
            emit_snapshot(app, &inner.snapshot);
            return if user_initiated {
                Err("update_blocked".to_string())
            } else {
                Ok(inner.snapshot.clone())
            };
        }
        if inner.snapshot.quarantined {
            return Err("update_quarantined".to_string());
        }

        let Some(update) = inner.pending_update.take() else {
            return Err("update_not_ready".to_string());
        };
        let Some(bytes) = inner.downloaded_bytes.take() else {
            inner.pending_update = Some(update);
            return Err("update_not_ready".to_string());
        };
        let target_version = inner
            .snapshot
            .target_version
            .clone()
            .unwrap_or_else(|| update.version.clone());

        inner.install_in_flight = true;
        inner.snapshot.phase = "installing".to_string();
        inner.snapshot.install_ready = false;
        inner.snapshot.error_code = None;
        inner.journal.pending_version = Some(target_version.clone());
        inner.journal.pending_from_version = Some(inner.snapshot.current_version.clone());
        inner.journal.pending_release_id = inner.release_id.clone();
        inner.journal.pending_cohort = inner.cohort;
        inner.journal.pending_started_at_ms = Some(now_ms());
        inner.journal.boot_started_at_ms = None;
        inner.journal.boot_failures = 0;
        inner.journal.last_error_code = None;
        persist_journal(&coordinator.journal_path, &inner.journal)?;
        emit_snapshot(app, &inner.snapshot);
        (update, bytes, target_version)
    };

    log::info!(
        target: "stem::update",
        "install_started target_version={} user_initiated={user_initiated}",
        safe_token(&target_version)
    );
    telemetry_event(app, coordinator, "installing", "started", None, 0);

    match update.install(&bytes) {
        Ok(()) => {
            telemetry_event(app, coordinator, "installing", "succeeded", None, 0);
            #[cfg(not(windows))]
            app.restart();
            Ok(coordinator.snapshot())
        }
        Err(_) => {
            telemetry_event(
                app,
                coordinator,
                "installing",
                "failed",
                Some("install_failed"),
                0,
            );
            let mut inner = coordinator
                .inner
                .lock()
                .map_err(|_| "update_state_unavailable".to_string())?;
            inner.install_in_flight = false;
            inner.pending_update = Some(update);
            inner.downloaded_bytes = Some(bytes);
            inner.snapshot.phase = "failed".to_string();
            inner.snapshot.error_code = Some("install_failed".to_string());
            inner.snapshot.install_ready = inner.snapshot.blockers.is_empty();
            inner.journal.pending_version = None;
            inner.journal.pending_from_version = None;
            inner.journal.pending_release_id = None;
            inner.journal.pending_cohort = None;
            inner.journal.pending_started_at_ms = None;
            inner.journal.last_error_code = Some("install_failed".to_string());
            persist_journal(&coordinator.journal_path, &inner.journal)?;
            emit_snapshot(app, &inner.snapshot);
            log::error!(target: "stem::update", "install_failed target_version={}", safe_token(&target_version));
            Err("install_failed".to_string())
        }
    }
}

fn fail_check(
    app: &AppHandle,
    coordinator: &DesktopUpdateCoordinator,
    code: &str,
) -> Result<(), String> {
    let mut inner = coordinator
        .inner
        .lock()
        .map_err(|_| "update_state_unavailable".to_string())?;
    inner.check_in_flight = false;
    inner.check_failures = inner.check_failures.saturating_add(1);
    let retry_index = inner
        .check_failures
        .saturating_sub(1)
        .min(CHECK_RETRY_MS.len() - 1);
    inner.next_check_at_ms = now_ms().saturating_add(CHECK_RETRY_MS[retry_index]);
    inner.snapshot.phase = "failed".to_string();
    inner.snapshot.error_code = Some(code.to_string());
    inner.journal.last_error_code = Some(code.to_string());
    persist_journal(&coordinator.journal_path, &inner.journal)?;
    emit_snapshot(app, &inner.snapshot);
    log::warn!(target: "stem::update", "operation_failed code={}", safe_token(code));
    Ok(())
}

fn record_progress(
    app: &AppHandle,
    coordinator: &DesktopUpdateCoordinator,
    downloaded: u64,
    total: Option<u64>,
) {
    let Ok(mut inner) = coordinator.inner.lock() else {
        return;
    };
    let percent = total
        .filter(|total| *total > 0)
        .map(|total| ((downloaded.saturating_mul(100) / total).min(100)) as u8);
    let changed = percent != inner.snapshot.progress_percent || total != inner.snapshot.total_bytes;
    inner.snapshot.downloaded_bytes = downloaded;
    inner.snapshot.total_bytes = total;
    inner.snapshot.progress_percent = percent;
    if changed {
        emit_snapshot(app, &inner.snapshot);
    }
}

#[derive(Serialize)]
struct DesktopUpdateEvent {
    event_id: String,
    session_id: String,
    release_id: String,
    from_version: String,
    to_version: String,
    channel: String,
    cohort: Option<u64>,
    phase: String,
    result: String,
    error_code: Option<String>,
    duration_ms: u64,
}

pub fn telemetry_event(
    app: &AppHandle,
    coordinator: &DesktopUpdateCoordinator,
    phase: &str,
    result: &str,
    error_code: Option<&str>,
    duration_ms: u64,
) {
    let Ok(inner) = coordinator.inner.lock() else {
        return;
    };
    queue_telemetry(app, &inner, phase, result, error_code, duration_ms);
}

fn queue_telemetry(
    app: &AppHandle,
    inner: &CoordinatorInner,
    phase: &str,
    result: &str,
    error_code: Option<&str>,
    duration_ms: u64,
) {
    let Some(diagnostics) = app.try_state::<DesktopDiagnostics>() else {
        return;
    };
    let release_id = inner
        .journal
        .pending_release_id
        .clone()
        .or_else(|| inner.release_id.clone());
    let Some(release_id) = release_id.filter(|value| !value.is_empty()) else {
        return;
    };
    let event = DesktopUpdateEvent {
        event_id: Uuid::new_v4().to_string(),
        session_id: diagnostics.session_id().to_string(),
        release_id,
        from_version: inner
            .journal
            .pending_from_version
            .clone()
            .unwrap_or_else(|| inner.snapshot.current_version.clone()),
        to_version: inner
            .journal
            .pending_version
            .clone()
            .or_else(|| inner.snapshot.target_version.clone())
            .unwrap_or_else(|| inner.snapshot.current_version.clone()),
        channel: inner.snapshot.channel.clone(),
        cohort: inner.journal.pending_cohort.or(inner.cohort),
        phase: phase.to_string(),
        result: result.to_string(),
        error_code: error_code.map(str::to_string),
        duration_ms: duration_ms.min(604_800_000),
    };
    tauri::async_runtime::spawn(async move {
        let Ok(client) = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
        else {
            return;
        };
        let _ = client.post(TELEMETRY_ENDPOINT).json(&event).send().await;
    });
}

#[derive(Default)]
struct UpdatePolicy {
    release_id: Option<String>,
    channel: Option<String>,
    cohort: Option<u64>,
    mandatory_after: Option<String>,
    mandatory: bool,
}

impl UpdatePolicy {
    fn from_update(update: &Update) -> Self {
        Self::from_json(&update.raw_json)
    }

    fn from_json(raw_json: &serde_json::Value) -> Self {
        let stem = raw_json.get("stem");
        Self {
            release_id: stem
                .and_then(|value| value.get("release_id"))
                .and_then(|value| value.as_str())
                .map(str::to_string),
            channel: stem
                .and_then(|value| value.get("channel"))
                .and_then(|value| value.as_str())
                .map(str::to_string),
            cohort: stem
                .and_then(|value| value.get("cohort"))
                .and_then(|value| value.as_u64()),
            mandatory_after: stem
                .and_then(|value| value.get("mandatory_deadline"))
                .and_then(|value| value.as_str())
                .map(str::to_string),
            mandatory: stem
                .and_then(|value| value.get("mandatory"))
                .and_then(|value| value.as_bool())
                .unwrap_or(false),
        }
    }
}

fn emit_snapshot(app: &AppHandle, snapshot: &DesktopUpdateSnapshot) {
    let _ = app.emit(UPDATE_EVENT, snapshot.clone());
}

fn load_journal(path: &Path) -> UpdateJournal {
    fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn persist_journal(path: &Path, journal: &UpdateJournal) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "update_state_path_invalid".to_string())?;
    fs::create_dir_all(parent).map_err(|_| "update_state_write_failed".to_string())?;
    let mut temporary =
        NamedTempFile::new_in(parent).map_err(|_| "update_state_write_failed".to_string())?;
    serde_json::to_writer(&mut temporary, journal)
        .map_err(|_| "update_state_write_failed".to_string())?;
    temporary
        .flush()
        .map_err(|_| "update_state_write_failed".to_string())?;
    temporary
        .persist(path)
        .map_err(|_| "update_state_write_failed".to_string())?;
    Ok(())
}

fn normalize_blockers(blockers: Vec<String>) -> Vec<String> {
    let mut normalized = blockers
        .into_iter()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| SAFETY_BLOCKERS.contains(&value.as_str()))
        .collect::<Vec<_>>();
    normalized.sort();
    normalized.dedup();
    normalized
}

fn requested_channel() -> Option<String> {
    std::env::args().find_map(|argument| {
        argument
            .strip_prefix("--update-channel=")
            .map(str::trim)
            .filter(|channel| matches!(*channel, "stable" | "canary"))
            .map(str::to_string)
    })
}

fn fallback_interval_ms(install_id: &str) -> u64 {
    let seed = install_id.bytes().fold(0u64, |value, byte| {
        value.wrapping_mul(31).wrapping_add(byte as u64)
    });
    let range = CHECK_FALLBACK_INTERVAL_MS / 5;
    CHECK_FALLBACK_INTERVAL_MS
        .saturating_sub(range)
        .saturating_add(seed % (range.saturating_mul(2).saturating_add(1)))
}

fn check_rate_limited(last_check_at_ms: u64, now: u64, user_initiated: bool) -> bool {
    !user_initiated && now.saturating_sub(last_check_at_ms) < CHECK_MIN_INTERVAL_MS
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(0)
}

fn safe_token(value: &str) -> String {
    value
        .chars()
        .filter(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_')
        })
        .take(80)
        .collect()
}

fn safe_route(value: &str) -> String {
    value
        .split('?')
        .next()
        .unwrap_or("/")
        .chars()
        .filter(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '/' | '-' | '_')
        })
        .take(120)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn coordinator_with_ready_update() -> (tempfile::TempDir, DesktopUpdateCoordinator) {
        let directory = tempfile::tempdir().expect("temporary directory");
        let journal = UpdateJournal {
            install_id: Uuid::new_v4().to_string(),
            channel: "stable".to_string(),
            ..UpdateJournal::default()
        };
        let mut snapshot = DesktopUpdateSnapshot::idle("0.1.31".to_string(), "stable".to_string());
        snapshot.phase = "waiting_safe_point".to_string();
        snapshot.install_ready = true;
        let coordinator = DesktopUpdateCoordinator {
            inner: Mutex::new(CoordinatorInner {
                snapshot,
                safety: DesktopUpdateSafety::default(),
                pending_update: None,
                downloaded_bytes: Some(vec![1, 2, 3]),
                check_in_flight: false,
                install_in_flight: false,
                last_check_at_ms: 0,
                next_check_at_ms: u64::MAX,
                check_failures: 0,
                last_heartbeat_at_ms: now_ms(),
                runtime_visible: false,
                hang_dump_captured: false,
                journal,
                release_id: Some("stable-0.1.32".to_string()),
                cohort: Some(42),
            }),
            journal_path: directory.path().join(JOURNAL_FILE),
        };
        (directory, coordinator)
    }

    #[test]
    fn safety_blockers_are_allowlisted_sorted_and_unique() {
        assert_eq!(
            normalize_blockers(vec![
                "call".to_string(),
                " secret ".to_string(),
                "draft".to_string(),
                "CALL".to_string(),
            ]),
            vec!["call".to_string(), "draft".to_string()]
        );
    }

    #[test]
    fn fallback_interval_is_stable_and_bounded() {
        let interval = fallback_interval_ms("019f50e2-dc4e-7821-bc0b-38de88c23813");
        assert_eq!(
            interval,
            fallback_interval_ms("019f50e2-dc4e-7821-bc0b-38de88c23813")
        );
        assert!(interval >= CHECK_FALLBACK_INTERVAL_MS * 4 / 5);
        assert!(interval <= CHECK_FALLBACK_INTERVAL_MS * 6 / 5);
        assert!(interval <= 18 * 60 * 1_000);
    }

    #[test]
    fn manual_check_bypasses_only_the_background_rate_limit() {
        let last_check = 10_000;
        let now = last_check + 1_000;
        assert!(check_rate_limited(last_check, now, false));
        assert!(!check_rate_limited(last_check, now, true));
        assert!(!check_rate_limited(
            last_check,
            last_check + CHECK_MIN_INTERVAL_MS,
            false
        ));
    }

    #[test]
    fn update_policy_reads_stem_extension() {
        let raw_json = serde_json::json!({
            "stem": {
                "release_id": "canary-0.1.32",
                "channel": "canary",
                "cohort": 42,
                "mandatory": true,
                "mandatory_deadline": "2026-07-12T00:00:00Z"
            }
        });
        let policy = UpdatePolicy::from_json(&raw_json);
        assert_eq!(policy.channel.as_deref(), Some("canary"));
        assert_eq!(policy.release_id.as_deref(), Some("canary-0.1.32"));
        assert_eq!(policy.cohort, Some(42));
        assert!(policy.mandatory);
        assert_eq!(
            policy.mandatory_after.as_deref(),
            Some("2026-07-12T00:00:00Z")
        );
    }

    #[test]
    fn blockers_always_override_manual_and_idle_safe_points() {
        let (_directory, coordinator) = coordinator_with_ready_update();
        assert!(coordinator.should_auto_install(AUTO_INSTALL_IDLE_SECONDS, false));
        let snapshot = coordinator.set_safety(DesktopUpdateSafety {
            blockers: vec!["call".to_string()],
            window_hidden: true,
        });
        assert_eq!(snapshot.phase, "waiting_safe_point");
        assert!(!snapshot.install_ready);
        assert!(!coordinator.should_auto_install(u64::MAX, true));
    }

    #[test]
    fn journal_round_trip_is_atomic_and_does_not_persist_download_bytes() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let path = directory.path().join(JOURNAL_FILE);
        let journal = UpdateJournal {
            install_id: Uuid::new_v4().to_string(),
            channel: "stable".to_string(),
            pending_version: Some("0.1.32".to_string()),
            ..UpdateJournal::default()
        };
        persist_journal(&path, &journal).expect("persist journal");
        let restored = load_journal(&path);
        assert_eq!(restored.pending_version.as_deref(), Some("0.1.32"));
        let raw = fs::read_to_string(path).expect("read journal");
        assert!(!raw.contains("downloaded_bytes"));
    }
}
