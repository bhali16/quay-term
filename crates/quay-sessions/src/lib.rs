//! Saved sessions, folders, tags and `~/.ssh/config` import for quay-term.
//!
//! Sessions live in SQLite. They never hold secrets: passwords and key
//! passphrases are kept in the OS keychain (see `quay-vault`) under the
//! session's id.

mod model;
mod sshconfig;
mod store;

pub use model::{AuthKind, Folder, SavedSession, SessionInput};
pub use sshconfig::{parse_ssh_config, read_ssh_config, ImportedHost};
pub use store::{Error, ImportSummary, SessionStore};
