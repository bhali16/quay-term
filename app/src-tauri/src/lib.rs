use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

use quay_core::{AppInfo, Session, TermSize};
use quay_pty::{PtySession, SpawnOptions};
use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::{AppHandle, Emitter, State};

/// Open terminal sessions, keyed by the id handed to the frontend.
#[derive(Default)]
struct Sessions {
    next_id: AtomicU32,
    map: Mutex<HashMap<u32, PtySession>>,
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
            let _ = app.emit("session-exit", id);
        },
    )
    .map_err(err)?;
    state.map.lock().unwrap().insert(id, session);
    Ok(id)
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
    // Dropping the session kills the child.
    state.map.lock().unwrap().remove(&id);
}

pub fn run() {
    tauri::Builder::default()
        .manage(Sessions::default())
        .invoke_handler(tauri::generate_handler![
            get_app_info,
            open_local_session,
            session_write,
            session_resize,
            session_close
        ])
        .run(tauri::generate_context!())
        .expect("error while running quay-term");
}
