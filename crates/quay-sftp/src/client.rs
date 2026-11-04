//! Blocking SFTP operations over one SSH connection.

use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use quay_ssh::{block_on, ConnectOptions, Connection, HostKeyInfo};
use russh_sftp::client::{Config, SftpSession};
use russh_sftp::protocol::{FileType, OpenFlags};
use serde::Serialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use ts_rs::TS;

use crate::error::SftpError;

const CHUNK: usize = 256 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../app/ui/src/lib/bindings/")]
pub enum EntryKind {
    File,
    Dir,
    Symlink,
    Other,
}

/// One item in a remote directory listing.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../../../app/ui/src/lib/bindings/")]
pub struct Entry {
    pub name: String,
    /// Full remote path.
    pub path: String,
    pub kind: EntryKind,
    #[ts(type = "number")]
    pub size: u64,
    /// Seconds since the Unix epoch.
    #[ts(type = "number | null")]
    pub modified: Option<u64>,
    /// Unix permission bits, e.g. 0o644.
    pub mode: Option<u32>,
}

fn kind_of(file_type: FileType) -> EntryKind {
    match file_type {
        FileType::Dir => EntryKind::Dir,
        FileType::File => EntryKind::File,
        FileType::Symlink => EntryKind::Symlink,
        FileType::Other => EntryKind::Other,
    }
}

/// Join remote path parts with `/` (SFTP paths are always POSIX).
pub fn join_remote(dir: &str, name: &str) -> String {
    if dir.ends_with('/') {
        format!("{dir}{name}")
    } else {
        format!("{dir}/{name}")
    }
}

pub struct SftpClient {
    sftp: SftpSession,
    // Keeps the connection alive for as long as the client exists.
    _conn: Arc<Connection>,
}

impl SftpClient {
    /// Connect and start the SFTP subsystem. Blocks; call from a worker thread.
    pub fn connect(
        opts: &ConnectOptions,
        verify: &mut dyn FnMut(&HostKeyInfo) -> bool,
    ) -> Result<Self, SftpError> {
        Self::open(Arc::new(Connection::connect(opts, verify)?))
    }

    /// Start the SFTP subsystem on a connection that something else (a
    /// terminal tab) already opened, so the host needs no second login.
    pub fn open(conn: Arc<Connection>) -> Result<Self, SftpError> {
        let sftp = block_on(async {
            let stream = conn.open_subsystem("sftp").await?;
            let cfg = Config {
                // A slow link or a big directory can take longer than the default 10 s.
                request_timeout_secs: 60,
                ..Config::default()
            };
            SftpSession::new_with_config(stream, cfg)
                .await
                .map_err(|e| SftpError::Subsystem(e.to_string()))
        })?;
        Ok(Self { sftp, _conn: conn })
    }

    /// The server's idea of the starting directory (usually the home folder).
    pub fn home(&self) -> Result<String, SftpError> {
        block_on(self.sftp.canonicalize(".")).map_err(|e| SftpError::op("Resolve", ".", e))
    }

    /// Resolve `.`/`..` and symlinks in a path.
    pub fn canonical(&self, path: &str) -> Result<String, SftpError> {
        block_on(self.sftp.canonicalize(path)).map_err(|e| SftpError::op("Open", path, e))
    }

    /// List a directory: folders first, then files, each sorted by name.
    pub fn list(&self, dir: &str) -> Result<Vec<Entry>, SftpError> {
        block_on(async {
            let rows = self
                .sftp
                .read_dir(dir)
                .await
                .map_err(|e| SftpError::op("Open", dir, e))?;
            let mut out = Vec::new();
            for row in rows {
                let name = row.file_name();
                if name == "." || name == ".." {
                    continue;
                }
                let full = join_remote(dir, &name);
                let mut attrs = row.metadata();
                let mut kind = kind_of(attrs.file_type());
                if kind == EntryKind::Symlink {
                    // Show a link to a folder as a folder so it can be opened.
                    if let Ok(target) = self.sftp.metadata(full.as_str()).await {
                        if target.is_dir() {
                            kind = EntryKind::Dir;
                        }
                        attrs = target;
                    }
                }
                out.push(Entry {
                    name,
                    path: full,
                    kind,
                    size: attrs.size.unwrap_or(0),
                    modified: attrs.mtime.map(u64::from),
                    mode: attrs.permissions.map(|p| p & 0o7777),
                });
            }
            out.sort_by(|a, b| {
                let dir_first = (b.kind == EntryKind::Dir).cmp(&(a.kind == EntryKind::Dir));
                dir_first.then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            });
            Ok(out)
        })
    }

    pub fn mkdir(&self, path: &str) -> Result<(), SftpError> {
        block_on(self.sftp.create_dir(path)).map_err(|e| SftpError::op("Create folder", path, e))
    }

    pub fn rename(&self, from: &str, to: &str) -> Result<(), SftpError> {
        block_on(self.sftp.rename(from, to)).map_err(|e| SftpError::op("Rename", from, e))
    }

    /// Delete a file, or a folder and everything inside it.
    pub fn remove(&self, path: &str) -> Result<(), SftpError> {
        let attrs = block_on(self.sftp.symlink_metadata(path))
            .map_err(|e| SftpError::op("Delete", path, e))?;
        if attrs.is_dir() {
            for child in self.list(path)? {
                self.remove(&child.path)?;
            }
            block_on(self.sftp.remove_dir(path)).map_err(|e| SftpError::op("Delete", path, e))
        } else {
            block_on(self.sftp.remove_file(path)).map_err(|e| SftpError::op("Delete", path, e))
        }
    }

    /// Total bytes in a remote file or folder tree.
    pub fn size_of(&self, path: &str) -> Result<u64, SftpError> {
        let attrs =
            block_on(self.sftp.metadata(path)).map_err(|e| SftpError::op("Open", path, e))?;
        if attrs.is_dir() {
            let mut sum = 0;
            for child in self.list(path)? {
                sum += self.size_of(&child.path)?;
            }
            Ok(sum)
        } else {
            Ok(attrs.size.unwrap_or(0))
        }
    }

    /// Download a file or folder to `local` (the full destination path).
    /// `progress` gets the bytes just written; `cancel` aborts between chunks.
    pub fn download(
        &self,
        remote: &str,
        local: &Path,
        cancel: &AtomicBool,
        progress: &mut dyn FnMut(u64),
    ) -> Result<(), SftpError> {
        let attrs = block_on(self.sftp.metadata(remote))
            .map_err(|e| SftpError::op("Download", remote, e))?;
        if attrs.is_dir() {
            std::fs::create_dir_all(local).map_err(|e| SftpError::local(local, e))?;
            for child in self.list(remote)? {
                self.download(&child.path, &local.join(&child.name), cancel, progress)?;
            }
            return Ok(());
        }
        block_on(async {
            let mut src = self
                .sftp
                .open(remote)
                .await
                .map_err(|e| SftpError::op("Download", remote, e))?;
            let mut dst = File::create(local).map_err(|e| SftpError::local(local, e))?;
            let mut buf = vec![0u8; CHUNK];
            loop {
                if cancel.load(Ordering::Relaxed) {
                    drop(dst);
                    let _ = std::fs::remove_file(local);
                    return Err(SftpError::Cancelled);
                }
                let n = src.read(&mut buf).await.map_err(|e| SftpError::Op {
                    op: "Download",
                    path: remote.to_string(),
                    detail: e.to_string(),
                })?;
                if n == 0 {
                    return Ok(());
                }
                dst.write_all(&buf[..n])
                    .map_err(|e| SftpError::local(local, e))?;
                progress(n as u64);
            }
        })
    }

    /// Upload a file or folder to `remote` (the full destination path).
    pub fn upload(
        &self,
        local: &Path,
        remote: &str,
        cancel: &AtomicBool,
        progress: &mut dyn FnMut(u64),
    ) -> Result<(), SftpError> {
        let meta = std::fs::metadata(local).map_err(|e| SftpError::local(local, e))?;
        if meta.is_dir() {
            // An existing folder is fine (merge into it).
            if block_on(self.sftp.metadata(remote)).is_err() {
                self.mkdir(remote)?;
            }
            let mut names: Vec<PathBuf> = std::fs::read_dir(local)
                .map_err(|e| SftpError::local(local, e))?
                .filter_map(|e| e.ok().map(|e| e.path()))
                .collect();
            names.sort();
            for child in names {
                let name = child.file_name().unwrap_or_default().to_string_lossy();
                self.upload(&child, &join_remote(remote, &name), cancel, progress)?;
            }
            return Ok(());
        }
        block_on(async {
            let mut src = File::open(local).map_err(|e| SftpError::local(local, e))?;
            let mut dst = self
                .sftp
                .open_with_flags(
                    remote,
                    OpenFlags::WRITE | OpenFlags::CREATE | OpenFlags::TRUNCATE,
                )
                .await
                .map_err(|e| SftpError::op("Upload", remote, e))?;
            let write_err = |e: std::io::Error| SftpError::Op {
                op: "Upload",
                path: remote.to_string(),
                detail: e.to_string(),
            };
            let mut buf = vec![0u8; CHUNK];
            loop {
                if cancel.load(Ordering::Relaxed) {
                    drop(dst);
                    let _ = self.sftp.remove_file(remote).await;
                    return Err(SftpError::Cancelled);
                }
                let n = src.read(&mut buf).map_err(|e| SftpError::local(local, e))?;
                if n == 0 {
                    // Writes are pipelined; shutdown waits for the server to confirm them all.
                    return dst.shutdown().await.map_err(write_err);
                }
                dst.write_all(&buf[..n]).await.map_err(write_err)?;
                progress(n as u64);
            }
        })
    }
}

/// Total bytes in a local file or folder tree.
pub fn local_size(path: &Path) -> std::io::Result<u64> {
    let meta = std::fs::metadata(path)?;
    if meta.is_dir() {
        let mut sum = 0;
        for e in std::fs::read_dir(path)? {
            sum += local_size(&e?.path())?;
        }
        Ok(sum)
    } else {
        Ok(meta.len())
    }
}
