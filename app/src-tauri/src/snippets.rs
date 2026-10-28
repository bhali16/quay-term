//! The snippet library: named, reusable commands kept in one JSON file.
//!
//! A snippet's `folder` is a path such as `Deploy/Prod`; there is no folder
//! table, so a folder exists while a snippet names it. The body may hold
//! `{{variable}}` placeholders, which the UI asks for when the snippet is sent.

use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::State;
use ts_rs::TS;

use crate::err;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../ui/src/lib/bindings/")]
pub struct Snippet {
    pub id: u32,
    pub name: String,
    pub folder: String,
    pub body: String,
}

#[derive(Default, Serialize, Deserialize)]
struct File {
    snippets: Vec<Snippet>,
}

pub(crate) struct Snippets {
    path: PathBuf,
    items: Mutex<Vec<Snippet>>,
}

impl Snippets {
    /// A missing or unreadable file is an empty library; it is replaced on the next save.
    pub(crate) fn open(path: PathBuf) -> Self {
        let items = std::fs::read(&path)
            .ok()
            .and_then(|b| serde_json::from_slice::<File>(&b).ok())
            .map(|f| f.snippets)
            .unwrap_or_default();
        Self { path, items: Mutex::new(items) }
    }

    fn persist(&self, items: &[Snippet]) -> Result<(), String> {
        let json = serde_json::to_vec_pretty(&File { snippets: items.to_vec() }).map_err(err)?;
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(err)?;
        }
        // Write beside the file and rename, so a crash never leaves half a library.
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, json).map_err(err)?;
        std::fs::rename(&tmp, &self.path).map_err(err)
    }

    fn save(&self, id: Option<u32>, name: &str, folder: &str, body: &str) -> Result<Snippet, String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("A snippet needs a name.".into());
        }
        if body.trim().is_empty() {
            return Err("A snippet needs some text to send.".into());
        }
        let folder = folder
            .split('/')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("/");
        let mut items = self.items.lock().unwrap();
        let mut next = items.clone();
        let snippet = match id {
            Some(id) => {
                let s = next.iter_mut().find(|s| s.id == id).ok_or("That snippet no longer exists.")?;
                s.name = name.into();
                s.folder = folder;
                s.body = body.into();
                s.clone()
            }
            None => {
                let s = Snippet {
                    id: next.iter().map(|s| s.id).max().unwrap_or(0) + 1,
                    name: name.into(),
                    folder,
                    body: body.into(),
                };
                next.push(s.clone());
                s
            }
        };
        self.persist(&next)?;
        *items = next;
        Ok(snippet)
    }

    fn delete(&self, id: u32) -> Result<(), String> {
        let mut items = self.items.lock().unwrap();
        let next: Vec<Snippet> = items.iter().filter(|s| s.id != id).cloned().collect();
        self.persist(&next)?;
        *items = next;
        Ok(())
    }
}

#[tauri::command]
pub(crate) fn snippets_load(s: State<'_, Snippets>) -> Vec<Snippet> {
    s.items.lock().unwrap().clone()
}

#[tauri::command]
pub(crate) fn snippet_save(
    s: State<'_, Snippets>,
    id: Option<u32>,
    name: String,
    folder: String,
    body: String,
) -> Result<Snippet, String> {
    s.save(id, &name, &folder, &body)
}

#[tauri::command]
pub(crate) fn snippet_delete(s: State<'_, Snippets>, id: u32) -> Result<(), String> {
    s.delete(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp() -> PathBuf {
        let d = std::env::temp_dir().join(format!("quay-snippets-{}-{:?}", std::process::id(), std::thread::current().id()));
        let _ = std::fs::remove_dir_all(&d);
        d.join("snippets.json")
    }

    #[test]
    fn saves_edits_deletes_and_reloads() {
        let path = temp();
        let s = Snippets::open(path.clone());
        let a = s.save(None, " Restart ", " ops// web /", "sudo systemctl restart {{unit}}").unwrap();
        assert_eq!((a.id, a.name.as_str(), a.folder.as_str()), (1, "Restart", "ops/web"));
        let b = s.save(None, "Disk", "", "df -h").unwrap();
        assert_eq!(b.id, 2);
        s.save(Some(1), "Restart unit", "ops", "sudo systemctl restart {{unit}}").unwrap();
        s.delete(2).unwrap();
        let again = Snippets::open(path);
        let items = again.items.lock().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name, "Restart unit");
    }

    #[test]
    fn rejects_empty_input_and_unknown_ids() {
        let s = Snippets::open(temp());
        assert!(s.save(None, " ", "", "ls").is_err());
        assert!(s.save(None, "x", "", "  ").is_err());
        assert!(s.save(Some(9), "x", "", "ls").is_err());
    }

    #[test]
    fn a_corrupt_file_is_an_empty_library() {
        let path = temp();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"{not json").unwrap();
        assert!(Snippets::open(path).items.lock().unwrap().is_empty());
    }
}
