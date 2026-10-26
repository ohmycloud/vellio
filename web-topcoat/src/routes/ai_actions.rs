//! Ai HTTP handlers.
use super::*;

#[route(POST "/read/{book_id}/translate")]
async fn translate_book(cx: &Cx, Form(input): Form<AiPageInput>) -> Result<SeeOther> {
    let id = path_param::<BookId>(cx).to_string();
    let book = library().workspace(&id).map_err(|_| not_found())?;
    let prefs = model_preferences().map_err(Error::from_anyhow)?;
    let config = prefs
        .connection(true)
        .map_err(|_| bad_request("翻译模型尚未就绪，请检查首页的模型设置"))?;
    let options = ai::TranslationOptions {
        pages_per_batch: prefs.pages_per_batch,
        parallel: prefs.parallel,
    };
    let concurrent_papers = prefs
        .extra
        .get("concurrent_papers")
        .and_then(Value::as_u64)
        .unwrap_or(1)
        .clamp(1, 4) as usize;
    let raw = book.validated_state().map_err(Error::from_anyhow)?;
    let pages = raw["paper"]["meta"][if raw["paper"]["meta"]["format"] == "epub" {
        "chapter_count"
    } else {
        "page_count"
    }]
    .as_u64()
    .unwrap_or(1);
    if (prefs.ask_above == 0 || pages > prefs.ask_above as u64)
        && input.confirm.as_deref() != Some("yes")
    {
        return Err(bad_request("长篇书籍或文档需要确认模型调用费用").into());
    }
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut tasks = translations().lock().unwrap_or_else(|p| p.into_inner());
        if tasks.contains_key(&id) {
            return Err(bad_request("该书正在翻译").into());
        }
        if tasks.len() >= concurrent_papers {
            return Err(bad_request("已达到模型设置中的同时翻译书目上限").into());
        }
        tasks.insert(id.clone(), cancel.clone());
    }
    tokio::task::spawn_blocking(move || {
        if let Err(error) = ai::translate_batched(&book, &config, options, &cancel, |_, _| {}) {
            eprintln!("Web translation interrupted: {error:#}");
        }
        translations()
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(&id);
    });
    let mode = if matches!(
        input.mode.as_str(),
        "original" | "translation" | "bilingual"
    ) {
        input.mode
    } else {
        "bilingual".into()
    };
    Ok(see_other(format!(
        "/read/{}?page={}&mode={mode}",
        path_param::<BookId>(cx),
        input.page
    )))
}

#[route(POST "/read/{book_id}/translate/cancel")]
async fn cancel_translate(cx: &Cx, Form(input): Form<AiPageInput>) -> Result<SeeOther> {
    let id = path_param::<BookId>(cx);
    library().workspace(id).map_err(|_| not_found())?;
    if let Some(cancel) = translations()
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .get(id)
    {
        cancel.store(true, std::sync::atomic::Ordering::Relaxed);
    }
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

#[route(GET "/read/{book_id}/job")]
async fn translation_status(cx: &Cx) -> Result<Json<Value>> {
    let id = path_param::<BookId>(cx);
    let book = library().workspace(id).map_err(|_| not_found())?;
    let job = book.load("job").map_err(Error::from_anyhow)?;
    let active = translations()
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .contains_key(id);
    Ok(Json(
        json!({"state": if active && !job["state"].is_string() { "starting" } else { job["state"].as_str().unwrap_or("idle") },
        "active":active,"done":job["done"],"total":job["total"]}),
    ))
}

#[route(POST "/read/{book_id}/ask")]
async fn ask_ai(cx: &Cx, Form(input): Form<AskInput>) -> Result<SeeOther> {
    if input.text.trim().is_empty()
        || input.text.len() > 4_000
        || input.quote.as_deref().is_some_and(|q| q.len() > 1_000)
    {
        return Err(bad_request("提问内容无效").into());
    }
    let id = path_param::<BookId>(cx);
    let book = library().workspace(id).map_err(|_| not_found())?;
    let prefs = model_preferences().map_err(Error::from_anyhow)?;
    if !prefs.features.ask_ai {
        return Err(bad_request("桌面设置已关闭 Ask AI").into());
    }
    let config = prefs
        .connection(false)
        .map_err(|_| bad_request("Ask AI 模型尚未就绪，请检查首页的模型设置"))?;
    let state = book.validated_state().map_err(Error::from_anyhow)?;
    if input.anchor.as_deref().is_some_and(|anchor| {
        !state["paper"]["blocks"]
            .as_array()
            .is_some_and(|bs| bs.iter().any(|b| b["id"] == anchor))
    }) {
        return Err(bad_request("提问定位已失效").into());
    }
    let note = input.note_id.as_deref().filter(|id| !id.is_empty());
    if note.is_some_and(|note_id| {
        !state["reader"]["notes"][note_id].is_object()
            || state["reader"]["notes"][note_id]["anchor"].as_str() != input.anchor.as_deref()
            || state["reader"]["notes"][note_id]["deleted"] == true
    }) {
        return Err(bad_request("笔记已改变，请刷新后重试").into());
    }
    let request = ai::ChatRequest {
        text: input.text,
        refs: input
            .anchor
            .map(|anchor| ai::ChatReference {
                anchor,
                quote: input.quote.unwrap_or_default(),
            })
            .into_iter()
            .collect(),
        note: note.map(str::to_owned),
        ..Default::default()
    };
    let (outcome, recorded_error) = tokio::task::spawn_blocking(move || {
        let outcome = ai::ask_with_context(
            &book,
            "web-reader",
            &request,
            &config,
            &AtomicBool::new(false),
            |_| {},
        );
        let recorded_error = outcome.is_err()
            && book.load("chat").ok().is_some_and(|chat| {
                chat["threads"].as_array().is_some_and(|threads| {
                    threads.iter().any(|thread| {
                        thread["id"] == "web-reader"
                            && thread["messages"].as_array().is_some_and(|messages| {
                                let len = messages.len();
                                len >= 2
                                    && messages[len - 1]["status"] == "error"
                                    && messages[len - 2]["content"] == request.text
                            })
                    })
                })
            });
        (outcome, recorded_error)
    })
    .await?;
    if let Err(error) = outcome {
        eprintln!("Web Ask AI interrupted: {error:#}");
        if !recorded_error {
            return Err(Error::from_anyhow(error));
        }
    }
    let mode = if matches!(
        input.mode.as_str(),
        "original" | "translation" | "bilingual"
    ) {
        input.mode
    } else {
        "bilingual".into()
    };
    Ok(see_other(format!(
        "/read/{id}?page={}&mode={mode}#chat-panel",
        input.page
    )))
}
