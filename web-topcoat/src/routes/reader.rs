//! Reader HTTP handlers.
use super::*;

#[page("/read/{book_id}")]
async fn read(cx: &Cx) -> Result<impl View> {
    let id = path_param::<BookId>(cx).to_owned();
    let query = query_params::<ReadQuery>(cx)?;
    let mode = match query.mode.as_deref() {
        Some("original") => "original",
        Some("translation") => "translation",
        _ => "bilingual",
    }
    .to_owned();
    let book = library().workspace(&id).map_err(|_| not_found())?;
    let raw = tokio::task::spawn_blocking({
        let book = book.clone();
        move || book.validated_state()
    })
    .await?
    .map_err(Error::from_anyhow)?;
    let state = ReaderState::from_value(raw.clone()).map_err(Error::from_anyhow)?;
    let pdf = raw["paper"]["meta"]["format"] != "epub";
    let wanted = query.page.unwrap_or_else(|| {
        let saved = raw["reader"]["progress"]["block"].as_str().unwrap_or("");
        raw["paper"]["blocks"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|b| b["id"] == saved)
            .and_then(|b| b[if pdf { "page" } else { "chapter" }].as_u64())
            .unwrap_or(1) as usize
    });
    let total = if pdf {
        raw["paper"]["meta"]["page_count"].as_u64().unwrap_or(1)
    } else {
        raw["paper"]["meta"]["chapter_count"].as_u64().unwrap_or(1)
    }
    .max(1) as usize;
    let number = wanted.clamp(1, total);
    let title = state.title().to_owned();
    let model_prefs = model_preferences();
    let ask_available = model_prefs
        .as_ref()
        .is_ok_and(|prefs| prefs.features.ask_ai && prefs.connection(false).is_ok());
    let web_appearance = model_prefs
        .as_ref()
        .map(|prefs| prefs.appearance.clone())
        .unwrap_or_default();
    let web_font = match web_appearance.reading_font() {
        easyread_gpui::session::ReadingFont::NotoSerifSc => "noto",
        easyread_gpui::session::ReadingFont::SystemSerif => "system",
        easyread_gpui::session::ReadingFont::SansSerif => "sans",
    };
    let translating = translations()
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .contains_key(&id.to_string());
    let job_state = raw["job"]["state"].as_str().unwrap_or("idle").to_owned();
    let job_progress = format!(
        "翻译 {}/{}",
        raw["job"]["done"].as_u64().unwrap_or(0),
        raw["job"]["total"].as_u64().unwrap_or(0)
    );
    let chat_messages: Vec<ChatMessage> = ai::normalize_chat_state(raw["chat"].clone())
        .ok()
        .and_then(|chat| {
            chat["threads"]
                .as_array()?
                .iter()
                .find(|thread| thread["id"] == "web-reader")
                .cloned()
        })
        .and_then(|thread| thread["messages"].as_array().cloned())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|message| {
            let role = message["role"].as_str()?.to_owned();
            if role != "assistant" && role != "user" {
                return None;
            }
            let failed = message["status"] == "error";
            let body = message["content"].as_str().unwrap_or("");
            Some(ChatMessage {
                role,
                body_html: if failed && body.trim().is_empty() {
                    "<p>模型请求失败，请检查本机服务日志和模型配置后重试。</p>".to_owned()
                } else {
                    note_markdown::render(body)
                },
                failed,
            })
        })
        .collect();
    let notes: Vec<_> = raw["reader"]["notes"]
        .as_object()
        .into_iter()
        .flat_map(|o| o.values())
        .filter(|n| !n["deleted"].as_bool().unwrap_or(false))
        .cloned()
        .collect();
    let source_pdf = if pdf {
        book.asset("source.pdf").ok()
    } else {
        None
    };
    let (blocks, book_contents) = tokio::task::spawn_blocking({
        let raw = raw.clone();
        let state = state.clone();
        let notes = notes.clone();
        let book_id = id.to_string();
        move || {
            let blocks = display_blocks(
                &raw,
                &state,
                &notes,
                number,
                pdf,
                source_pdf.as_deref(),
                &book_id,
            );
            let contents = table_of_contents::load(&raw, &state, source_pdf.as_deref(), total);
            (blocks, contents)
        }
    })
    .await?;
    let paper_blocks = raw["paper"]["blocks"].as_array();
    let mut all_notes: Vec<_> = notes
        .iter()
        .filter_map(|n| {
            let anchor = n["anchor"].as_str()?;
            let index = paper_blocks?
                .iter()
                .position(|block| block["id"] == anchor)?;
            let page = paper_blocks?[index][if pdf { "page" } else { "chapter" }]
                .as_u64()
                .unwrap_or(1) as usize;
            Some((
                index,
                RenderNote {
                    id: n["id"].as_str().unwrap_or("").into(),
                    anchor: anchor.into(),
                    side: n["side"].as_str().unwrap_or("zh").into(),
                    prefix: n["prefix"].as_str().unwrap_or("").into(),
                    suffix: n["suffix"].as_str().unwrap_or("").into(),
                    source_annotation: n["source_annotation"].as_str().unwrap_or("").into(),
                    kind: n["kind"].as_str().unwrap_or("note").into(),
                    quote: n["quote"].as_str().unwrap_or("").into(),
                    body: n["body"].as_str().unwrap_or("").into(),
                    body_html: note_markdown::render(n["body"].as_str().unwrap_or("")),
                    created: n["created"].as_str().unwrap_or("").into(),
                    color: n["color"].as_str().unwrap_or("yellow").into(),
                    style: n["style"].as_str().unwrap_or("highlight").into(),
                    answered: n["answered"].as_bool().unwrap_or(false),
                    replies: raw["discussion"]["entries"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter(|entry| {
                            entry["reply_to"] == n["id"]
                                && entry["kind"] == "reply"
                                && entry["live"] == true
                                && entry["deleted"] != true
                        })
                        .map(|entry| NoteReply {
                            by: entry["by"].as_str().unwrap_or("AI").to_owned(),
                            body_html: note_markdown::render(entry["body"].as_str().unwrap_or("")),
                        })
                        .collect(),
                    page,
                },
            ))
        })
        .collect();
    all_notes.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.created.cmp(&b.1.created)));
    let visible_notes: Vec<_> = all_notes
        .iter()
        .filter(|(_, n)| blocks.iter().any(|b| b.id == n.anchor))
        .map(|(_, n)| n.clone())
        .collect();
    let drawer_notes: Vec<_> = all_notes.into_iter().map(|(_, n)| n).collect();
    let paper_note = raw["reader"]["paper_note"]["body"]
        .as_str()
        .unwrap_or("")
        .to_owned();
    let position = format!("{} {} / {}", if pdf { "第" } else { "章节" }, number, total);
    let prev = format!(
        "/read/{id}?page={}&mode={mode}",
        number.saturating_sub(1).max(1)
    );
    let next = format!("/read/{id}?page={}&mode={mode}", (number + 1).min(total));
    let notes_action = format!("/read/{id}/notes");
    let pdf_page = format!("/read/{id}/pages/page-{number:03}.webp");
    let mode_class = format!("paper mode-{mode} original-first");
    let contents_html = table_of_contents::render(&book_contents, &id, &mode, number);
    Ok(view! {
        <!DOCTYPE html>
        <html lang="zh-CN"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
            <title>(title.clone())" · Vellio"</title><link rel="stylesheet" href="/reader.css"><link rel="stylesheet" href="/popover.css"><link rel="stylesheet" href="/margin-editor.css"><script src="/reader-updates.js" defer="defer"></script><script src="/margin-editor.js" defer="defer"></script><script src="/reader.js" defer="defer"></script>
        </head><body>
            <nav class="top" aria-label="阅读工具栏">
                <a class="library-button icon-button" href="/" title="书库" aria-label="书库">"▤"</a>
                let contents_id = element_id(cx);
                <div class="contents-menu"><button type="button" popovertarget=(&contents_id) class="icon-button" title="目录" aria-label="目录">"☷"</button>
                    <div id=(&contents_id) popover="auto" class="contents-popover vellio-popover" aria-label="书籍目录"><strong>"目录"</strong>
                        if contents_html.is_empty() {
                            <p class="contents-empty">"原书未提供可用目录。"</p>
                        } else {
                            if book_contents.inferred { <p class="contents-hint">"原书未提供导航目录，以下按正文标题显示。"</p> }
                            <nav class="contents-sections" aria-label="书籍章节">(Unescaped::new_unchecked(contents_html))</nav>
                        }
                    </div>
                </div>
                <span class="book-title" title=(title.clone())>(title)</span><span class="location-label">(position)</span><span class="spacer"></span>
                <div class="page-nav"><a class="icon-button" href=(prev) aria-label="上一页或章节" aria-disabled=(number <= 1)>"‹"</a><a class="icon-button" href=(next) aria-label="下一页或章节" aria-disabled=(number >= total)>"›"</a></div>
                <div class="mode-control" role="group" aria-label="阅读模式">
                    for (name, label) in [("original", "原文"), ("translation", "译文"), ("bilingual", "双语")] {
                        <a class=(if mode == name { "selected" } else { "" }) aria-current=(if mode == name { "page" } else { "false" }) href=(format!("/read/{id}?page={number}&mode={name}"))>(label)</a>
                    }
                    let order_id = element_id(cx);
                    <div class="order-menu"><button type="button" popovertarget=(&order_id) aria-label="双语显示顺序" title="双语显示顺序">"⌄"</button><div id=(&order_id) popover="auto" class="order-options vellio-popover" data-align="end" aria-label="双语显示顺序"><button type="button" data-reading-order="source" aria-pressed="true" popovertarget=(&order_id) popovertargetaction="hide">"原文在前"</button><button type="button" data-reading-order="translation" aria-pressed="false" popovertarget=(&order_id) popovertargetaction="hide">"译文在前"</button></div></div>
                </div>
                if pdf { <button type="button" class="tool-button" id="source-toggle" aria-expanded="false" aria-controls="source-panel">"▣ PDF"</button> }
                <button type="button" class="tool-button" id="notes-toggle" aria-expanded="false" aria-controls="notes-drawer">"▤ Notes"</button>
                if ask_available { <button class="tool-button" type="button" id="chat-toggle" aria-expanded="false" aria-controls="chat-panel">"✳ Ask AI"</button> }
                if translating { <form class="translate-action" method="post" action=(format!("/read/{id}/translate/cancel"))><input type="hidden" name="page" value=(number)><input type="hidden" name="mode" value=(mode.clone())><button class="tool-button">"取消翻译"</button></form> }
                <span id="job-status" class="job-status" data-active=(translating.to_string()) data-state=(job_state.clone())>(if translating { job_progress } else if job_state == "partial" { "翻译中断，可重试".to_owned() } else { String::new() })</span>
                reader_controls::appearance_menu()
            </nav>
            <div class="reading-progress" role="progressbar" aria-label="阅读进度" aria-valuemin="0" aria-valuemax="100" aria-valuenow="0"><span></span></div>
            <div class="reader-body" data-book-id=(id.to_string()) data-page=(number.to_string()) data-total=(total.to_string()) data-default-font=(web_font) data-default-size=(web_appearance.font_size.to_string()) data-default-width=(web_appearance.line_width.to_string()) data-default-leading=((web_appearance.line_spacing*10.).round().to_string()) data-default-margin=(web_appearance.margin_notes.to_string())><div class="stage"><div class="left-space"></div><article class=(mode_class)>
                for b in blocks {
                    <section class=(format!("blk blk-{}{}{}", b.kind, if b.untranslated { " untranslated" } else { "" }, if b.paired.is_empty() { "" }else{ " paired" })) id=(format!("block-{}", b.id)) data-id=(b.id) data-page-block=(b.page_block.to_string())>
                        if !b.src.is_empty() { <img class="book-illustration" src=(b.src) alt=(b.alt)> }
                        <div class="translated">(Unescaped::new_unchecked(b.translated))</div>
                        <div class="source" lang="en">(Unescaped::new_unchecked(b.original))</div>
                        if b.page_block && !b.untranslated && b.paired.is_empty() { <small class="alignment-warning">"本页原文与译文结构不匹配，无法可靠地逐段对应；保留整页对照。"</small> }
                        if !b.paired.is_empty() { <div class="paired-flow" hidden=(true)>(Unescaped::new_unchecked(b.paired))</div> }
                        if matches!(b.kind.as_str(), "para" | "heading" | "note" | "figure" | "table") {
                            for paragraph in b.edit_paragraphs {
                                <form class="translation-editor" data-paragraph=(paragraph.index.to_string()) hidden=(true) method="post" action=(format!("/read/{id}/translation"))>
                                    <input type="hidden" name="block" value=(b.edit_key.clone())><input type="hidden" name="paragraph" value=(paragraph.index.to_string())><input type="hidden" name="expected_text" value=(b.edit_hash.clone())><input type="hidden" name="expected_edit" value=(b.edit_previous.clone())>
                                    <input type="hidden" name="page" value=(number)><input type="hidden" name="mode" value=(mode.clone())>
                                    <label>"修订该段译文"<textarea name="text" maxlength="64000" spellcheck="false">(paragraph.text)</textarea></label>
                                    <div class="translation-actions"><button type="button" data-translation-cancel="true">"取消"</button><button type="submit" name="action" value="revert" disabled=(!b.has_edit)>"还原该段"</button><button type="submit" name="action" value="save" class="save">"保存译文"</button></div>
                                </form>
                            }
                        }
                    </section>
                }
            </article><div class="source-gutter"></div><aside class="margin" aria-label="页边笔记">
                for n in visible_notes.clone() {
                    <div class=(format!("note-card {}", n.kind)) data-note=(n.id.clone()) data-anchor=(n.anchor) data-side=(n.side) data-prefix=(n.prefix) data-suffix=(n.suffix) data-created=(n.created.clone()) data-source-annotation=(n.source_annotation)>
                        <div class="tag">(if n.kind == "question" { "问题" } else if n.kind == "highlight" { "划线" } else { "笔记" })</div>
                        if !n.quote.is_empty() { <div class="quote">(n.quote)</div> }
                        <div class="body">(Unescaped::new_unchecked(n.body_html.clone()))</div>
                        for reply in n.replies.clone() { <div class="ai-reply"><small>(reply.by)</small><div class="body">(Unescaped::new_unchecked(reply.body_html))</div></div> }
                        <div class="note-ai-slot">if matches!(n.kind.as_str(), "note" | "question") && !n.body.trim().is_empty() {
                            <button class="note-ai-action" type="button" data-ask-note=(n.id.clone()) disabled=(!ask_available) title=(if ask_available { "基于引文与笔记直接提问" } else { "请先在模型设置中启用并配置 Ask AI" })>(if n.kind == "question" { "✳ Ask AI to answer" } else { "✳ Ask AI for feedback" })</button>
                        }</div>
                        <details class="note-edit" hidden=(true)><summary>"笔记数据"</summary><form method="post" action=(format!("/read/{id}/notes/edit"))>
                            <input type="hidden" name="note_id" value=(n.id.clone())><input type="hidden" name="page" value=(number)><input type="hidden" name="mode" value=(mode.clone())>
                            <label>"类型"<select name="kind"><option value="note" selected=(n.kind == "note")>"笔记"</option><option value="question" selected=(n.kind == "question")>"问题"</option><option value="highlight" selected=(n.kind == "highlight")>"划线"</option></select></label>
                            <label>"标记"<select name="style"><option value="highlight" selected=(n.style != "underline")>"高亮"</option><option value="underline" selected=(n.style == "underline")>"下划线"</option></select></label>
                            <label>"颜色"<select name="color"><option value="yellow" selected=(n.color == "yellow")>"黄"</option><option value="green" selected=(n.color == "green")>"绿"</option><option value="blue" selected=(n.color == "blue")>"蓝"</option><option value="pink" selected=(n.color == "pink")>"红"</option></select></label>
                            <label>"问题状态"<select name="answered"><option value="no" selected=(!n.answered)>"未解答"</option><option value="yes" selected=(n.answered)>"已解答"</option></select></label>
                            <label class="full">"内容"<textarea name="body" maxlength="16000">(n.body.clone())</textarea></label><button type="submit">"保存修改"</button><span class="autosave-status" aria-live="polite"></span>
                        </form></details>
                        <div class="card-actions" aria-label="笔记操作">
                            <button type="button" data-edit-note=(n.id.clone()) aria-label="编辑笔记">"Edit"</button>
                            if matches!(n.kind.as_str(), "note" | "question") { <form method="post" action=(format!("/read/{id}/notes/action"))><input type="hidden" name="note_id" value=(n.id.clone())><input type="hidden" name="page" value=(number)><input type="hidden" name="mode" value=(mode.clone())><button type="submit" name="action" value="kind" data-toggle-note=(n.id.clone())>(if n.kind == "question" { "Make it a note" } else { "Make it a question" })</button></form> }
                            <form method="post" action=(format!("/read/{id}/notes/delete"))><input type="hidden" name="note_id" value=(n.id.clone())><input type="hidden" name="page" value=(number)><input type="hidden" name="mode" value=(mode.clone())><button type="submit" aria-label="删除笔记">"Delete"</button></form>
                        </div>
                    </div>
                }
            </aside><div class="right-space"></div></div>
            if pdf { <aside id="source-panel" class="tool-panel source-panel" aria-label="PDF 原页" hidden=(true)>
                <div class="panel-heading"><strong>"PDF 原页 · "(number)" / "(total)</strong><button class="panel-close" type="button" data-close-panel="true" aria-label="关闭原页">"×"</button></div>
                <a href=(pdf_page.clone()) target="_blank" rel="noopener">"在新标签页查看原页 ↗"</a><img src=(pdf_page) alt=(format!("PDF 第 {number} 页"))>
            </aside> }
            <aside id="notes-drawer" class="tool-panel notes-drawer" aria-label="笔记列表" hidden=(true)>
                <div class="panel-heading"><div><span class="eyebrow">"Annotations"</span><strong>"全书笔记"</strong></div><button class="panel-close" type="button" data-close-panel="true" aria-label="关闭笔记列表">"×"</button></div>
                <div class="drawer-filters" role="group" aria-label="笔记筛选"><button type="button" data-note-filter="all" class="active">"全部"</button><button type="button" data-note-filter="note">"笔记"</button><button type="button" data-note-filter="highlight">"划线"</button><button type="button" data-note-filter="unanswered">"未解答"</button></div>
                <div class="paper-note"><strong>"全书笔记"</strong><form method="post" action=(format!("/read/{id}/paper-note"))><input type="hidden" name="page" value=(number)><input type="hidden" name="mode" value=(mode.clone())><textarea name="body" placeholder="记录全文要点…" maxlength="32000">(paper_note)</textarea><button type="submit">"保存全文笔记"</button></form></div>
                <p class="drawer-hint">"全文批注 · 点击引文跳转到对应页。"</p>
                for n in drawer_notes { <div class=(format!("drawer-note {}", n.kind)) data-note=(n.id.clone()) data-anchor=(n.anchor.clone()) data-kind=(n.kind.clone()) data-answered=(n.answered.to_string()) data-page=(n.page.to_string()) data-side=(n.side) data-prefix=(n.prefix) data-suffix=(n.suffix) data-created=(n.created.clone()) data-source-annotation=(n.source_annotation)>
                    <span class="tag">(format!("{} · {} {}", if n.kind == "question" { "My question" } else if n.kind == "highlight" { "My highlight" } else { "My note" }, if pdf { "p." } else { "章节" }, n.page))</span>
                    if !n.quote.is_empty() { <a class="quote" href=(format!("/read/{id}?page={}&mode={mode}#block-{}", n.page, n.anchor))>(n.quote)</a> }
                    <div class="body">(Unescaped::new_unchecked(n.body_html.clone()))</div>
                    for reply in n.replies.clone() { <div class="ai-reply"><small>(reply.by)</small><div class="body">(Unescaped::new_unchecked(reply.body_html))</div></div> }
                    <div class="note-ai-slot">if matches!(n.kind.as_str(), "note" | "question") && !n.body.trim().is_empty() {
                        <button class="note-ai-action" type="button" data-ask-note=(n.id.clone()) disabled=(!ask_available) title=(if ask_available { "基于引文与笔记直接提问" } else { "请先在模型设置中启用并配置 Ask AI" })>(if n.kind == "question" { "✳ Ask AI to answer" } else { "✳ Ask AI for feedback" })</button>
                    }</div>
                    <div class="card-actions" aria-label="笔记操作"><button type="button" data-edit-note=(n.id.clone()) aria-label="编辑笔记">"Edit"</button>
                        if matches!(n.kind.as_str(), "note" | "question") { <form method="post" action=(format!("/read/{id}/notes/action"))><input type="hidden" name="note_id" value=(n.id.clone())><input type="hidden" name="page" value=(number)><input type="hidden" name="mode" value=(mode.clone())><button type="submit" name="action" value="kind" data-toggle-note=(n.id.clone())>(if n.kind == "question" { "Make it a note" } else { "Make it a question" })</button></form> }
                        <form method="post" action=(format!("/read/{id}/notes/delete"))><input type="hidden" name="note_id" value=(n.id.clone())><input type="hidden" name="page" value=(number)><input type="hidden" name="mode" value=(mode.clone())><button type="submit" aria-label="删除笔记">"Delete"</button></form>
                    </div>
                    <details class="note-edit" hidden=(true)><summary>"笔记数据"</summary><form method="post" action=(format!("/read/{id}/notes/edit"))>
                        <input type="hidden" name="note_id" value=(n.id)><input type="hidden" name="page" value=(number)><input type="hidden" name="mode" value=(mode.clone())>
                        <label>"类型"<select name="kind"><option value="note" selected=(n.kind == "note")>"笔记"</option><option value="question" selected=(n.kind == "question")>"问题"</option><option value="highlight" selected=(n.kind == "highlight")>"划线"</option></select></label>
                        <label>"标记"<select name="style"><option value="highlight" selected=(n.style != "underline")>"高亮"</option><option value="underline" selected=(n.style == "underline")>"下划线"</option></select></label>
                        <label>"颜色"<select name="color"><option value="yellow" selected=(n.color == "yellow")>"黄"</option><option value="green" selected=(n.color == "green")>"绿"</option><option value="blue" selected=(n.color == "blue")>"蓝"</option><option value="pink" selected=(n.color == "pink")>"红"</option></select></label>
                        <label>"问题状态"<select name="answered"><option value="no" selected=(!n.answered)>"未解答"</option><option value="yes" selected=(n.answered)>"已解答"</option></select></label>
                        <label class="full">"内容"<textarea name="body" maxlength="16000">(n.body)</textarea></label><button type="submit">"保存修改"</button><span class="autosave-status" aria-live="polite"></span>
                    </form></details>
                </div> }
            </aside>
            if ask_available { <aside id="chat-panel" class="tool-panel chat-panel" aria-label="Ask AI 对话" hidden=(true)>
                <div class="panel-heading"><strong>"✳ Ask AI"</strong><button class="panel-close" type="button" data-close-panel="true" aria-label="关闭 Ask AI">"×"</button></div>
                <p class="drawer-hint">"基于当前书籍及选定引文提问；请求经本机 Rust 服务器发送，密钥不会进入浏览器。"</p>
                <div class="chat-messages">for message in chat_messages { <div class=(format!("chat-message {}",message.role))><span>(if message.failed { "AI · 请求失败" } else if message.role == "assistant" { "AI" } else { "你" })</span><div class="body">(Unescaped::new_unchecked(message.body_html))</div></div> }</div>
                <form id="chat-form" method="post" action=(format!("/read/{id}/ask"))>
                    <input type="hidden" name="anchor"><input type="hidden" name="quote"><input type="hidden" name="note_id"><input type="hidden" name="page" value=(number)><input type="hidden" name="mode" value=(mode.clone())>
                    <label>"提问"<textarea name="text" maxlength="4000" required=(true) placeholder="对当前书籍或章节有什么疑问？"></textarea></label><button type="submit">"发送问题"</button>
                </form>
            </aside> }</div>
            <div id="floating" class="floating" role="toolbar" aria-label="选文操作" data-mark-style="highlight">
                <span class="mark-styles" role="group" aria-label="标记样式">
                    <button type="button" data-style="highlight" class="selected" aria-label="高亮" aria-pressed="true" title="选择高亮，再点击颜色应用">
                        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" focusable="false"><path d="m9 11-6 6v3h9l3-3"/><path d="m22 12-4.6 4.6a2 2 0 0 1-2.8 0l-5.2-5.2a2 2 0 0 1 0-2.8L14 4"/></svg>
                    </button>
                    <button type="button" data-style="underline" aria-label="下划线" aria-pressed="false" title="选择下划线，再点击颜色应用">
                        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" focusable="false"><path d="M6 4v6a6 6 0 0 0 12 0V4"/><line x1="4" x2="20" y1="20" y2="20"/></svg>
                    </button>
                </span><span class="swatches" role="group" aria-label="选择颜色并应用标记"><button type="button" data-color="yellow" aria-label="黄色高亮"></button><button type="button" data-color="green" aria-label="绿色高亮"></button><button type="button" data-color="blue" aria-label="蓝色高亮"></button><button type="button" data-color="pink" aria-label="红色高亮"></button></span><button type="button" data-action="original" aria-expanded="false">"En / 译文"</button><button type="button" data-action="edit">"修订译文"</button>if ask_available { <button type="button" data-action="ask">"Ask AI"</button> }<button type="button" data-action="note">"笔记"</button><button type="button" data-action="question">"问题"</button><button type="button" data-action="copy">"复制"</button><button type="button" data-action="close">"×"</button>
            </div>
            <div id="margin-editor" class="margin-editor" aria-label="页边笔记编辑器" hidden=(true)></div>
            <div id="margin-drafts" hidden=(true)></div>
            <template id="margin-note-template">
                <form class="margin-note-form" method="post" action=(format!("/read/{id}/notes/margin"))>
                    <input type="hidden" name="note_id"><input type="hidden" name="session_id"><input type="hidden" name="revision" value="0">
                    <input type="hidden" name="anchor"><input type="hidden" name="side"><input type="hidden" name="quote"><input type="hidden" name="prefix"><input type="hidden" name="suffix"><input type="hidden" name="source_annotation">
                    <input type="hidden" name="kind" value="note"><input type="hidden" name="color" value="yellow"><input type="hidden" name="answered" value="no">
                    <input type="hidden" name="page" value=(number)><input type="hidden" name="mode" value=(mode.clone())>
                    <div class="margin-note-header"><span class="margin-note-title">"My note"</span><span class="margin-question-badge" hidden=(true)>"Unanswered"</span><time></time><button type="button" data-margin-close="true" aria-label="关闭笔记" title="关闭 · Esc">"×"</button></div>
                    <blockquote class="margin-note-quote"></blockquote>
                    <textarea name="body" aria-label="My note text" maxlength="16000" placeholder="Write your understanding, questions or ideas… (supports $math$, **bold**)" spellcheck="false"></textarea>
                    <div class="margin-note-options"><span role="group" aria-label="笔记类型"><button type="button" data-note-kind="note" aria-pressed="true">"Notes"</button><button type="button" data-note-kind="question" aria-pressed="false">"Question"</button></span><span class="note-colors" role="group" aria-label="引文颜色"><button type="button" data-note-color="yellow" aria-label="黄色"></button><button type="button" data-note-color="green" aria-label="绿色"></button><button type="button" data-note-color="blue" aria-label="蓝色"></button><button type="button" data-note-color="pink" aria-label="红色"></button></span></div>
                    <button class="margin-answer" type="button" data-note-answered="true" hidden=(true)>"Mark as answered"</button>
                    <div class="margin-note-footer"><button type="button" data-note-retry="true">"Retry"</button><span class="autosave-status" aria-live="polite">"Autosave"</span><span>" · Esc to close"</span></div>
                </form>
            </template>
            <form id="mark-form" hidden=(true) method="post" action=(notes_action)>
                    <input type="hidden" name="anchor"><input type="hidden" name="side"><input type="hidden" name="quote"><input type="hidden" name="prefix"><input type="hidden" name="suffix"><input type="hidden" name="kind"><input type="hidden" name="style" value="highlight"><input type="hidden" name="color" value="yellow">
                    <input type="hidden" name="page" value=(number)><input type="hidden" name="mode" value=(mode.clone())>
                    <input type="hidden" name="body">
            </form>
        </body></html>
    })
}
