//! SFTP operations and a transfer queue for quay-term.
//!
//! [`SftpClient`] opens its own SSH connection (separate from any terminal
//! tab) and exposes blocking file operations. [`TransferQueue`] runs uploads
//! and downloads on a small worker pool and reports progress.

mod client;
mod error;
mod queue;

pub use client::{join_remote, Entry, EntryKind, SftpClient};
pub use error::SftpError;
pub use queue::{
    Direction, TransferEvent, TransferId, TransferQueue, TransferRequest, TransferState,
};
