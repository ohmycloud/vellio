//! Notes HTTP handlers.
use super::*;

#[route(POST "/read/{book_id}/notes")]
async fn save_note(cx: &Cx, Form(input): Form<NoteInput>) -> Result<SeeOther> {
    let book_id = path_param::<BookId>(cx);
    let book = library().workspace(book_id).map_err(|_| not_found())?;
    if !matches!(input.kind.as_str(), "note" | "question" | "highlight")
        || !matches!(input.color.as_str(), "yellow" | "green" | "blue" | "pink")
        || !matches!(input.style.as_str(), "highlight" | "underline")
        || input.body.len() > 16_000
        || input.quote.len() > 4_000
        || input.prefix.len() > 128
        || input.suffix.len() > 128
        || !matches!(input.side.as_str(), "en" | "zh")
    {
        return Err(bad_request("笔记内容无效").into());
    }
    let raw = book.load("paper").map_err(Error::from_anyhow)?;
    if !raw["blocks"]
        .as_array()
        .is_some_and(|blocks| blocks.iter().any(|b| b["id"] == input.anchor))
    {
        return Err(bad_request("笔记段落不存在").into());
    }
    let note = json!({"id": uuid::Uuid::new_v4().to_string(), "anchor": input.anchor, "side": input.side, "quote":input.quote,
        "prefix":input.prefix, "suffix":input.suffix, "kind":input.kind, "body":input.body, "color":input.color, "style":input.style, "created":easyread_gpui::store::now(), "updated":easyread_gpui::store::now()});
    book.reader_ops(&[json!({"op":"note","note":note})])
        .map_err(Error::from_anyhow)?;
    let mode = if matches!(
        input.mode.as_str(),
        "original" | "translation" | "bilingual"
    ) {
        input.mode
    } else {
        "bilingual".into()
    };
    Ok(see_other(format!(
        "/read/{book_id}?page={}&mode={mode}",
        input.page
    )))
}

/// The margin editor owns a stable ID before its first save. Replaying a
/// committed request (or closing while a save is pending) must not add a note.
#[route(POST "/read/{book_id}/notes/margin")]
async fn save_margin_note(cx: &Cx, Form(input): Form<MarginNoteInput>) -> Result<SeeOther> {
    if !matches!(input.kind.as_str(), "note" | "question")
        || !matches!(input.color.as_str(), "yellow" | "green" | "blue" | "pink")
        || !matches!(input.answered.as_str(), "yes" | "no")
        || !matches!(input.side.as_str(), "en" | "zh")
        || input.body.len() > 16_000
        || input.quote.len() > 4_000
        || input.prefix.len() > 128
        || input.suffix.len() > 128
        || uuid::Uuid::parse_str(&input.session_id).is_err()
    {
        return Err(bad_request("页边笔记内容无效").into());
    }
    // Serialize the revision check and write, including the final pagehide
    // beacon: an older in-flight autosave cannot overwrite the latest draft.
    static WRITES: OnceLock<Mutex<()>> = OnceLock::new();
    let _guard = WRITES
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    let id = path_param::<BookId>(cx);
    let book = library().workspace(id).map_err(|_| not_found())?;
    let raw = book.validated_state().map_err(Error::from_anyhow)?;
    if !raw["paper"]["blocks"]
        .as_array()
        .is_some_and(|blocks| blocks.iter().any(|b| b["id"] == input.anchor))
    {
        return Err(bad_request("笔记段落不存在").into());
    }
    let stored = &raw["reader"]["notes"][&input.note_id];
    if stored.is_null() {
        if uuid::Uuid::parse_str(&input.note_id).is_err() || input.body.trim().is_empty() {
            return Err(bad_request("新笔记不能为空").into());
        }
    } else if stored["deleted"] == true
        || stored["anchor"] != input.anchor
        || stored["side"].as_str().unwrap_or("zh") != input.side
        || !matches!(stored["kind"].as_str(), Some("note" | "question"))
    {
        return Err(bad_request("该笔记已删除或锚点不匹配").into());
    }
    let mode = if matches!(
        input.mode.as_str(),
        "original" | "translation" | "bilingual"
    ) {
        &input.mode
    } else {
        "bilingual"
    };
    let destination = format!("/read/{id}?page={}&mode={mode}", input.page);
    if stored["web_margin_version"]["session"] == input.session_id
        && stored["web_margin_version"]["revision"]
            .as_u64()
            .is_some_and(|revision| revision >= input.revision)
    {
        return Ok(see_other(destination));
    }
    let stamp = easyread_gpui::store::now();
    let mut note = if stored.is_null() {
        json!({"id":input.note_id,"anchor":input.anchor,"side":input.side,"quote":input.quote,
            "prefix":input.prefix,"suffix":input.suffix,"created":stamp,"style":"underline"})
    } else {
        stored.clone()
    };
    note["kind"] = json!(input.kind);
    note["body"] = json!(input.body);
    note["color"] = json!(input.color);
    note["answered"] = json!(input.answered == "yes");
    note["updated"] = json!(stamp);
    note["web_margin_version"] = json!({"session":input.session_id,"revision":input.revision});
    let mut source_update = None;
    if !input.source_annotation.is_empty() {
        let source = &raw["reader"]["notes"][&input.source_annotation];
        if source["deleted"] != true {
            if source["kind"] != "highlight"
                || source["anchor"] != input.anchor
                || source["side"].as_str().unwrap_or("zh") != input.side
            {
                return Err(bad_request("关联标记无效").into());
            }
            if source["color"] != input.color {
                let mut source = source.clone();
                source["color"] = json!(input.color);
                source["updated"] = json!(stamp);
                source_update = Some(json!({"op":"note","note":source}));
            }
        }
        // Attach writing to the mark without turning that independent mark
        // into a question or adding a second visible decoration over it.
        note["source_annotation"] = json!(input.source_annotation);
        note["decoration"] = json!("none");
        note.as_object_mut().unwrap().remove("style");
    }
    let mut ops = vec![json!({"op":"note","note":note})];
    if let Some(source) = source_update {
        ops.push(source);
    }
    book.reader_ops(&ops).map_err(Error::from_anyhow)?;
    Ok(see_other(destination))
}

#[route(POST "/notes/preview")]
async fn preview_note(Form(input): Form<NotePreviewInput>) -> Result<Html<String>> {
    if input.body.len() > 16_000 {
        return Err(bad_request("笔记过长").into());
    }
    Ok(Html(note_markdown::render(&input.body)))
}

#[route(POST "/read/{book_id}/notes/edit")]
async fn edit_note(cx: &Cx, Form(input): Form<NoteEdit>) -> Result<SeeOther> {
    if !matches!(input.kind.as_str(), "note" | "question" | "highlight")
        || !matches!(input.color.as_str(), "yellow" | "green" | "blue" | "pink")
        || !matches!(input.style.as_str(), "highlight" | "underline")
        || !matches!(input.answered.as_str(), "yes" | "no")
        || input.body.len() > 16_000
    {
        return Err(bad_request("笔记修改无效").into());
    }
    let id = path_param::<BookId>(cx);
    let book = library().workspace(id).map_err(|_| not_found())?;
    let current = book.load("reader").map_err(Error::from_anyhow)?;
    let Some(mut note) = current["notes"]
        .get(&input.note_id)
        .cloned()
        .filter(|n| !n["deleted"].as_bool().unwrap_or(false))
    else {
        return Err(not_found().into());
    };
    note["kind"] = json!(input.kind);
    note["body"] = json!(input.body);
    note["color"] = json!(input.color);
    note["style"] = json!(input.style);
    note["answered"] = json!(input.answered == "yes");
    note["updated"] = json!(easyread_gpui::store::now());
    book.reader_ops(&[json!({"op":"note","note":note})])
        .map_err(Error::from_anyhow)?;
    let mode = if matches!(
        input.mode.as_str(),
        "original" | "translation" | "bilingual"
    ) {
        input.mode
    } else {
        "bilingual".into()
    };
    Ok(see_other(format!(
        "/read/{id}?page={}&mode={mode}",
        input.page
    )))
}

#[route(POST "/read/{book_id}/notes/action")]
async fn note_action(cx: &Cx, Form(input): Form<NoteAction>) -> Result<SeeOther> {
    if input.action != "kind" {
        return Err(bad_request("笔记操作无效").into());
    }
    let id = path_param::<BookId>(cx);
    let book = library().workspace(id).map_err(|_| not_found())?;
    let reader = book.load("reader").map_err(Error::from_anyhow)?;
    let Some(mut note) = reader["notes"].get(&input.note_id).cloned().filter(|note| {
        note["deleted"] != true && matches!(note["kind"].as_str(), Some("note" | "question"))
    }) else {
        return Err(bad_request("笔记已删除或不能转换类型").into());
    };
    note["kind"] = json!(if note["kind"] == "question" {
        "note"
    } else {
        "question"
    });
    // Only change kind: retain body, answer/status, color, decoration and links.
    note["updated"] = json!(easyread_gpui::store::now());
    book.reader_ops(&[json!({"op":"note","note":note})])
        .map_err(Error::from_anyhow)?;
    let mode = if matches!(
        input.mode.as_str(),
        "original" | "translation" | "bilingual"
    ) {
        &input.mode
    } else {
        "bilingual"
    };
    Ok(see_other(format!(
        "/read/{id}?page={}&mode={mode}",
        input.page
    )))
}

#[route(POST "/read/{book_id}/paper-note")]
async fn save_paper_note(cx: &Cx, Form(input): Form<PaperNoteInput>) -> Result<SeeOther> {
    if input.body.len() > 32_000 {
        return Err(bad_request("全文笔记过长").into());
    }
    let id = path_param::<BookId>(cx);
    let book = library().workspace(id).map_err(|_| not_found())?;
    book.reader_ops(&[json!({"op":"paper_note","body":input.body})])
        .map_err(Error::from_anyhow)?;
    let mode = if matches!(
        input.mode.as_str(),
        "original" | "translation" | "bilingual"
    ) {
        input.mode
    } else {
        "bilingual".into()
    };
    Ok(see_other(format!(
        "/read/{id}?page={}&mode={mode}",
        input.page
    )))
}

#[derive(Deserialize)]
struct DeleteNote {
    note_id: String,
    page: usize,
    mode: String,
}
#[route(POST "/read/{book_id}/notes/delete")]
async fn delete_note(cx: &Cx, Form(input): Form<DeleteNote>) -> Result<SeeOther> {
    let id = path_param::<BookId>(cx);
    let book = library().workspace(id).map_err(|_| not_found())?;
    // Refuse blind deletion of arbitrary note IDs.
    let current = book.load("reader").map_err(Error::from_anyhow)?;
    if current["notes"].get(&input.note_id).is_none() {
        return Err(not_found().into());
    }
    book.reader_ops(&[json!({"op":"note_del","id":input.note_id})])
        .map_err(Error::from_anyhow)?;
    let mode = if matches!(
        input.mode.as_str(),
        "original" | "translation" | "bilingual"
    ) {
        input.mode
    } else {
        "bilingual".into()
    };
    Ok(see_other(format!(
        "/read/{id}?page={}&mode={mode}",
        input.page
    )))
}
