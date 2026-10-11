mod library;
mod sftp;

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{mpsc, Mutex};
use std::time::Duration;

use quay_core::{AppInfo, HostKeyPrompt, Session, SessionExit, SshTarget, TermSize};
use quay_pty::{PtySession, SpawnOptions};
use quay_sessions::{Folder, SavedSession, SessionInput};
use quay_ssh::{ConnectOptions, SshSession};
use quay_vault::KeychainStore;
use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::{AppHandle, Emitter, Manager, State};

use library::{ImportResult, Library, LibraryView, SecretUpdate};
use sftp::Sftps;

/// How long the user has to answer a host-key prompt.
const TRUST_TIMEOUT: Duration = Duration::from_secs(120);

/// Open terminal sessions (local or SSH), keyed by the id handed to the frontend.
#[derive(Default)]
struct Sessions {
    next_id: AtomicU32,
    map: Mutex<HashMap<u32, Box<dyn Session>>>,
}

/// Host-key prompts waiting for the user's answer.
#[derive(Default)]
pub(crate) struct Prompts {
    next_id: AtomicU32,
    pub(crate) waiting: Mutex<HashMap<u32, mpsc::Sender<bool>>>,
}

pub(crate) fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// Register a pending host-key prompt. The returned closure asks the UI (via
/// an event) and waits for `ssh_trust_response`; the caller removes the entry
/// from `prompts.waiting` when the connect call is over.
pub(crate) fn verifier(
    app: &AppHandle,
    prompts: &Prompts,
    host: &str,
    port: u16,
) -> (
    u32,
    impl FnMut(&quay_ssh::HostKeyInfo) -> bool + Send + 'static,
) {
    let app = app.clone();
    let host = host.to_string();
    let prompt_id = prompts.next_id.fetch_add(1, Ordering::Relaxed) + 1;
    let (answer_tx, answer_rx) = mpsc::channel::<bool>();
    prompts.waiting.lock().unwrap().insert(prompt_id, answer_tx);
    let verify = move |info: &quay_ssh::HostKeyInfo| {
        let _ = app.emit(
            "ssh-host-key-prompt",
            HostKeyPrompt {
                prompt_id,
                host: host.clone(),
                port,
                key_type: info.key_type.clone(),
                fingerprint: info.fingerprint.clone(),
            },
        );
        answer_rx.recv_timeout(TRUST_TIMEOUT).unwrap_or(false)
    };
    (prompt_id, verify)
}

#[tauri::command]
fn get_app_info() -> AppInfo {
    AppInfo::current(env!("CARGO_PKG_VERSION"))
}

/// Start a local shell. Output streams over `on_output` as raw bytes; a
/// `session-exit` event carrying the id fires when the shell ends.
#[tauri::command]
fn open_local_session(
    app: AppHandle,
    state: State<'_, Sessions>,
    cols: u16,
    rows: u16,
    on_output: Channel<InvokeResponseBody>,
) -> Result<u32, String> {
    let id = state.next_id.fetch_add(1, Ordering::Relaxed) + 1;
    let session = PtySession::spawn(
        SpawnOptions {
            program: None,
            args: vec![],
            size: TermSize { cols, rows },
        },
        move |chunk| {
            let _ = on_output.send(InvokeResponseBody::Raw(chunk));
        },
        move || {
            let _ = app.emit("session-exit", SessionExit { id, reason: None });
        },
    )
    .map_err(err)?;
    state.map.lock().unwrap().insert(id, Box::new(session));
    Ok(id)
}

/// Connect over SSH and start a shell. Blocks (on a worker thread) through
/// the handshake, the host-key prompt and authentication; the error string is
/// meant to be shown to the user as-is.
#[tauri::command]
async fn open_ssh_session(
    app: AppHandle,
    sessions: State<'_, Sessions>,
    prompts: State<'_, Prompts>,
    target: SshTarget,
    cols: u16,
    rows: u16,
    on_output: Channel<InvokeResponseBody>,
) -> Result<u32, String> {
    start_ssh(app, &sessions, &prompts, target, cols, rows, on_output).await
}

/// Connect to a saved session. The stored secret is read from the keychain
/// here and never reaches the webview; `secret` is one the user just typed.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
async fn open_saved_session(
    app: AppHandle,
    sessions: State<'_, Sessions>,
    prompts: State<'_, Prompts>,
    library: State<'_, Library>,
    id: i64,
    secret: Option<String>,
    remember: bool,
    cols: u16,
    rows: u16,
    on_output: Channel<InvokeResponseBody>,
) -> Result<u32, String> {
    let target = library.target_for(id, secret, remember)?;
    start_ssh(app, &sessions, &prompts, target, cols, rows, on_output).await
}

async fn start_ssh(
    app: AppHandle,
    sessions: &Sessions,
    prompts: &Prompts,
    target: SshTarget,
    cols: u16,
    rows: u16,
    on_output: Channel<InvokeResponseBody>,
) -> Result<u32, String> {
    let id = sessions.next_id.fetch_add(1, Ordering::Relaxed) + 1;
    let (prompt_id, verify) = verifier(&app, prompts, &target.host, target.port);
    let dest = format!("{}@{}:{}", target.user, target.host, target.port);
    log::info!("ssh connect {dest}");

    let result = tauri::async_runtime::spawn_blocking(move || {
        SshSession::connect(
            &ConnectOptions::new(target),
            TermSize { cols, rows },
            verify,
            move |chunk| {
                let _ = on_output.send(InvokeResponseBody::Raw(chunk));
            },
            move |reason| {
                let _ = app.emit("session-exit", SessionExit { id, reason });
            },
        )
    })
    .await
    .map_err(err)?;
    prompts.waiting.lock().unwrap().remove(&prompt_id);

    let session = result.map_err(|e| {
        log::warn!("ssh connect {dest} failed: {e}");
        err(e)
    })?;
    log::info!("ssh connected {dest} (session {id})");
    sessions.map.lock().unwrap().insert(id, Box::new(session));
    Ok(id)
}

#[tauri::command]
fn library_load(library: State<'_, Library>, query: Option<String>) -> Result<LibraryView, String> {
    library.load(query.as_deref().filter(|q| !q.trim().is_empty()))
}

#[tauri::command]
fn session_save(
    library: State<'_, Library>,
    id: Option<i64>,
    input: SessionInput,
    secret: SecretUpdate,
) -> Result<SavedSession, String> {
    library.save(id, &input, secret)
}

#[tauri::command]
fn session_delete(library: State<'_, Library>, id: i64) -> Result<(), String> {
    library.delete(id)
}

#[tauri::command]
fn folder_create(
    library: State<'_, Library>,
    name: String,
    parent_id: Option<i64>,
) -> Result<Folder, String> {
    library.create_folder(&name, parent_id)
}

#[tauri::command]
fn folder_rename(library: State<'_, Library>, id: i64, name: String) -> Result<(), String> {
    library.rename_folder(id, &name)
}

#[tauri::command]
fn session_move(
    library: State<'_, Library>,
    id: i64,
    folder_id: Option<i64>,
) -> Result<(), String> {
    library.move_session(id, folder_id)
}

#[tauri::command]
fn folder_move(library: State<'_, Library>, id: i64, parent_id: Option<i64>) -> Result<(), String> {
    library.move_folder(id, parent_id)
}

#[tauri::command]
fn folder_delete(library: State<'_, Library>, id: i64) -> Result<(), String> {
    library.delete_folder(id)
}

/// Import hosts from `~/.ssh/config`.
#[tauri::command]
fn import_ssh_config(library: State<'_, Library>) -> Result<ImportResult, String> {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .ok_or("Could not find your home directory.")?;
    library.import_ssh_config(&std::path::Path::new(&home).join(".ssh").join("config"))
}

/// The user's answer to an `ssh-host-key-prompt` event.
#[tauri::command]
fn ssh_trust_response(prompts: State<'_, Prompts>, prompt_id: u32, trust: bool) {
    if let Some(tx) = prompts.waiting.lock().unwrap().get(&prompt_id) {
        let _ = tx.send(trust);
    }
}

#[tauri::command]
fn session_write(state: State<'_, Sessions>, id: u32, data: Vec<u8>) -> Result<(), String> {
    let mut map = state.map.lock().unwrap();
    map.get_mut(&id)
        .ok_or("no such session")?
        .write(&data)
        .map_err(err)
}

#[tauri::command]
fn session_resize(state: State<'_, Sessions>, id: u32, cols: u16, rows: u16) -> Result<(), String> {
    let mut map = state.map.lock().unwrap();
    map.get_mut(&id)
        .ok_or("no such session")?
        .resize(TermSize { cols, rows })
        .map_err(err)
}

#[tauri::command]
fn session_close(state: State<'_, Sessions>, id: u32) {
    // Dropping the session ends it (kills the child / closes the channel).
    state.map.lock().unwrap().remove(&id);
}

/// Where settings.json lives, kept in managed state.
struct SettingsPath(std::path::PathBuf);

#[tauri::command]
fn settings_load(path: State<'_, SettingsPath>) -> quay_config::Settings {
    quay_config::Settings::load(&path.0)
}

#[tauri::command]
fn settings_save(
    path: State<'_, SettingsPath>,
    settings: quay_config::Settings,
) -> Result<quay_config::Settings, String> {
    let settings = settings.sanitized();
    settings.save(&path.0).map_err(err)?;
    Ok(settings)
}

/// Facts worth pasting into a bug report. Never includes secrets or host names.
#[derive(serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../../app/ui/src/lib/bindings/")]
struct Diagnostics {
    version: String,
    os: String,
    arch: String,
    settings_file: String,
    log_dir: String,
}

#[tauri::command]
fn get_diagnostics(app: AppHandle, path: State<'_, SettingsPath>) -> Diagnostics {
    Diagnostics {
        version: env!("CARGO_PKG_VERSION").into(),
        os: std::env::consts::OS.into(),
        arch: std::env::consts::ARCH.into(),
        settings_file: path.0.display().to_string(),
        log_dir: app
            .path()
            .app_log_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_default(),
    }
}

pub fn run() {
    tauri::Builder::default()
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                .target(tauri_plugin_log::Target::new(
                    tauri_plugin_log::TargetKind::LogDir {
                        file_name: Some("quay-term".into()),
                    },
                ))
                .target(tauri_plugin_log::Target::new(
                    tauri_plugin_log::TargetKind::Stdout,
                ))
                .max_file_size(1_000_000)
                .rotation_strategy(tauri_plugin_log::RotationStrategy::KeepOne)
                .build(),
        )
        .plugin(tauri_plugin_dialog::init())
        .manage(Sessions::default())
        .manage(Prompts::default())
        .manage(Sftps::default())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            let library = Library::open(&dir.join("sessions.db"), Box::new(KeychainStore::new()))?;
            app.manage(library);
            app.manage(SettingsPath(dir.join("settings.json")));
            log::info!(
                "quay-term {} starting on {}",
                env!("CARGO_PKG_VERSION"),
                std::env::consts::OS
            );
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_app_info,
            settings_load,
            settings_save,
            get_diagnostics,
            open_local_session,
            open_ssh_session,
            ssh_trust_response,
            open_saved_session,
            library_load,
            session_save,
            session_delete,
            folder_create,
            folder_rename,
            folder_delete,
            session_move,
            folder_move,
            import_ssh_config,
            session_write,
            session_resize,
            session_close,
            sftp::sftp_open,
            sftp::sftp_open_saved,
            sftp::sftp_list,
            sftp::sftp_canonical,
            sftp::sftp_mkdir,
            sftp::sftp_rename,
            sftp::sftp_delete,
            sftp::sftp_upload,
            sftp::sftp_download,
            sftp::sftp_cancel,
            sftp::sftp_close
        ])
        .run(tauri::generate_context!())
        .expect("error while running quay-term");
}
