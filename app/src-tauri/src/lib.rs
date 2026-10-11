mod library;

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
struct Prompts {
    next_id: AtomicU32,
    waiting: Mutex<HashMap<u32, mpsc::Sender<bool>>>,
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
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
    let host = target.host.clone();
    let port = target.port;
    let prompt_app = app.clone();
    let prompt_id = prompts.next_id.fetch_add(1, Ordering::Relaxed) + 1;
    let (answer_tx, answer_rx) = mpsc::channel::<bool>();
    prompts.waiting.lock().unwrap().insert(prompt_id, answer_tx);

    let result = tauri::async_runtime::spawn_blocking(move || {
        SshSession::connect(
            &ConnectOptions::new(target),
            TermSize { cols, rows },
            |info| {
                let _ = prompt_app.emit(
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
            },
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

    let session = result.map_err(err)?;
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

pub fn run() {
    tauri::Builder::default()
        .manage(Sessions::default())
        .manage(Prompts::default())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            let library = Library::open(&dir.join("sessions.db"), Box::new(KeychainStore::new()))?;
            app.manage(library);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_app_info,
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
            session_close
        ])
        .run(tauri::generate_context!())
        .expect("error while running quay-term");
}
