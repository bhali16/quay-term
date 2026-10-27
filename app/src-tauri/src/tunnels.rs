//! Tauri commands for port forwards: they run on a terminal tab's SSH
//! connection and stop when the tab's shell ends.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

use quay_core::TunnelSpec;
use quay_ssh::Tunnel;
use serde::Serialize;
use tauri::State;
use ts_rs::TS;

use crate::{err, Sessions};

enum Slot {
    Running(Tunnel),
    /// An auto-start that did not come up, kept so the user can see why.
    Failed(String),
}

struct Entry {
    session: u32,
    spec: TunnelSpec,
    slot: Slot,
}

#[derive(Default)]
pub(crate) struct Tunnels {
    next_id: AtomicU32,
    map: Mutex<BTreeMap<u32, Entry>>,
}

#[derive(Serialize, TS)]
#[ts(export, export_to = "../../ui/src/lib/bindings/")]
pub struct TunnelInfo {
    pub id: u32,
    pub spec: TunnelSpec,
    /// The port really listening (differs from the spec when it asked for 0).
    pub bound_port: u16,
    /// Why it is not running, if it is not.
    pub error: Option<String>,
    pub active: u32,
    pub total: u32,
    #[ts(type = "number")]
    pub bytes_up: u64,
    #[ts(type = "number")]
    pub bytes_down: u64,
}

impl Tunnels {
    pub(crate) fn stop_session(&self, session: u32) {
        self.map.lock().unwrap().retain(|_, e| e.session != session);
    }

    fn add(&self, session: u32, spec: TunnelSpec, slot: Slot) -> u32 {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        self.map.lock().unwrap().insert(
            id,
            Entry {
                session,
                spec,
                slot,
            },
        );
        id
    }

    fn info(id: u32, e: &Entry) -> TunnelInfo {
        let (bound_port, error, counts) = match &e.slot {
            Slot::Running(t) => (t.bound_port(), None, t.counts()),
            Slot::Failed(m) => (0, Some(m.clone()), Default::default()),
        };
        TunnelInfo {
            id,
            spec: e.spec.clone(),
            bound_port,
            error,
            active: counts.active as u32,
            total: counts.total.min(u32::MAX as u64) as u32,
            bytes_up: counts.bytes_up,
            bytes_down: counts.bytes_down,
        }
    }
}

/// Start the saved auto-start tunnels for a session that just connected.
/// A failure is recorded, not raised: the shell is already up.
pub(crate) fn start_auto(
    sessions: &Sessions,
    tunnels: &Tunnels,
    session: u32,
    specs: Vec<TunnelSpec>,
) {
    let Some(conn) = sessions.conns.lock().unwrap().get(&session).cloned() else {
        return;
    };
    for spec in specs {
        match conn.start_tunnel(&spec) {
            Ok(t) => {
                log::info!("tunnel up: {}", spec.describe());
                tunnels.add(session, spec, Slot::Running(t));
            }
            Err(e) => {
                log::warn!("tunnel {} failed: {e}", spec.describe());
                tunnels.add(session, spec, Slot::Failed(e.to_string()));
            }
        }
    }
}

/// Start a forward on terminal tab `session`'s connection.
#[tauri::command]
pub async fn tunnel_start(
    sessions: State<'_, Sessions>,
    tunnels: State<'_, Tunnels>,
    session: u32,
    spec: TunnelSpec,
) -> Result<TunnelInfo, String> {
    let conn = sessions
        .conns
        .lock()
        .unwrap()
        .get(&session)
        .cloned()
        .filter(|c| !c.is_closed())
        .ok_or("The connection for this tab is closed.")?;
    let wanted = spec.clone();
    let tunnel = tauri::async_runtime::spawn_blocking(move || conn.start_tunnel(&wanted))
        .await
        .map_err(err)?
        .map_err(err)?;
    log::info!("tunnel up: {}", spec.describe());
    let id = tunnels.add(session, spec, Slot::Running(tunnel));
    let map = tunnels.map.lock().unwrap();
    Ok(Tunnels::info(id, &map[&id]))
}

#[tauri::command]
pub fn tunnel_stop(tunnels: State<'_, Tunnels>, id: u32) {
    tunnels.map.lock().unwrap().remove(&id);
}

/// Every tunnel of terminal tab `session`, running or failed.
#[tauri::command]
pub fn tunnel_list(tunnels: State<'_, Tunnels>, session: u32) -> Vec<TunnelInfo> {
    tunnels
        .map
        .lock()
        .unwrap()
        .iter()
        .filter(|(_, e)| e.session == session)
        .map(|(id, e)| Tunnels::info(*id, e))
        .collect()
}
