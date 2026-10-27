use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../app/ui/src/lib/bindings/")]
pub enum AuthKind {
    Password,
    Key,
    Agent,
}

impl AuthKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Password => "password",
            Self::Key => "key",
            Self::Agent => "agent",
        }
    }

    /// `None` for a value this version does not know (e.g. written by a newer one).
    pub(crate) fn parse(s: &str) -> Option<Self> {
        match s {
            "password" => Some(Self::Password),
            "key" => Some(Self::Key),
            "agent" => Some(Self::Agent),
            _ => None,
        }
    }
}

/// A folder in the session tree. `parent_id` is `None` at the top level.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/ui/src/lib/bindings/")]
pub struct Folder {
    #[ts(type = "number")]
    pub id: i64,
    pub name: String,
    #[ts(type = "number | null")]
    pub parent_id: Option<i64>,
}

/// What the user edits when creating or changing a saved session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/ui/src/lib/bindings/")]
pub struct SessionInput {
    pub name: String,
    #[ts(type = "number | null")]
    pub folder_id: Option<i64>,
    pub host: String,
    pub port: u16,
    pub user: String,
    pub auth: AuthKind,
    /// Private key path, for `AuthKind::Key`.
    pub key_path: Option<String>,
    /// Display colour as a CSS hex string such as `#3b6fd8`.
    pub color: Option<String>,
    pub tags: Vec<String>,
    /// Another saved session to connect through (a jump host or bastion).
    #[serde(default)]
    #[ts(type = "number | null")]
    pub jump_id: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../app/ui/src/lib/bindings/")]
pub struct SavedSession {
    #[ts(type = "number")]
    pub id: i64,
    #[serde(flatten)]
    pub input: SessionInput,
    /// A password or passphrase is stored in the keychain for this session.
    /// Only a flag: the secret itself never touches the database.
    pub has_secret: bool,
}
