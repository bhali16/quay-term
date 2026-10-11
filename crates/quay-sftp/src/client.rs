//! Blocking SFTP operations over one SSH connection.

use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use quay_ssh::{connect_raw, ConnectOptions, HostKeyInfo, RawSession};
use serde::Serialize;
use ssh2::{FileStat, OpenFlags, OpenType, Sftp};
use ts_rs::TS;

use crate::error::SftpError;

const CHUNK: usize = 32 * 1024;

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

fn kind_of(stat: &FileStat) -> EntryKind {
    // S_IFMT: 0o170000; S_IFLNK 0o120000.
    match stat.perm.map(|p| p & 0o170000) {
        Some(0o120000) => EntryKind::Symlink,
        _ if stat.is_dir() => EntryKind::Dir,
        _ if stat.is_file() => EntryKind::File,
        _ => EntryKind::Other,
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
    sftp: Sftp,
    // Keeps the connection alive for as long as the client exists.
    _session: RawSession,
}

impl SftpClient {
    /// Connect and start the SFTP subsystem. Blocks; call from a worker thread.
    pub fn connect(
        opts: &ConnectOptions,
        verify: &mut dyn FnMut(&HostKeyInfo) -> bool,
    ) -> Result<Self, SftpError> {
        let session = connect_raw(opts, verify)?;
        let sftp = session
            .sftp()
            .map_err(|e| SftpError::Subsystem(e.message().to_string()))?;
        Ok(Self {
            sftp,
            _session: session,
        })
    }

    /// The server's idea of the starting directory (usually the home folder).
    pub fn home(&self) -> Result<String, SftpError> {
        let p = self
            .sftp
            .realpath(Path::new("."))
            .map_err(|e| SftpError::op("Resolve", ".", e))?;
        Ok(p.to_string_lossy().into_owned())
    }

    /// Resolve `.`/`..` and symlinks in a path.
    pub fn canonical(&self, path: &str) -> Result<String, SftpError> {
        let p = self
            .sftp
            .realpath(Path::new(path))
            .map_err(|e| SftpError::op("Open", path, e))?;
        Ok(p.to_string_lossy().into_owned())
    }

    /// List a directory: folders first, then files, each sorted by name.
    pub fn list(&self, dir: &str) -> Result<Vec<Entry>, SftpError> {
        let rows = self
            .sftp
            .readdir(Path::new(dir))
            .map_err(|e| SftpError::op("Open", dir, e))?;
        let mut out: Vec<Entry> = rows
            .into_iter()
            .filter_map(|(path, mut stat)| {
                let name = path.file_name()?.to_string_lossy().into_owned();
                if name == "." || name == ".." {
                    return None;
                }
                let full = join_remote(dir, &name);
                let mut kind = kind_of(&stat);
                if kind == EntryKind::Symlink {
                    // Show a link to a folder as a folder so it can be opened.
                    if let Ok(target) = self.sftp.stat(Path::new(&full)) {
                        if target.is_dir() {
                            kind = EntryKind::Dir;
                        }
                        stat = target;
                    }
                }
                Some(Entry {
                    name,
                    path: full,
                    kind,
                    size: stat.size.unwrap_or(0),
                    modified: stat.mtime,
                    mode: stat.perm.map(|p| p & 0o7777),
                })
            })
            .collect();
        out.sort_by(|a, b| {
            let dir_first = (b.kind == EntryKind::Dir).cmp(&(a.kind == EntryKind::Dir));
            dir_first.then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        Ok(out)
    }

    pub fn mkdir(&self, path: &str) -> Result<(), SftpError> {
        self.sftp
            .mkdir(Path::new(path), 0o755)
            .map_err(|e| SftpError::op("Create folder", path, e))
    }

    pub fn rename(&self, from: &str, to: &str) -> Result<(), SftpError> {
        self.sftp
            .rename(Path::new(from), Path::new(to), None)
            .map_err(|e| SftpError::op("Rename", from, e))
    }

    /// Delete a file, or a folder and everything inside it.
    pub fn remove(&self, path: &str) -> Result<(), SftpError> {
        let stat = self
            .sftp
            .lstat(Path::new(path))
            .map_err(|e| SftpError::op("Delete", path, e))?;
        if kind_of(&stat) == EntryKind::Dir {
            for child in self.list(path)? {
                self.remove(&child.path)?;
            }
            self.sftp
                .rmdir(Path::new(path))
                .map_err(|e| SftpError::op("Delete", path, e))
        } else {
            self.sftp
                .unlink(Path::new(path))
                .map_err(|e| SftpError::op("Delete", path, e))
        }
    }

    /// Total bytes in a remote file or folder tree.
    pub fn size_of(&self, path: &str) -> Result<u64, SftpError> {
        let stat = self
            .sftp
            .stat(Path::new(path))
            .map_err(|e| SftpError::op("Open", path, e))?;
        if stat.is_dir() {
            let mut sum = 0;
            for child in self.list(path)? {
                sum += self.size_of(&child.path)?;
            }
            Ok(sum)
        } else {
            Ok(stat.size.unwrap_or(0))
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
        let stat = self
            .sftp
            .stat(Path::new(remote))
            .map_err(|e| SftpError::op("Download", remote, e))?;
        if stat.is_dir() {
            std::fs::create_dir_all(local).map_err(|e| SftpError::local(local, e))?;
            for child in self.list(remote)? {
                self.download(&child.path, &local.join(&child.name), cancel, progress)?;
            }
            return Ok(());
        }
        let mut src = self
            .sftp
            .open(Path::new(remote))
            .map_err(|e| SftpError::op("Download", remote, e))?;
        let mut dst = File::create(local).map_err(|e| SftpError::local(local, e))?;
        let mut buf = vec![0u8; CHUNK];
        loop {
            if cancel.load(Ordering::Relaxed) {
                drop(dst);
                let _ = std::fs::remove_file(local);
                return Err(SftpError::Cancelled);
            }
            let n = src.read(&mut buf).map_err(|e| SftpError::Op {
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
            if self.sftp.stat(Path::new(remote)).is_err() {
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
        let mut src = File::open(local).map_err(|e| SftpError::local(local, e))?;
        let mut dst = self
            .sftp
            .open_mode(
                Path::new(remote),
                OpenFlags::WRITE | OpenFlags::CREATE | OpenFlags::TRUNCATE,
                0o644,
                OpenType::File,
            )
            .map_err(|e| SftpError::op("Upload", remote, e))?;
        let mut buf = vec![0u8; CHUNK];
        loop {
            if cancel.load(Ordering::Relaxed) {
                drop(dst);
                let _ = self.sftp.unlink(Path::new(remote));
                return Err(SftpError::Cancelled);
            }
            let n = src.read(&mut buf).map_err(|e| SftpError::local(local, e))?;
            if n == 0 {
                return Ok(());
            }
            dst.write_all(&buf[..n]).map_err(|e| SftpError::Op {
                op: "Upload",
                path: remote.to_string(),
                detail: e.to_string(),
            })?;
            progress(n as u64);
        }
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
