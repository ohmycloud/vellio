//! Shared classification for persistent margin cards and the book annotations drawer.
use serde_json::Value;
use std::collections::HashMap;

#[derive(Clone, Copy, Default, PartialEq)]
pub enum AnnotationFilter {
    #[default]
    All,
    MyNote,
    Highlights,
    Ai,
    Unanswered,
}

pub fn personal(note: &Value) -> bool {
    matches!(note["kind"].as_str(), Some("note" | "question"))
        || !note["body"].as_str().unwrap_or("").trim().is_empty()
}

pub fn label(note: &Value) -> &'static str {
    if note["kind"] == "question" {
        "My question"
    } else if note["kind"] == "ai" {
        "AI"
    } else if personal(note) {
        "My note"
    } else {
        "My highlight"
    }
}

/// An explicit status overrides legacy answer text, which is kept when a
/// question is reopened so marking it unanswered never destroys its answer.
pub fn answered(note: &Value) -> bool {
    note["answered"]
        .as_bool()
        .unwrap_or_else(|| !note["answer"].as_str().unwrap_or("").trim().is_empty())
}

/// Display creation time in the reader's local timezone. Older records may
/// only have an update time; invalid/missing timestamps should not be invented.
pub fn timestamp(note: &Value) -> Option<String> {
    ["created", "updated"].into_iter().find_map(|field| {
        let time = chrono::DateTime::parse_from_rfc3339(note[field].as_str()?).ok()?;
        Some(
            time.with_timezone(&chrono::Local)
                .format("%m/%d %H:%M")
                .to_string(),
        )
    })
}

pub fn matches(note: &Value, filter: AnnotationFilter) -> bool {
    if note["deleted"].as_bool().unwrap_or(false) {
        return false;
    }
    let ai = note["kind"] == "ai";
    match filter {
        AnnotationFilter::All => true,
        AnnotationFilter::MyNote => !ai && personal(note),
        AnnotationFilter::Highlights => !ai && !personal(note),
        AnnotationFilter::Ai => ai,
        AnnotationFilter::Unanswered => note["kind"] == "question" && !answered(note),
    }
}

pub fn notes(reader: &Value, pending: &HashMap<String, Value>) -> Vec<Value> {
    let mut notes = reader["notes"].as_object().cloned().unwrap_or_default();
    for (id, note) in pending {
        let stamp = |value: &Value| {
            value["updated"]
                .as_str()
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        };
        if stamp(note) >= stamp(notes.get(id).unwrap_or(&Value::Null)) {
            notes.insert(id.clone(), note.clone());
        }
    }
    notes
        .into_iter()
        .filter_map(|(id, mut note)| {
            if !note.is_object() || note["deleted"].as_bool().unwrap_or(false) {
                return None;
            }
            if !note["id"].is_string() {
                note["id"] = id.into();
            }
            Some(note)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn legacy_underline_with_body_is_a_note_and_questions_filter_correctly() {
        let legacy = json!({"kind":"highlight","style":"underline","body":"my understanding"});
        assert_eq!(label(&legacy), "My note");
        assert!(matches(&legacy, AnnotationFilter::MyNote));
        assert!(!matches(&legacy, AnnotationFilter::Highlights));
        let mut question = json!({"kind":"question"});
        assert!(matches(&question, AnnotationFilter::Unanswered));
        question["answer"] = json!("answered");
        assert!(!matches(&question, AnnotationFilter::Unanswered));
        question["deleted"] = json!(true);
        assert!(!matches(&question, AnnotationFilter::All));
    }

    #[test]
    fn explicit_question_status_overrides_legacy_answer_without_losing_text() {
        let mut question = json!({"kind":"question", "answer":"A saved answer."});
        assert!(answered(&question));
        assert!(!matches(&question, AnnotationFilter::Unanswered));
        question["answered"] = json!(false);
        assert!(!answered(&question));
        assert!(matches(&question, AnnotationFilter::Unanswered));
        assert_eq!(question["answer"], "A saved answer.");
        question["answered"] = json!(true);
        assert!(!matches(&question, AnnotationFilter::Unanswered));
        question["kind"] = json!("note");
        assert!(!matches(&question, AnnotationFilter::Unanswered));
    }

    #[test]
    fn note_timestamp_uses_local_creation_time_with_safe_legacy_fallback() {
        let created = "2026-10-14T07:36:00+08:00";
        let updated = "2026-10-15T11:22:00Z";
        let local = |value: &str| {
            chrono::DateTime::parse_from_rfc3339(value)
                .unwrap()
                .with_timezone(&chrono::Local)
                .format("%m/%d %H:%M")
                .to_string()
        };
        assert_eq!(
            timestamp(&json!({"created":created,"updated":updated})),
            Some(local(created))
        );
        assert_eq!(timestamp(&json!({"updated":updated})), Some(local(updated)));
        assert_eq!(
            timestamp(&json!({"created":"invalid","updated":updated})),
            Some(local(updated))
        );
        assert_eq!(timestamp(&json!({})), None);
        assert_eq!(timestamp(&json!({"created":"1","updated":"2"})), None);
    }

    #[test]
    fn pending_notes_remain_visible_until_saved_without_resurrecting_deletions() {
        let reader = json!({"notes":{
            "old":{"anchor":"b1","body":"old","updated":"2026-10-03T19:00:00+08:00"},
            "deleted":{"deleted":true,"updated":"2026-10-03T12:00:00Z"}}});
        let pending = HashMap::from([
            (
                "old".into(),
                json!({"anchor":"b1","body":"latest","updated":"2026-10-03T11:01:00Z"}),
            ),
            (
                "deleted".into(),
                json!({"body":"stale","updated":"2026-10-03T11:00:00Z"}),
            ),
            (
                "new".into(),
                json!({"anchor":"b2","body":"new","updated":"2026-10-03T11:00:00Z"}),
            ),
        ]);
        let notes = notes(&reader, &pending);
        assert_eq!(notes.len(), 2);
        assert_eq!(
            notes.iter().find(|note| note["id"] == "old").unwrap()["body"],
            "latest"
        );
        assert!(notes.iter().any(|note| note["anchor"] == "b2"));
    }
}
