//! Branded Windows setup for quay-term.
//!
//! The window is a small web UI in the app's own theme. The actual work is done by the
//! regular NSIS installer that Tauri builds, embedded in this executable and run
//! silently, so the uninstaller, registry entries and in-app updates behave exactly as
//! they do with the plain installer.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::Serialize;
use std::{
    fs, io,
    path::{Path, PathBuf},
    process::{Command, ExitStatus},
};
use tauri::{AppHandle, Emitter};
use tauri_plugin_dialog::DialogExt;

static PAYLOAD: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/payload.exe"));
const PRODUCT: &str = "Quay Term";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Info {
    version: &'static str,
    default_dir: String,
    existing: bool,
    has_payload: bool,
}

/// Where the per-user installer puts the app unless told otherwise.
fn default_dir() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(PRODUCT)
}

/// The app's executable inside an install folder: the biggest `.exe` that is not the
/// uninstaller.
fn find_app_exe(dir: &Path) -> Option<PathBuf> {
    fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .filter(|e| {
            let name = e.file_name().to_string_lossy().to_lowercase();
            name.ends_with(".exe") && !name.starts_with("uninstall")
        })
        .max_by_key(|e| e.metadata().map(|m| m.len()).unwrap_or(0))
        .map(|e| e.path())
}

#[tauri::command]
fn info() -> Info {
    let dir = default_dir();
    Info {
        version: env!("CARGO_PKG_VERSION"),
        existing: find_app_exe(&dir).is_some(),
        default_dir: dir.display().to_string(),
        has_payload: !PAYLOAD.is_empty(),
    }
}

#[tauri::command]
fn is_installed(dir: String) -> bool {
    find_app_exe(Path::new(&dir)).is_some()
}

#[tauri::command]
async fn pick_folder(app: AppHandle, current: String) -> Option<String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut dialog = app.dialog().file().set_title("Install Quay Term in");
        if let Some(parent) = Path::new(&current).parent().filter(|p| p.is_dir()) {
            dialog = dialog.set_directory(parent);
        }
        let picked = dialog.blocking_pick_folder()?.into_path().ok()?;
        // Always install into a "Quay Term" folder, so uninstalling never deletes
        // someone's Documents or Desktop.
        let named = picked
            .file_name()
            .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(PRODUCT));
        Some(
            if named { picked } else { picked.join(PRODUCT) }
                .display()
                .to_string(),
        )
    })
    .await
    .ok()
    .flatten()
}

#[tauri::command]
async fn install(app: AppHandle, dir: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || run_install(&app, Path::new(&dir)))
        .await
        .map_err(|e| e.to_string())?
}

fn run_install(app: &AppHandle, dir: &Path) -> Result<String, String> {
    if PAYLOAD.is_empty() {
        return Err("This setup was built without the installer payload.".into());
    }
    if !dir.is_absolute() {
        return Err("Choose a full folder path to install into.".into());
    }
    let _ = app.emit("stage", "extract");
    let tmp = std::env::temp_dir().join(format!("quay-setup-{}", std::process::id()));
    let installer = tmp.join("installer.exe");
    fs::create_dir_all(&tmp)
        .and_then(|()| fs::write(&installer, PAYLOAD))
        .map_err(|e| format!("Could not unpack the installer: {e}"))?;

    let _ = app.emit("stage", "install");
    let status = run_nsis(&installer, dir);
    let _ = fs::remove_dir_all(&tmp);

    match status {
        Ok(s) if s.success() => find_app_exe(dir)
            .map(|p| p.display().to_string())
            .ok_or_else(|| {
                "Setup finished but Quay Term was not found in the install folder.".to_string()
            }),
        Ok(s) => Err(format!(
            "The installer stopped with code {}. If Quay Term is open, close it and try again.",
            s.code().unwrap_or(-1)
        )),
        Err(e) => Err(format!("Could not start the installer: {e}")),
    }
}

/// NSIS only reads `/D=` unquoted and last, so it is passed through verbatim.
#[cfg(windows)]
fn run_nsis(installer: &Path, dir: &Path) -> io::Result<ExitStatus> {
    use std::os::windows::process::CommandExt;
    Command::new(installer)
        .arg("/S")
        .raw_arg(format!("/D={}", dir.display()))
        .status()
}

#[cfg(not(windows))]
fn run_nsis(_installer: &Path, _dir: &Path) -> io::Result<ExitStatus> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "this setup only installs on Windows",
    ))
}

#[tauri::command]
fn launch(app: AppHandle, exe: String) {
    let _ = Command::new(exe).spawn();
    app.exit(0);
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            info,
            is_installed,
            pick_folder,
            install,
            launch
        ])
        .run(tauri::generate_context!())
        .expect("error while running the setup");
}
