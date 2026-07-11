use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use log::{LevelFilter, Log, Metadata, Record};
use tauri::{AppHandle, Manager};
use uuid::Uuid;
use zip::write::SimpleFileOptions;

const LOG_NAME: &str = "stemred.jsonl";
const MAX_LOG_BYTES: u64 = 5 * 1024 * 1024;
const LOG_GENERATIONS: usize = 3;

static LOGGER: OnceLock<JsonLogger> = OnceLock::new();

struct LoggerState {
    directory: PathBuf,
    session_id: String,
}

struct JsonLogger {
    state: Mutex<LoggerState>,
}

impl Log for JsonLogger {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        metadata.target().starts_with("stem::")
    }

    fn log(&self, record: &Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let Ok(state) = self.state.lock() else {
            return;
        };
        let path = state.directory.join(LOG_NAME);
        let _ = rotate_logs(&state.directory);
        let entry = serde_json::json!({
            "timestamp_ms": now_ms(),
            "session_id": state.session_id,
            "level": record.level().as_str(),
            "target": sanitize(record.target(), 80),
            "message": sanitize(&record.args().to_string(), 500)
        });
        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
            let _ = serde_json::to_writer(&mut file, &entry);
            let _ = file.write_all(b"\n");
        }
    }

    fn flush(&self) {}
}

pub struct DesktopDiagnostics {
    directory: PathBuf,
    session_id: String,
    dump_captured: Mutex<bool>,
}

impl DesktopDiagnostics {
    pub fn load(app: &AppHandle) -> Result<Self, String> {
        let directory = app
            .path()
            .app_log_dir()
            .map_err(|error| error.to_string())?
            .join("diagnostics");
        fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
        rotate_logs(&directory)?;
        let session_id = Uuid::new_v4().to_string();
        init_logger(directory.clone(), session_id.clone());
        install_panic_marker(directory.clone(), session_id.clone());
        log::info!(target: "stem::runtime", "session_started version={}", app.package_info().version);
        Ok(Self {
            directory,
            session_id,
            dump_captured: Mutex::new(false),
        })
    }

    pub fn capture_hang_dump(&self) -> Result<(), String> {
        let mut captured = self
            .dump_captured
            .lock()
            .map_err(|_| "diagnostic_state_unavailable".to_string())?;
        if *captured {
            return Ok(());
        }
        let path = self.directory.join(format!("hang-{}.dmp", self.session_id));
        capture_current_process_dump(&path)?;
        *captured = true;
        log::error!(target: "stem::watchdog", "heartbeat_lost dump_saved=true");
        Ok(())
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn export(&self, include_dump: bool) -> Result<String, String> {
        let destination = self
            .directory
            .join(format!("stemred-diagnostics-{}.zip", self.session_id));
        let file = File::create(&destination).map_err(|error| error.to_string())?;
        let mut archive = zip::ZipWriter::new(file);
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

        let metadata = serde_json::to_vec_pretty(&serde_json::json!({
            "session_id": self.session_id,
            "created_at_ms": now_ms(),
            "includes_dump": include_dump
        }))
        .map_err(|error| error.to_string())?;
        archive
            .start_file("metadata.json", options)
            .map_err(|error| error.to_string())?;
        archive
            .write_all(&metadata)
            .map_err(|error| error.to_string())?;

        for entry in fs::read_dir(&self.directory).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            let path = entry.path();
            if path == destination || !path.is_file() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            let allowed = name == LOG_NAME
                || name.starts_with("stemred.jsonl.")
                || name == "panic-marker.json"
                || (include_dump && name == format!("hang-{}.dmp", self.session_id));
            if !allowed {
                continue;
            }
            let mut content = Vec::new();
            File::open(&path)
                .and_then(|mut file| file.read_to_end(&mut content))
                .map_err(|error| error.to_string())?;
            archive
                .start_file(name, options)
                .map_err(|error| error.to_string())?;
            archive
                .write_all(&content)
                .map_err(|error| error.to_string())?;
        }
        archive.finish().map_err(|error| error.to_string())?;
        Ok(destination.to_string_lossy().to_string())
    }
}

fn init_logger(directory: PathBuf, session_id: String) {
    let logger = LOGGER.get_or_init(|| JsonLogger {
        state: Mutex::new(LoggerState {
            directory,
            session_id,
        }),
    });
    let _ = log::set_logger(logger);
    log::set_max_level(LevelFilter::Info);
}

fn install_panic_marker(directory: PathBuf, session_id: String) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let marker = serde_json::json!({
            "timestamp_ms": now_ms(),
            "session_id": session_id,
            "line": info.location().map(|location| location.line())
        });
        let _ = fs::write(
            directory.join("panic-marker.json"),
            serde_json::to_vec(&marker).unwrap_or_default(),
        );
        previous(info);
    }));
}

fn rotate_logs(directory: &Path) -> Result<(), String> {
    let current = directory.join(LOG_NAME);
    let size = fs::metadata(&current)
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    if size < MAX_LOG_BYTES {
        return Ok(());
    }
    let oldest = directory.join(format!("{LOG_NAME}.{LOG_GENERATIONS}"));
    let _ = fs::remove_file(oldest);
    for index in (1..LOG_GENERATIONS).rev() {
        let source = directory.join(format!("{LOG_NAME}.{index}"));
        let target = directory.join(format!("{LOG_NAME}.{}", index + 1));
        if source.exists() {
            fs::rename(source, target).map_err(|error| error.to_string())?;
        }
    }
    fs::rename(current, directory.join(format!("{LOG_NAME}.1"))).map_err(|error| error.to_string())
}

#[cfg(windows)]
fn capture_current_process_dump(path: &Path) -> Result<(), String> {
    use std::os::windows::io::AsRawHandle;
    use std::ptr;
    use windows_sys::Win32::System::Diagnostics::Debug::{
        MiniDumpWithThreadInfo, MiniDumpWriteDump,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetCurrentProcessId};

    let file = File::create(path).map_err(|error| error.to_string())?;
    let ok = unsafe {
        MiniDumpWriteDump(
            GetCurrentProcess(),
            GetCurrentProcessId(),
            file.as_raw_handle(),
            MiniDumpWithThreadInfo,
            ptr::null(),
            ptr::null(),
            ptr::null(),
        )
    };
    if ok == 0 {
        return Err("hang_dump_failed".to_string());
    }
    Ok(())
}

#[cfg(not(windows))]
fn capture_current_process_dump(path: &Path) -> Result<(), String> {
    fs::write(path, b"hang dump is only supported on Windows").map_err(|error| error.to_string())
}

fn sanitize(value: &str, limit: usize) -> String {
    value
        .chars()
        .filter(|character| !character.is_control())
        .take(limit)
        .collect()
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(0)
}
