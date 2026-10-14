//! Persisted desktop preferences. Model secrets are referenced by environment name.
use crate::{
    ai,
    session::Appearance,
    store::{read_json, write_json},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Features {
    pub ask_ai: bool,
    pub edit: bool,
    pub original: bool,
    pub page: bool,
}
impl Default for Features {
    fn default() -> Self {
        Self {
            ask_ai: true,
            edit: true,
            original: true,
            page: true,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub appearance: Appearance,
    pub bilingual: bool,
    pub features: Features,
    pub shortcuts_enabled: bool,
    pub shortcuts: BTreeMap<String, String>,
    pub models: Vec<ai::ModelProfile>,
    pub translation_model: String,
    pub ask_model: String,
    pub pages_per_batch: usize,
    pub parallel: usize,
    pub ask_above: usize,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            appearance: Appearance::default(),
            bilingual: false,
            features: Features::default(),
            shortcuts_enabled: true,
            shortcuts: default_shortcuts(),
            models: vec![],
            translation_model: String::new(),
            ask_model: String::new(),
            pages_per_batch: 2,
            parallel: 0,
            ask_above: 60,
            extra: BTreeMap::new(),
        }
    }
}
pub const SHORTCUTS: [(&str, &str, &str, &str); 19] = [
    ("next", "Next paragraph", "j", "Reading"),
    ("previous", "Previous paragraph", "k", "Reading"),
    ("bilingual", "Single / bilingual", "b", "Reading"),
    ("contents", "Contents", "t", "Reading"),
    ("larger", "Larger text", "=", "Reading"),
    ("smaller", "Smaller text", "-", "Reading"),
    ("reset_size", "Reset font size", "0", "Reading"),
    ("notes", "Notes panel", "m", "Panels"),
    ("ask", "Ask AI (with current paragraph)", "a", "Panels"),
    ("page", "Original page panel", "o", "Panels"),
    ("previous_page", "Previous original page", "[", "Panels"),
    ("next_page", "Next original page", "]", "Panels"),
    (
        "note",
        "Add a note to the current paragraph",
        "n",
        "Current paragraph",
    ),
    (
        "question",
        "Ask a question about the current paragraph",
        "q",
        "Current paragraph",
    ),
    (
        "original",
        "Show this paragraph’s English",
        "y",
        "Current paragraph",
    ),
    ("edit", "Edit translation", "e", "Current paragraph"),
    (
        "retranslate",
        "Retranslate this paragraph",
        "",
        "Current paragraph",
    ),
    (
        "locate_page",
        "See this paragraph’s original page",
        "p",
        "Current paragraph",
    ),
    ("copy", "Copy this translation", "c", "Current paragraph"),
];
pub fn default_shortcuts() -> BTreeMap<String, String> {
    SHORTCUTS
        .iter()
        .map(|(id, _, key, _)| (id.to_string(), key.to_string()))
        .collect()
}
impl Preferences {
    pub fn load(root: Option<&Path>) -> Result<Self> {
        let mut value: Self = match root {
            Some(root) => {
                serde_json::from_value(read_json(&root.join(".settings.json"), json!({}))?)?
            }
            None => Self::default(),
        };
        value.appearance.normalize();
        value.pages_per_batch = value.pages_per_batch.clamp(1, 10);
        value.parallel = value.parallel.min(8);
        if value.models.is_empty()
            && let Ok(c) = ai::Config::environment()
        {
            let p = ai::ModelProfile {
                id: "environment".into(),
                name: c.model.clone(),
                model: c.model,
                base_url: c.base_url,
                ..Default::default()
            };
            value.translation_model = p.id.clone();
            value.ask_model = p.id.clone();
            value.models.push(p);
        }
        Ok(value)
    }
    pub fn save(&self, root: &Path) -> Result<()> {
        self.validate()?;
        write_json(&root.join(".settings.json"), &serde_json::to_value(self)?)
    }
    pub fn validate(&self) -> Result<()> {
        let mut used = BTreeMap::new();
        for (id, key) in &self.shortcuts {
            if key.is_empty() {
                continue;
            }
            ensure!(
                key.chars().count() == 1
                    && key.chars().all(|c| !c.is_control())
                    && !matches!(key.as_str(), "1" | "2" | "3" | "4"),
                "Use one key; 1–4 are reserved for highlights"
            );
            ensure!(
                used.insert(key.to_lowercase(), id).is_none(),
                "Two shortcuts use the same key: {key}"
            );
        }
        for model in &self.models {
            model.validate()?;
        }
        for id in [&self.translation_model, &self.ask_model] {
            ensure!(
                id.is_empty() || self.models.iter().any(|m| &m.id == id),
                "Default model no longer exists"
            );
        }
        Ok(())
    }
    pub fn connection(&self, translation: bool) -> Result<ai::Connection> {
        let id = if translation {
            &self.translation_model
        } else {
            &self.ask_model
        };
        if self.models.is_empty() {
            return Ok(ai::Connection::api(ai::Config::environment()?));
        }
        let profile = self
            .models
            .iter()
            .find(|m| &m.id == id)
            .context("Choose a default model in Settings → Model")?;
        ai::Connection::from_profile(profile.clone())
    }
    pub fn action(&self, key: &str) -> Option<&str> {
        self.shortcuts_enabled
            .then(|| {
                self.shortcuts
                    .iter()
                    .find(|(id, k)| {
                        !k.is_empty() && k.eq_ignore_ascii_case(key) && self.action_enabled(id)
                    })
                    .map(|(id, _)| id.as_str())
            })
            .flatten()
    }
    pub fn action_enabled(&self, id: &str) -> bool {
        match id {
            "ask" => self.features.ask_ai,
            "edit" => self.features.edit,
            "original" | "bilingual" => self.features.original,
            "page" | "locate_page" | "previous_page" | "next_page" => self.features.page,
            "retranslate" => false,
            _ => true,
        }
    }
}

pub fn location_file() -> PathBuf {
    if let Some(path) = std::env::var_os("EASYREAD_LOCATION_FILE") {
        return path.into();
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    #[cfg(target_os = "macos")]
    let dir = home.join("Library/Application Support/EasyReadGPUI");
    #[cfg(target_os = "windows")]
    let dir = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or(home)
        .join("EasyReadGPUI");
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let dir = home.join(".local/share/easyread-gpui");
    dir.join(".library-location.json")
}
pub fn saved_library() -> Option<PathBuf> {
    let v = read_json(&location_file(), json!({})).ok()?;
    let path = PathBuf::from(v["path"].as_str()?);
    path.is_dir().then_some(path)
}
pub fn remember_library(path: &Path) -> Result<()> {
    let file = location_file();
    std::fs::create_dir_all(file.parent().context("Invalid settings location")?)?;
    write_json(&file, &json!({"path":path}))
}
pub fn cloud_drives() -> Vec<(String, PathBuf)> {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return vec![];
    };
    let mut candidates = vec![
        ("Dropbox".into(), home.join("Dropbox")),
        ("OneDrive".into(), home.join("OneDrive")),
        (
            "iCloud Drive".into(),
            home.join("Library/Mobile Documents/com~apple~CloudDocs"),
        ),
    ];
    if let Ok(entries) = std::fs::read_dir(home.join("Library/CloudStorage")) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if entry.path().is_dir() {
                candidates.push((name, entry.path()))
            }
        }
    }
    candidates.retain(|(_, p)| p.is_dir());
    candidates.sort_by(|a, b| a.1.cmp(&b.1));
    candidates.dedup_by(|a, b| a.1 == b.1);
    candidates
}
/// Publish a complete copy to a new destination, retaining the original on every outcome.
pub fn copy_library(source: &Path, destination: &Path, cancel: &AtomicBool) -> Result<()> {
    let source = source.canonicalize()?;
    ensure!(
        !destination.exists(),
        "Destination already exists; choose a new library folder"
    );
    let parent = destination
        .parent()
        .context("Invalid destination")?
        .canonicalize()?;
    ensure!(
        !parent.starts_with(&source),
        "Destination cannot be inside the current library"
    );
    let staging = tempfile::Builder::new()
        .prefix(".easyread-copy-")
        .tempdir_in(&parent)?;
    fn copy(from: &Path, to: &Path, cancel: &AtomicBool) -> Result<()> {
        for entry in std::fs::read_dir(from)? {
            let entry = entry?;
            ensure!(!cancel.load(Ordering::Relaxed), "Library copy cancelled");
            let kind = entry.file_type()?;
            ensure!(
                !kind.is_symlink(),
                "Library contains a symbolic link: {}",
                entry.file_name().to_string_lossy()
            );
            ensure!(
                entry.file_name() != ".write.lock",
                "Library is being written; retry after the task finishes"
            );
            let dest = to.join(entry.file_name());
            if kind.is_dir() {
                std::fs::create_dir(&dest)?;
                copy(&entry.path(), &dest, cancel)?;
            } else if kind.is_file() {
                std::fs::copy(entry.path(), dest)?;
            }
        }
        Ok(())
    }
    copy(&source, staging.path(), cancel)?;
    ensure!(!cancel.load(Ordering::Relaxed), "Library copy cancelled");
    // The destination must still be absent when publishing; never merge two libraries.
    ensure!(!destination.exists(), "Destination appeared during copying");
    std::fs::rename(staging.path(), destination)?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preferences_roundtrip_preserves_unknown_fields_and_secret_references() {
        let t = tempfile::tempdir().unwrap();
        let mut p = Preferences::default();
        p.extra.insert("future".into(), json!({"x":1}));
        p.models.push(ai::ModelProfile {
            id: "api".into(),
            name: "Example".into(),
            model: "example".into(),
            ..Default::default()
        });
        p.translation_model = "api".into();
        p.ask_model = "api".into();
        p.save(t.path()).unwrap();
        assert_eq!(Preferences::load(Some(t.path())).unwrap(), p);
        assert!(
            !std::fs::read_to_string(t.path().join(".settings.json"))
                .unwrap()
                .contains("api_key")
        );
    }
    #[test]
    fn shortcut_conflicts_and_feature_gates() {
        let mut p = Preferences::default();
        p.features.ask_ai = false;
        assert_eq!(p.action("a"), None);
        assert_eq!(p.action("J"), Some("next"));
        p.shortcuts_enabled = false;
        assert_eq!(p.action("j"), None);
        p.shortcuts.insert("next".into(), "k".into());
        assert!(p.validate().is_err());
    }
    #[test]
    fn copying_retains_source_and_refuses_merge_and_nested_targets() {
        let t = tempfile::tempdir().unwrap();
        let source = t.path().join("source");
        std::fs::create_dir(&source).unwrap();
        std::fs::write(source.join("note"), "saved").unwrap();
        let dest = t.path().join("copy");
        let cancel = AtomicBool::new(false);
        copy_library(&source, &dest, &cancel).unwrap();
        assert_eq!(std::fs::read_to_string(dest.join("note")).unwrap(), "saved");
        assert!(source.join("note").exists());
        assert!(copy_library(&source, &dest, &cancel).is_err());
        assert!(copy_library(&source, &source.join("nested"), &cancel).is_err());
        cancel.store(true, Ordering::Relaxed);
        assert!(copy_library(&source, &t.path().join("cancel"), &cancel).is_err());
        assert!(!t.path().join("cancel").exists());
    }
}
