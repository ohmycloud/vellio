use super::*;
use crate::store::{empty_reader, write_json};
use std::sync::Mutex;

struct Fake {
    seen: Mutex<Value>,
    fail: bool,
    cancel: bool,
    edit: Option<Workspace>,
}
impl Fake {
    fn new() -> Self {
        Self {
            seen: Mutex::new(Value::Null),
            fail: false,
            cancel: false,
            edit: None,
        }
    }
}
impl Transport for Fake {
    fn model(&self) -> &str {
        "explicit-test-model"
    }
    fn request(
        &self,
        messages: Value,
        cancelled: &AtomicBool,
        delta: &mut dyn FnMut(&str),
    ) -> Result<String> {
        *self.seen.lock().unwrap() = messages;
        if let Some(workspace) = &self.edit {
            workspace.update("reader", |reader| {
                reader["notes"]["question"]["body"] = json!("Edited while streaming");
                Ok(())
            })?;
        }
        delta("A streamed answer.");
        if self.cancel {
            cancelled.store(true, Ordering::Relaxed);
        }
        if self.fail || self.cancel {
            bail!("explicit test failure");
        }
        Ok("A streamed answer.".into())
    }
}
fn fixture() -> (tempfile::TempDir, Workspace) {
    let temp = tempfile::tempdir().unwrap();
    let workspace = Workspace {
        root: temp.path().to_owned(),
    };
    let blocks = (0..10)
        .map(|i| {
            json!({"id":format!("b{i}"),"type":"para","page":i+1,
        "en":format!("Original paragraph {i}"),"zh":format!("译文 {i}")})
        })
        .collect::<Vec<_>>();
    write_json(
        &workspace.root.join("paper.json"),
        &json!({"meta":{"title_en":"Native Ask AI regression"},"blocks":blocks}),
    )
    .unwrap();
    let mut reader = empty_reader();
    reader["notes"] = json!({
        "question":{"id":"question","anchor":"b1","kind":"question","body":"Why?","quote":"Original paragraph 1",
            "color":"pink","answered":false,"created":"2020-01-01T00:00:00Z","updated":"2020-01-01T00:00:00Z","future":{"keep":true}},
        "private":{"id":"private","anchor":"b9","kind":"note","body":"PRIVATE_NOTE_CONTENT","color":"blue"}
    });
    write_json(&workspace.root.join("reader.json"), &reader).unwrap();
    (temp, workspace)
}
fn request() -> ChatRequest {
    ChatRequest {
        text: "Why does this work?".into(),
        refs: vec![
            ChatReference {
                anchor: "b1".into(),
                quote: "Original paragraph 1".into(),
            },
            ChatReference {
                anchor: "b6".into(),
                quote: "Original paragraph 6".into(),
            },
        ],
        note: Some("question".into()),
        model_id: "profile-id".into(),
        ..Default::default()
    }
}
#[test]
fn direct_note_ask_streams_persists_refs_and_attaches_one_reply_without_losing_edits() {
    let (_temp, ws) = fixture();
    let paper = ws.load("paper").unwrap();
    let original = ws.load("reader").unwrap()["notes"]["question"].clone();
    let mut model = Fake::new();
    model.edit = Some(ws.clone());
    let mut streamed = String::new();
    ask_with_context(
        &ws,
        "thread",
        &request(),
        &model,
        &AtomicBool::new(false),
        |text| streamed.push_str(text),
    )
    .unwrap();
    assert_eq!(streamed, "A streamed answer.");
    let prompt = model.seen.lock().unwrap()[0]["content"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(prompt.contains("Original paragraph 1"));
    assert!(prompt.contains("Original paragraph 6"));
    assert!(!prompt.contains("Original paragraph 9"));
    assert!(!prompt.contains("PRIVATE_NOTE_CONTENT"));
    let chat = ws.load("chat").unwrap();
    let thread = &chat["threads"][0];
    assert_eq!(thread["messages"][0]["refs"].as_array().unwrap().len(), 2);
    assert_eq!(thread["messages"][0]["note"], "question");
    assert_eq!(thread["model"], "profile-id");
    assert_eq!(thread["messages"][1]["status"], "done");
    let note = ws.load("reader").unwrap()["notes"]["question"].clone();
    assert_eq!(note["body"], "Edited while streaming");
    assert_eq!(note["answered"], true);
    for field in ["anchor", "quote", "created", "future"] {
        assert_eq!(note[field], original[field]);
    }
    let discussion = ws.load("discussion").unwrap();
    let id = discussion["entries"][0]["id"].clone();
    assert_eq!(discussion["entries"][0]["reply_to"], "question");
    assert_eq!(discussion["entries"][0]["body"], streamed);
    ask_with_context(
        &ws,
        "thread",
        &request(),
        &model,
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    let discussion = ws.load("discussion").unwrap();
    assert_eq!(discussion["entries"].as_array().unwrap().len(), 1);
    assert_eq!(discussion["entries"][0]["id"], id);
    assert_eq!(ws.load("paper").unwrap(), paper);
}
#[test]
fn failed_or_cancelled_answers_preserve_partial_chat_but_do_not_answer_the_question() {
    for cancel in [false, true] {
        let (_temp, ws) = fixture();
        let reader = ws.load("reader").unwrap();
        let mut model = Fake::new();
        model.fail = !cancel;
        model.cancel = cancel;
        assert!(
            ask_with_context(
                &ws,
                "thread",
                &request(),
                &model,
                &AtomicBool::new(false),
                |_| {}
            )
            .is_err()
        );
        assert_eq!(ws.load("reader").unwrap(), reader);
        assert!(
            ws.load("discussion").unwrap()["entries"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        let answer = ws.load("chat").unwrap()["threads"][0]["messages"][1].clone();
        assert_eq!(answer["content"], "A streamed answer.");
        assert_eq!(answer["status"], if cancel { "cancelled" } else { "error" });
    }
}
#[test]
fn contexts_use_local_references_color_filtered_notes_and_whole_paper_only_when_requested() {
    let (_temp, ws) = fixture();
    let state = ws.state().unwrap();
    let mut req = request();
    let local = chat_context(&state, &req).unwrap();
    assert!(!local.contains("PRIVATE_NOTE_CONTENT"));
    req.text = "Explain my red highlights".into();
    let colored = chat_context(&state, &req).unwrap();
    assert!(colored.contains("Why?"));
    assert!(!colored.contains("PRIVATE_NOTE_CONTENT"));
    req.answer_style = AnswerStyle::CloseReading;
    let whole = chat_context(&state, &req).unwrap();
    assert!(whole.contains("Original paragraph 9"));
    assert!(whole.contains("未提供或省略"));
}
#[test]
fn answer_style_explanations_match_context_budget_without_changing_saved_ids() {
    for (style, id) in [
        (AnswerStyle::Standard, "standard"),
        (AnswerStyle::Simple, "ste100"),
        (AnswerStyle::CloseReading, "xray"),
    ] {
        assert_eq!(serde_json::to_value(style).unwrap(), id);
        assert!(style.description().starts_with(style.label()));
        if style != AnswerStyle::Standard {
            assert!(style.description().contains("用量更多"));
            assert!(style.description().contains("节选"));
        }
    }
}

#[test]
fn conversation_effort_options_preserve_unknown_fields_and_can_return_to_model_defaults() {
    let (_temp, ws) = fixture();
    ws.update("chat", |chat| {
        *chat=json!({"threads":[{"id":"effort","messages":[],"chat_options":{"future_option":7,"reasoning_effort":"low"}}]});
        Ok(())
    }).unwrap();
    let mut request = request();
    request.note = None;
    request.reasoning_effort = Some("max".into());
    ask_with_context(
        &ws,
        "effort",
        &request,
        &Fake::new(),
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    assert_eq!(
        ws.load("chat").unwrap()["threads"][0]["chat_options"],
        json!({"future_option":7,"reasoning_effort":"max"})
    );
    request.reasoning_effort = None;
    ask_with_context(
        &ws,
        "effort",
        &request,
        &Fake::new(),
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    assert_eq!(
        ws.load("chat").unwrap()["threads"][0]["chat_options"],
        json!({"future_option":7})
    );
    request.reasoning_effort = Some(String::new());
    ask_with_context(
        &ws,
        "effort",
        &request,
        &Fake::new(),
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    assert_eq!(
        ws.load("chat").unwrap()["threads"][0]["chat_options"],
        json!({"future_option":7,"reasoning_effort":""})
    );
}

#[test]
fn legacy_chat_normalization_keeps_messages_and_unknown_fields_and_sorts_recent_first() {
    let original = json!({"custom":42,"messages":[{"role":"user","content":"Legacy question","at":"2025-01-01T00:00:00Z"},
        {"role":"assistant","content":"Legacy answer","at":"2025-01-02T00:00:00Z"}],
        "threads":[{"id":"newer","updated":"2025-02-01T00:00:00Z","messages":[]}]});
    let normalized = normalize_chat_state(original.clone()).unwrap();
    assert_eq!(normalized["threads"][0]["id"], "newer");
    assert_eq!(normalized["threads"][1]["id"], "t-first");
    assert_eq!(normalized["threads"][1]["messages"], original["messages"]);
    assert_eq!(normalized["threads"][1]["title"], "Legacy question");
    assert_eq!(normalized["custom"], 42);
    assert_eq!(
        AnswerStyle::from_value(&json!("unknown")),
        AnswerStyle::Standard
    );
}

#[test]
fn thread_operations_preserve_unknown_fields_and_pinned_discussions_survive_deletion() {
    let (_temp, ws) = fixture();
    let model = Fake::new();
    ask_with_context(
        &ws,
        "thread",
        &request(),
        &model,
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    ws.update("chat", |chat| {
        chat["custom"] = json!({"keep":true});
        chat["threads"][0]["future"] = json!(42);
        Ok(())
    })
    .unwrap();
    rename_chat_thread(&ws, "thread", "New title").unwrap();
    let chat = ws.load("chat").unwrap();
    assert_eq!(chat["threads"][0]["title"], "New title");
    assert_eq!(chat["threads"][0]["future"], 42);
    let mid = chat["threads"][0]["messages"][1]["id"].as_str().unwrap();
    pin_chat_message(&ws, "thread", mid).unwrap();
    pin_chat_message(&ws, "thread", mid).unwrap();
    let discussion = ws.load("discussion").unwrap();
    assert_eq!(discussion["entries"].as_array().unwrap().len(), 2);
    delete_chat_thread(&ws, "thread").unwrap();
    assert!(
        ws.load("chat").unwrap()["threads"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(ws.load("chat").unwrap()["custom"]["keep"], true);
    assert_eq!(ws.load("discussion").unwrap(), discussion);
}
