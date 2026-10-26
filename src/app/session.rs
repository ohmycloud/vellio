//! Native UI recovery, separate from committed schema-2 reader edits/notes.
use crate::store::{read_json, write_json};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EditDraft {
    pub text: String,
    pub base: String,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct NoteDraft {
    pub body: String,
    pub key: String,
    pub quote: String,
    pub kind: String,
    pub prefix: String,
    pub suffix: String,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Draft {
    pub edit: Option<EditDraft>,
    pub note: Option<NoteDraft>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Position {
    pub block: String,
    /// Logical pixels into this block. Reflow may change the visual location.
    pub offset: f32,
    pub at: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Appearance {
    pub font_size: f32,
    pub line_width: f32,
    pub line_spacing: f32,
    pub serif: bool,
    // An absent font retains the legacy serif/sans preference.
    #[serde(default)]
    pub font: Option<ReadingFont>,
    pub margin_notes: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReadingFont {
    SystemSerif,
    SansSerif,
    #[default]
    #[serde(other)]
    NotoSerifSc,
}
impl ReadingFont {
    pub const ALL: [Self; 3] = [Self::NotoSerifSc, Self::SystemSerif, Self::SansSerif];
    pub fn label(self) -> &'static str {
        match self {
            Self::NotoSerifSc => "宋体",
            Self::SystemSerif => "系统衬线",
            Self::SansSerif => "无衬线",
        }
    }
}
impl Default for Appearance {
    fn default() -> Self {
        Self {
            font_size: 21.,
            line_width: 35.,
            line_spacing: 1.9,
            serif: true,
            font: Some(ReadingFont::NotoSerifSc),
            margin_notes: true,
        }
    }
}
impl Appearance {
    pub fn reading_font(&self) -> ReadingFont {
        self.font.unwrap_or(if self.serif {
            ReadingFont::NotoSerifSc
        } else {
            ReadingFont::SansSerif
        })
    }
    pub fn set_font(&mut self, font: ReadingFont) {
        self.font = Some(font);
        self.serif = font != ReadingFont::SansSerif;
    }
    pub fn normalize(&mut self) {
        let valid = |value: f32, min: f32, max: f32, fallback: f32| {
            if value.is_finite() {
                value.clamp(min, max)
            } else {
                fallback
            }
        };
        self.font_size = valid(self.font_size, 14., 32., 21.).round();
        self.line_width = valid(self.line_width, 25., 60., 35.).round();
        self.line_spacing = (valid(self.line_spacing, 1.2, 2.6, 1.9) * 10.).round() / 10.;
    }
    pub fn text_width(&self) -> f32 {
        self.font_size * self.line_width
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadingMode {
    #[default]
    Original,
    Translation,
    Bilingual,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Session {
    pub drafts: BTreeMap<String, Draft>,
    pub progress: Option<Position>,
    pub bilingual: bool,
    // Missing in older sessions: retain their previous single/bilingual mode.
    #[serde(default)]
    pub reading_mode: Option<ReadingMode>,
    pub original_first: bool,
    pub show_pdf: bool,
    pub appearance: Appearance,
    pub selected_key: Option<String>,
    pub panel: String,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}
impl Default for Session {
    fn default() -> Self {
        Self {
            drafts: BTreeMap::new(),
            progress: None,
            bilingual: false,
            reading_mode: Some(ReadingMode::Original),
            original_first: true,
            show_pdf: true,
            appearance: Appearance::default(),
            selected_key: None,
            panel: "notes".into(),
            extra: BTreeMap::new(),
        }
    }
}
impl Session {
    pub fn resolved_reading_mode(&self) -> ReadingMode {
        self.reading_mode.unwrap_or(if self.bilingual {
            ReadingMode::Bilingual
        } else {
            ReadingMode::Translation
        })
    }

    pub fn load(root: &Path) -> Result<Self> {
        let mut session: Self =
            serde_json::from_value(read_json(&root.join(".native-reader.json"), json!({}))?)?;
        session.appearance.normalize();
        Ok(session)
    }
    pub fn save(&self, root: &Path) -> Result<()> {
        write_json(
            &root.join(".native-reader.json"),
            &serde_json::to_value(self)?,
        )
    }
    pub fn set_draft(&mut self, key: String, draft: Draft) {
        if draft == Draft::default() {
            self.drafts.remove(&key);
        } else {
            self.drafts.insert(key, draft);
        }
    }
    /// A late save acknowledgement must not erase newer typing.
    pub fn acknowledge(&mut self, key: &str, edit: Option<&EditDraft>, note: Option<&NoteDraft>) {
        if let Some(draft) = self.drafts.get_mut(key) {
            if edit.is_some() && draft.edit.as_ref() == edit {
                draft.edit = None;
            }
            if note.is_some() && draft.note.as_ref() == note {
                draft.note = None;
            }
            if *draft == Draft::default() {
                self.drafts.remove(key);
            }
        }
    }
    pub fn position(&self, legacy: &Value, ids: &[&str]) -> Option<(usize, f32)> {
        let native = self
            .progress
            .as_ref()
            .filter(|p| p.offset.is_finite() && (0.0..=1_000_000.0).contains(&p.offset));
        let legacy_at = legacy["at"]
            .as_str()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok());
        let native = native.filter(|p| {
            chrono::DateTime::parse_from_rfc3339(&p.at)
                .is_ok_and(|at| legacy_at.is_none_or(|old| at >= old))
        });
        if let Some(p) = native
            && let Some(index) = ids.iter().position(|id| *id == p.block)
        {
            return Some((index, p.offset));
        }
        let index = ids
            .iter()
            .position(|id| Some(*id) == legacy["block"].as_str())?;
        Some((index, 0.0))
    }
}

/// A successful store transaction can still ignore an older timestamp.
/// Only retire drafts once the requested content is actually present.
pub fn draft_ops_committed(reader: &Value, ops: &[Value]) -> bool {
    ops.iter().all(|op| match op["op"].as_str() {
        Some("edit") => {
            let Some(key) = op["block"].as_str() else {
                return false;
            };
            let edit = &reader["edits"][key];
            if op["zh"].is_null() {
                edit.is_null() || edit["reverted"].as_bool() == Some(true)
            } else {
                edit["zh"] == op["zh"] && edit["reverted"].as_bool() != Some(true)
            }
        }
        Some("note") => {
            let Some(id) = op["note"]["id"].as_str() else {
                return false;
            };
            reader["notes"][id] == op["note"]
        }
        _ => false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn appearance_restores_legacy_defaults_and_bounds_persisted_preferences() {
        let root = tempfile::tempdir().unwrap();
        write_json(
            &root.path().join(".native-reader.json"),
            &json!({"future":42}),
        )
        .unwrap();
        let mut session = Session::load(root.path()).unwrap();
        assert_eq!(session.appearance, Appearance::default());
        session.appearance = Appearance {
            font_size: 25.,
            line_width: 42.,
            line_spacing: 2.2,
            serif: false,
            font: Some(ReadingFont::SansSerif),
            margin_notes: false,
        };
        session.save(root.path()).unwrap();
        assert_eq!(Session::load(root.path()).unwrap(), session);
        write_json(
            &root.path().join(".native-reader.json"),
            &json!({
                "appearance":{"font_size":999,"line_width":-2,"line_spacing":99},
                "future":42
            }),
        )
        .unwrap();
        let session = Session::load(root.path()).unwrap();
        assert_eq!(session.appearance.font_size, 32.);
        assert_eq!(session.appearance.line_width, 25.);
        assert_eq!(session.appearance.line_spacing, 2.6);
        assert!(session.appearance.serif);
        assert!(session.appearance.margin_notes);
        assert_eq!(session.extra["future"], 42);
    }
    #[test]
    fn reading_modes_default_to_original_and_restore_legacy_sessions() {
        assert_eq!(
            Session::default().resolved_reading_mode(),
            ReadingMode::Original
        );
        for (bilingual, expected) in [
            (false, ReadingMode::Translation),
            (true, ReadingMode::Bilingual),
        ] {
            let legacy: Session = serde_json::from_value(json!({"bilingual":bilingual})).unwrap();
            assert_eq!(legacy.resolved_reading_mode(), expected);
            assert!(legacy.original_first);
        }
        let root = tempfile::tempdir().unwrap();
        for mode in [
            ReadingMode::Original,
            ReadingMode::Translation,
            ReadingMode::Bilingual,
        ] {
            let session = Session {
                reading_mode: Some(mode),
                original_first: false,
                ..Default::default()
            };
            session.save(root.path()).unwrap();
            let restored = Session::load(root.path()).unwrap();
            assert_eq!(restored.resolved_reading_mode(), mode);
            assert!(!restored.original_first);
        }
    }

    #[test]
    fn reading_font_preserves_legacy_choices_and_round_trips_explicit_fonts() {
        let legacy: Appearance = serde_json::from_value(json!({"serif":false})).unwrap();
        assert_eq!(legacy.reading_font(), ReadingFont::SansSerif);
        let legacy: Appearance = serde_json::from_value(json!({"serif":true})).unwrap();
        assert_eq!(legacy.reading_font(), ReadingFont::NotoSerifSc);
        assert_eq!(
            Appearance::default().reading_font(),
            ReadingFont::NotoSerifSc
        );
        for font in ReadingFont::ALL {
            let mut appearance = Appearance::default();
            appearance.set_font(font);
            let saved = serde_json::to_value(&appearance).unwrap();
            let restored: Appearance = serde_json::from_value(saved).unwrap();
            assert_eq!(restored.reading_font(), font);
            assert_eq!(restored.serif, font != ReadingFont::SansSerif);
        }
        let future: Appearance = serde_json::from_value(json!({"font":"future-font"})).unwrap();
        assert_eq!(future.reading_font(), ReadingFont::NotoSerifSc);
    }
    #[test]
    fn ignored_or_superseded_save_does_not_acknowledge_draft() {
        let op = json!({"op":"edit","block":"b","zh":"my draft"});
        assert!(!draft_ops_committed(
            &json!({"edits":{"b":{"zh":"other writer"}}}),
            std::slice::from_ref(&op)
        ));
        assert!(draft_ops_committed(
            &json!({"edits":{"b":{"zh":"my draft"}}}),
            &[op]
        ));
        let note = json!({"id":"n","body":"draft","updated":"2026-10-03T00:00:00Z"});
        assert!(draft_ops_committed(
            &json!({"notes":{"n":note}}),
            &[json!({"op":"note","note":note})]
        ));
        assert!(!draft_ops_committed(
            &json!({"notes":{"n":{"id":"n","body":"other"}}}),
            &[json!({"op":"note","note":note})]
        ));
    }
    #[test]
    fn drafts_survive_reopen_and_only_acknowledged_version_is_removed() {
        let root = tempfile::tempdir().unwrap();
        let old = EditDraft {
            text: "草稿🦀".into(),
            base: "base".into(),
        };
        let note = NoteDraft {
            body: "问题".into(),
            key: "b#2".into(),
            quote: "引文".into(),
            kind: "question".into(),
            ..Default::default()
        };
        let mut session = Session::default();
        session.set_draft(
            "b#caption".into(),
            Draft {
                edit: Some(old.clone()),
                note: Some(note.clone()),
            },
        );
        session.extra.insert("future".into(), json!(42));
        session.save(root.path()).unwrap();
        let mut session = Session::load(root.path()).unwrap();
        assert_eq!(session.drafts["b#caption"].note, Some(note.clone()));
        session
            .drafts
            .get_mut("b#caption")
            .unwrap()
            .edit
            .as_mut()
            .unwrap()
            .text = "继续编辑".into();
        session.acknowledge("b#caption", Some(&old), Some(&note));
        assert_eq!(
            session.drafts["b#caption"].edit.as_ref().unwrap().text,
            "继续编辑"
        );
        assert!(session.drafts["b#caption"].note.is_none());
        assert_eq!(session.extra["future"], 42);
        assert!(!root.path().join("reader.json").exists());
    }
    #[test]
    fn progress_prefers_latest_valid_known_block_and_preserves_offset() {
        let mut session = Session {
            progress: Some(Position {
                block: "b".into(),
                offset: 123.5,
                at: "2026-10-03T01:00:00Z".into(),
            }),
            ..Default::default()
        };
        let legacy = json!({"block":"a","at":"2026-10-03T00:00:00Z"});
        assert_eq!(session.position(&legacy, &["a", "b"]), Some((1, 123.5)));
        assert_eq!(
            session.position(
                &json!({"block":"a","at":"2026-10-03T02:00:00Z"}),
                &["a", "b"]
            ),
            Some((0, 0.0))
        );
        session.progress.as_mut().unwrap().offset = -1.0;
        assert_eq!(session.position(&legacy, &["a", "b"]), Some((0, 0.0)));
        session.progress.as_mut().unwrap().offset = 10.0;
        session.progress.as_mut().unwrap().block = "removed".into();
        assert_eq!(session.position(&legacy, &["a", "b"]), Some((0, 0.0)));
    }
    #[test]
    fn malformed_session_is_reported_without_overwriting_it() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join(".native-reader.json"), "{broken").unwrap();
        assert!(Session::load(root.path()).is_err());
        assert_eq!(
            std::fs::read_to_string(root.path().join(".native-reader.json")).unwrap(),
            "{broken"
        );
    }
}
