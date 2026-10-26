//! Translation HTTP handlers.
use super::*;

#[route(POST "/read/{book_id}/translation")]
async fn save_translation(cx: &Cx, Form(input): Form<TranslationInput>) -> Result<SeeOther> {
    if input.text.len() > 64_000 || !matches!(input.action.as_str(), "save" | "revert") {
        return Err(bad_request("译文修订无效").into());
    }
    let expected_edit: Value =
        serde_json::from_str(&input.expected_edit).map_err(|_| bad_request("修订版本无效"))?;
    let id = path_param::<BookId>(cx);
    let book = library().workspace(id).map_err(|_| not_found())?;
    let raw = book.validated_state().map_err(Error::from_anyhow)?;
    let state = ReaderState::from_value(raw.clone()).map_err(Error::from_anyhow)?;
    let caption = input.block.ends_with("#caption");
    let block_id = input.block.strip_suffix("#caption").unwrap_or(&input.block);
    let Some(block) = state
        .paper
        .blocks
        .iter()
        .find(|b| b.id == block_id)
        .filter(|b| {
            if caption {
                matches!(b.kind.as_str(), "figure" | "table")
            } else {
                matches!(b.kind.as_str(), "para" | "heading" | "note")
            }
        })
    else {
        return Err(bad_request("不可修订该内容块").into());
    };
    let current = if caption {
        state.caption(block)
    } else {
        state.text(&block.id, &block.zh, &block.en)
    };
    let stored = raw["reader"]["edits"]
        .get(&input.block)
        .cloned()
        .unwrap_or(Value::Null);
    if easyread_gpui::model::text_hash(current) != input.expected_text || stored != expected_edit {
        return Err(bad_request("译文已改变，请刷新后重试").into());
    }
    let base = if caption {
        if block.caption_zh.is_empty() {
            &block.caption_en
        } else {
            &block.caption_zh
        }
    } else if block.zh.is_empty() {
        &block.en
    } else {
        &block.zh
    };
    // Reconstruct the server-owned native HTML, never accept a whole-page
    // buffer or client-supplied HTML. ParagraphEdit merges only this paragraph.
    let pdf = raw["paper"]["meta"]["format"] != "epub";
    let position = raw["paper"]["blocks"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|b| b["id"] == block_id)
        .and_then(|b| b[if pdf { "page" } else { "chapter" }].as_u64())
        .unwrap_or(1) as usize;
    let source_pdf = book.asset("source.pdf").ok();
    let rendered = display_blocks(&raw, &state, &[], position, pdf, source_pdf.as_deref(), id)
        .into_iter()
        .find(|b| b.id == block_id)
        .ok_or_else(|| bad_request("段落已不存在"))?;
    let plan = easyread_gpui::paragraph_edit::ParagraphEdit::new(
        current,
        &rendered.edit_html,
        input.paragraph,
    )
    .map_err(|_| bad_request("无法可靠定位该段译文，请重新选择正文段落"))?;
    let replacement = if input.action == "revert" {
        let mut baseline = raw.clone();
        if let Some(edits) = baseline["reader"]["edits"].as_object_mut() {
            edits.remove(&input.block);
        }
        let baseline_state =
            ReaderState::from_value(baseline.clone()).map_err(Error::from_anyhow)?;
        let baseline = display_blocks(
            &baseline,
            &baseline_state,
            &[],
            position,
            pdf,
            source_pdf.as_deref(),
            id,
        )
        .into_iter()
        .find(|b| b.id == block_id)
        .ok_or_else(|| bad_request("原译文已不存在"))?;
        if baseline.edit_paragraphs.len() != rendered.edit_paragraphs.len() {
            return Err(bad_request("段落结构已改变，无法安全还原该段").into());
        }
        easyread_gpui::paragraph_edit::ParagraphEdit::new(
            base,
            &baseline.edit_html,
            input.paragraph,
        )
        .map_err(|_| bad_request("无法安全还原该段"))?
        .text
    } else {
        input.text
    };
    let (markdown, html) = plan.merge(&replacement).map_err(Error::from_anyhow)?;
    let reset = input.action == "revert" && markdown == *base;
    let op = json!({"op":"edit","block":input.block,"zh":if reset { Value::Null } else { json!(markdown) },
        "native_layout":{"html":html,"source_hash":easyread_gpui::model::text_hash(if caption { &block.caption_en } else { &block.en }),
            "translation_hash":easyread_gpui::model::text_hash(&markdown)},
        "base":easyread_gpui::model::text_hash(base), "expected_text":input.expected_text,"expected_edit":expected_edit});
    book.reader_ops(&[op]).map_err(Error::from_anyhow)?;
    let mode = if matches!(
        input.mode.as_str(),
        "original" | "translation" | "bilingual"
    ) {
        input.mode
    } else {
        "bilingual".into()
    };
    Ok(see_other(format!(
        "/read/{id}?page={}&mode={mode}#block-{block_id}",
        input.page
    )))
}

#[route(POST "/read/{book_id}/progress")]
async fn save_progress(cx: &Cx, Form(input): Form<ProgressInput>) -> Result<SeeOther> {
    if !input.ratio.is_finite() || !(0.0..=1.0).contains(&input.ratio) {
        return Err(bad_request("进度无效").into());
    }
    let id = path_param::<BookId>(cx);
    let book = library().workspace(id).map_err(|_| not_found())?;
    let paper = book.load("paper").map_err(Error::from_anyhow)?;
    if !paper["blocks"]
        .as_array()
        .is_some_and(|blocks| blocks.iter().any(|block| block["id"] == input.block))
    {
        return Err(bad_request("进度段落无效").into());
    }
    book.reader_ops(&[json!({"op":"progress","block":input.block,"ratio":input.ratio})])
        .map_err(Error::from_anyhow)?;
    Ok(see_other(format!("/read/{id}")))
}
