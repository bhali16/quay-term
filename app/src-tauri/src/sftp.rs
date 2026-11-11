//! Tauri commands for the file panel: one SFTP connection per panel.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use quay_core::SshTarget;
use quay_sftp::{
    Direction, Entry, SftpClient, TransferEvent, TransferId, TransferQueue, TransferRequest,
};
use quay_ssh::ConnectOptions;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use ts_rs::TS;

use crate::library::{Library, Route};
use crate::{challenger, err, forget_prompt, verifier, Prompts, Sessions};

struct Conn {
    client: Arc<SftpClient>,
    queue: TransferQueue,
}

#[derive(Default)]
pub struct Sftps {
    next_id: AtomicU32,
    map: Mutex<HashMap<u32, Arc<Conn>>>,
}

impl Sftps {
    fn get(&self, id: u32) -> Result<Arc<Conn>, String> {
        self.map
            .lock()
            .unwrap()
            .get(&id)
            .cloned()
            .ok_or_else(|| "The file connection was closed.".to_string())
    }
}

#[derive(Serialize, TS)]
#[ts(export, export_to = "../../ui/src/lib/bindings/")]
pub struct SftpOpened {
    pub id: u32,
    pub home: String,
}

/// A download that was queued: its transfer id and the local file it is written to.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../ui/src/lib/bindings/")]
pub struct QueuedDownload {
    pub id: TransferId,
    pub path: String,
}

/// A transfer event tagged with the connection it belongs to.
#[derive(Clone, Serialize, TS)]
#[ts(export, export_to = "../../ui/src/lib/bindings/")]
pub struct SftpTransfer {
    pub conn: u32,
    #[serde(flatten)]
    #[ts(flatten)]
    pub event: TransferEvent,
}

/// Wrap a connected client in a transfer queue and register it.
///
/// `home` is resolved by the caller on a blocking thread: `SftpClient` methods block on the
/// SSH runtime, which panics if called from an async task (the command would then never answer).
fn register(
    app: &AppHandle,
    sftps: &Sftps,
    client: SftpClient,
    home: String,
) -> Result<SftpOpened, String> {
    let id = sftps.next_id.fetch_add(1, Ordering::Relaxed) + 1;
    let client = Arc::new(client);
    let emit_app = app.clone();
    let queue = TransferQueue::new(client.clone(), move |event| {
        let _ = emit_app.emit("sftp-transfer", SftpTransfer { conn: id, event });
    });
    sftps
        .map
        .lock()
        .unwrap()
        .insert(id, Arc::new(Conn { client, queue }));
    Ok(SftpOpened { id, home })
}

async fn open(
    app: AppHandle,
    sftps: &Sftps,
    prompts: &Prompts,
    route: Route,
) -> Result<SftpOpened, String> {
    let (prompt_id, mut verify) = verifier(&app, prompts);
    let mut opts = ConnectOptions::new(route.target);
    opts.jumps = route.jumps;
    opts.challenger = Some(challenger(&app, prompts, prompt_id));
    let result = tauri::async_runtime::spawn_blocking(move || {
        let client = SftpClient::connect(&opts, &mut verify)?;
        let home = client.home()?;
        Ok::<_, quay_sftp::SftpError>((client, home))
    })
    .await
    .map_err(err)?;
    forget_prompt(prompts, prompt_id);
    let (client, home) = result.map_err(err)?;
    register(&app, sftps, client, home)
}

/// Open SFTP on the connection behind terminal tab `session`: no second login.
#[tauri::command]
pub async fn sftp_open_shared(
    app: AppHandle,
    sftps: State<'_, Sftps>,
    sessions: State<'_, Sessions>,
    session: u32,
) -> Result<SftpOpened, String> {
    let conn = sessions
        .conns
        .lock()
        .unwrap()
        .get(&session)
        .cloned()
        .filter(|c| !c.is_closed())
        .ok_or("The terminal connection is closed.")?;
    let (client, home) = tauri::async_runtime::spawn_blocking(move || {
        let client = SftpClient::open(conn)?;
        let home = client.home()?;
        Ok::<_, quay_sftp::SftpError>((client, home))
    })
    .await
    .map_err(err)?
    .map_err(err)?;
    register(&app, &sftps, client, home)
}

#[tauri::command]
pub async fn sftp_open(
    app: AppHandle,
    sftps: State<'_, Sftps>,
    prompts: State<'_, Prompts>,
    target: SshTarget,
) -> Result<SftpOpened, String> {
    open(app, &sftps, &prompts, target.into()).await
}

#[tauri::command]
pub async fn sftp_open_saved(
    app: AppHandle,
    sftps: State<'_, Sftps>,
    prompts: State<'_, Prompts>,
    library: State<'_, Library>,
    id: i64,
    secret: Option<String>,
    jump_secrets: Vec<crate::library::JumpSecret>,
) -> Result<SftpOpened, String> {
    let route = library.route_for(id, secret, false, &jump_secrets)?;
    open(app, &sftps, &prompts, route).await
}

/// Run a blocking SFTP call off the main thread.
async fn blocking<T: Send + 'static>(
    sftps: &Sftps,
    id: u32,
    f: impl FnOnce(&Conn) -> Result<T, quay_sftp::SftpError> + Send + 'static,
) -> Result<T, String> {
    let conn = sftps.get(id)?;
    tauri::async_runtime::spawn_blocking(move || f(&conn))
        .await
        .map_err(err)?
        .map_err(err)
}

#[tauri::command]
pub async fn sftp_list(
    sftps: State<'_, Sftps>,
    id: u32,
    path: String,
) -> Result<Vec<Entry>, String> {
    blocking(&sftps, id, move |c| c.client.list(&path)).await
}

#[tauri::command]
pub async fn sftp_canonical(
    sftps: State<'_, Sftps>,
    id: u32,
    path: String,
) -> Result<String, String> {
    blocking(&sftps, id, move |c| c.client.canonical(&path)).await
}

#[tauri::command]
pub async fn sftp_mkdir(sftps: State<'_, Sftps>, id: u32, path: String) -> Result<(), String> {
    blocking(&sftps, id, move |c| c.client.mkdir(&path)).await
}

#[tauri::command]
pub async fn sftp_rename(
    sftps: State<'_, Sftps>,
    id: u32,
    from: String,
    to: String,
) -> Result<(), String> {
    blocking(&sftps, id, move |c| c.client.rename(&from, &to)).await
}

#[tauri::command]
pub async fn sftp_delete(sftps: State<'_, Sftps>, id: u32, path: String) -> Result<(), String> {
    blocking(&sftps, id, move |c| c.client.remove(&path)).await
}

/// Queue uploads of local files/folders into the remote folder `dir`.
#[tauri::command]
pub fn sftp_upload(
    sftps: State<'_, Sftps>,
    id: u32,
    locals: Vec<String>,
    dir: String,
) -> Result<Vec<TransferId>, String> {
    let conn = sftps.get(id)?;
    Ok(locals
        .into_iter()
        .map(|l| {
            let local = PathBuf::from(l);
            let name = local.file_name().unwrap_or_default().to_string_lossy();
            conn.queue.submit(TransferRequest {
                direction: Direction::Upload,
                remote: quay_sftp::join_remote(&dir, &name),
                local,
            })
        })
        .collect())
}

/// Queue a download into `dir`, or into the user's Downloads folder when none is given.
/// Returns the transfer and the local file it goes to (never over a file that is already there).
#[tauri::command]
pub fn sftp_download(
    app: AppHandle,
    sftps: State<'_, Sftps>,
    id: u32,
    remote: String,
    dir: Option<String>,
) -> Result<QueuedDownload, String> {
    let conn = sftps.get(id)?;
    let dir = match dir {
        Some(d) => {
            let d = PathBuf::from(d);
            if !d.is_dir() {
                return Err(format!("{} is not a folder.", d.display()));
            }
            d
        }
        None => app
            .path()
            .download_dir()
            .map_err(|_| "Could not find your Downloads folder.".to_string())?,
    };
    let name = remote
        .rsplit('/')
        .find(|s| !s.is_empty())
        .unwrap_or("download");
    let local = free_name(&dir, name);
    let path = local.display().to_string();
    let id = conn.queue.submit(TransferRequest {
        direction: Direction::Download,
        local,
        remote,
    });
    Ok(QueuedDownload { id, path })
}

/// `name`, or `name (2)`, `name (3)`... if something is already there.
fn free_name(dir: &Path, name: &str) -> PathBuf {
    let first = dir.join(name);
    if !first.exists() {
        return first;
    }
    let p = Path::new(name);
    let stem = p.file_stem().unwrap_or_default().to_string_lossy();
    let ext = p
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    (2..)
        .map(|n| dir.join(format!("{stem} ({n}){ext}")))
        .find(|c| !c.exists())
        .expect("endless range")
}

#[tauri::command]
pub fn sftp_cancel(sftps: State<'_, Sftps>, id: u32, transfer: TransferId) -> Result<(), String> {
    sftps.get(id)?.queue.cancel(transfer);
    Ok(())
}

#[tauri::command]
pub fn sftp_close(sftps: State<'_, Sftps>, id: u32) {
    sftps.map.lock().unwrap().remove(&id);
}

#[cfg(test)]
mod tests {
    use super::free_name;

    #[test]
    fn picks_a_free_name() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(free_name(dir.path(), "a.txt"), dir.path().join("a.txt"));
        std::fs::write(dir.path().join("a.txt"), "").unwrap();
        assert_eq!(free_name(dir.path(), "a.txt"), dir.path().join("a (2).txt"));
        std::fs::write(dir.path().join("a (2).txt"), "").unwrap();
        assert_eq!(free_name(dir.path(), "a.txt"), dir.path().join("a (3).txt"));
        std::fs::write(dir.path().join("dir"), "").unwrap();
        assert_eq!(free_name(dir.path(), "dir"), dir.path().join("dir (2)"));
    }
}
