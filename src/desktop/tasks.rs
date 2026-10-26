//! Background task dispatch and UI-thread event application.
use super::*;

impl ReaderView {
    pub(super) fn worker(
        &mut self,
        cx: &mut Context<Self>,
        work: impl FnOnce(Arc<AtomicBool>, mpsc::Sender<Event>) + Send + 'static,
    ) {
        self.busy = true;
        self.home.cancellable_task = false;
        self.cancel = Arc::new(AtomicBool::new(false));
        let cancel = self.cancel.clone();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let errors = tx.clone();
            let failed = cancel.clone();
            if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| work(cancel, tx))).is_err()
            {
                failed.store(true, Ordering::Relaxed);
                let _ = errors.send(Event::Error("后台任务异常，请重试".into()));
            }
        });
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(80))
                    .await;
                let mut disconnected = false;
                let mut events = Vec::new();
                loop {
                    match rx.try_recv() {
                        Ok(e) => events.push(e),
                        Err(mpsc::TryRecvError::Empty) => break,
                        Err(mpsc::TryRecvError::Disconnected) => {
                            disconnected = true;
                            break;
                        }
                    }
                }
                // Keep waiting off the UI entity when the worker has no
                // progress. Otherwise an 80ms poll needlessly invalidates the
                // retained text/layout caches throughout a slow AI/PDF task.
                if events.is_empty() && !disconnected {
                    if this.upgrade().is_none() {
                        break;
                    }
                    continue;
                }
                if this
                    .update(cx, |view, cx| {
                        for event in events {
                            match event {
                                Event::Catalog(result) => match result {
                                    Ok(items) => {
                                        view.items = items;
                                        view.home.load_covers(view.library.as_ref(), &view.items);
                                    }
                                    Err(e) => view.home.notice = format!("文献库读取失败：{e}"),
                                },
                                Event::Loaded(ws, raw) => {
                                    view.home.visible = false;
                                    view.home.cancellable_task = false;
                                    gpui_kit::component::Theme::change(
                                        gpui_kit::component::ThemeMode::Light,
                                        None,
                                        cx,
                                    );
                                    typography::apply_theme(false, cx);
                                    view.queue_margin_note();
                                    view.margin_editor = None;
                                    view.margin_records.clear();
                                    view.margin_card_heights.clear();
                                    view.discussion_scrolls.get_mut().clear();
                                    view.capture_inputs(cx);
                                    view.capture_position();
                                    view.flush_session();
                                    view.selection = None;
                                    view.annotation_popup = None;
                                    view.note_selection = None;
                                    view.text_views.clear();
                                    view.active_reading_paragraph = None;
                                    view.inline_edit = None;
                                    view.block_bounds.clear();
                                    view.navigation_note = None;
                                    view.annotation_scroll = None;
                                    view.hovered_annotation = None;
                                    view.annotation_jump_generation =
                                        view.annotation_jump_generation.wrapping_add(1);
                                    view.block_images.clear();
                                    view.image_loading.clear();
                                    view.image_preview = None;
                                    view.original_blocks.clear();
                                    view.clear_paragraph_layouts();
                                    let (session, warning) = Self::load_session(Some(&ws));
                                    view.persisted_session = session.clone();
                                    view.session = session;
                                    view.appearance = view
                                        .session
                                        .as_ref()
                                        .map(|s| s.appearance.clone())
                                        .unwrap_or_else(|| {
                                            view.settings.committed.appearance.clone()
                                        });
                                    view.appearance_open = false;
                                    view.navigation = None;
                                    view.workspace = Some(ws);
                                    view.reading_order_open = false;
                                    view.reading_mode = view
                                        .session
                                        .as_ref()
                                        .map(Session::resolved_reading_mode)
                                        .unwrap_or(if view.settings.committed.bilingual {
                                            ReadingMode::Bilingual
                                        } else {
                                            ReadingMode::Original
                                        });
                                    view.original_first =
                                        view.session.as_ref().is_none_or(|s| s.original_first);
                                    view.show_pdf =
                                        view.session.as_ref().is_some_and(|s| s.show_pdf);
                                    if view.session.is_none() && warning.is_none() {
                                        view.session = Some(Session {
                                            appearance: view.appearance.clone(),
                                            bilingual: view.reading_mode == ReadingMode::Bilingual,
                                            reading_mode: Some(view.reading_mode),
                                            original_first: view.original_first,
                                            show_pdf: false,
                                            ..Default::default()
                                        });
                                    }
                                    view.enforce_features();
                                    view.restore_inputs = true;
                                    view.selected = raw["reader"]["progress"]["block"]
                                        .as_str()
                                        .and_then(|id| {
                                            raw["paper"]["blocks"].as_array().and_then(|bs| {
                                                bs.iter().position(|b| b["id"] == id)
                                            })
                                        })
                                        .unwrap_or(0);
                                    view.formulas.clear();
                                    view.inline_formulas.clear();
                                    view.rich_markdown_cache.get_mut().clear();
                                    view.apply_state(raw);
                                    let (index, offset) = view.restored_position();
                                    view.panel = Self::restored_panel(view.session.as_ref());
                                    view.panel_open = false;
                                    view.annotation_filter = AnnotationFilter::All;
                                    view.paper_notes_tab = false;
                                    view.restore_paper_note = true;
                                    view.selected = view
                                        .session
                                        .as_ref()
                                        .and_then(|s| s.selected_key.as_ref())
                                        .and_then(|key| {
                                            view.state.paper.blocks.iter().position(|b| {
                                                b.id == key.trim_end_matches("#caption")
                                            })
                                        })
                                        .unwrap_or(index);
                                    view.page_block = None;
                                    view.follow_block(index);
                                    view.list.reset(view.state.paper.blocks.len());
                                    view.list.scroll_to(ListOffset {
                                        item_ix: index,
                                        offset_in_item: px(offset),
                                    });
                                    view.live.clear();
                                    view.live_question.clear();
                                    view.chat_ui.streaming = false;
                                    view.chat_ui.closing = false;
                                    view.chat_ui.generation =
                                        view.chat_ui.generation.wrapping_add(1);
                                    view.chat_ui.refs.clear();
                                    view.chat_ui.pending_note = None;
                                    view.chat_ui.error = None;
                                    view.chat_ui.model.clear();
                                    view.chat_ui.reasoning = None;
                                    view.chat_ui.no_auto = false;
                                    view.chat_ui.list_open = false;
                                    view.chat_ui.model_open = false;
                                    view.chat_ui.style_open = false;
                                    view.chat_ui.style = ai::AnswerStyle::Standard;
                                    view.tid = view.raw["chat"]["threads"]
                                        .as_array()
                                        .and_then(|t| t.first())
                                        .and_then(|t| t["id"].as_str())
                                        .unwrap_or("t-native")
                                        .to_owned();
                                    let current_thread = view.tid.clone();
                                    view.select_chat_thread(&current_thread, cx);
                                    view.load_page();
                                    view.load_outline(cx);
                                    view.items = view
                                        .library
                                        .as_ref()
                                        .and_then(|l| l.list().ok())
                                        .unwrap_or_default();
                                    view.home.load_covers(view.library.as_ref(), &view.items);
                                    view.status = warning.unwrap_or_default();
                                }
                                Event::Refresh(raw) => view.refresh_state(raw),
                                Event::InlineEditSaved(root, result) => {
                                    if view.workspace.as_ref().is_some_and(|ws| ws.root == root) {
                                        match result {
                                            Ok(raw) => {
                                                view.inline_edit = None;
                                                view.refresh_state(raw);
                                                view.restore_inputs = true;
                                                view.status = "修订已保存".into();
                                            }
                                            Err(error) => {
                                                if let Some(edit) = &mut view.inline_edit {
                                                    edit.saving = false;
                                                    edit.error = Some(error.clone());
                                                }
                                                view.status = error;
                                            }
                                        }
                                    }
                                }
                                Event::SavedDraft(key, edit, note, revert) => {
                                    view.capture_inputs(cx);
                                    let committed_base = view
                                        .state
                                        .paper
                                        .blocks
                                        .iter()
                                        .position(|b| b.id == key.trim_end_matches("#caption"))
                                        .map(|i| text_hash(&view.text_at(i)));
                                    if let Some(session) = &mut view.session {
                                        session.acknowledge(&key, edit.as_ref(), note.as_ref());
                                        if edit.is_some()
                                            && let Some(base) = committed_base
                                            && let Some(draft) = session
                                                .drafts
                                                .get_mut(&key)
                                                .and_then(|d| d.edit.as_mut())
                                        {
                                            draft.base = base;
                                        }
                                    }
                                    if view.selected_key().as_deref() == Some(&key) {
                                        let unchanged_edit = edit.as_ref().is_some_and(|d| {
                                            view.editor.read(cx).value().as_ref() == d.text
                                        });
                                        view.editor_base = view.selected_text();
                                        view.draft_conflict = false;
                                        if revert && unchanged_edit {
                                            view.restore_inputs = true;
                                        }
                                        if let Some(note) = note
                                            && view.current_note(cx) == note
                                        {
                                            view.restore_inputs = true;
                                        }
                                    }
                                    view.flush_session();
                                }
                                Event::Formula(tex, result) => {
                                    view.epub_reflow.invalidate();
                                    view.epub_reflow_displayed =
                                        (usize::MAX, usize::MAX, usize::MAX);
                                    view.formulas.insert(tex, result);
                                }
                                Event::InlineFormula(tex, result) => {
                                    view.epub_reflow.invalidate();
                                    view.epub_reflow_displayed =
                                        (usize::MAX, usize::MAX, usize::MAX);
                                    view.inline_formulas.insert(tex, result);
                                }
                                Event::Delta(s) => {
                                    view.live.push_str(&s);
                                    if view.chat_following {
                                        view.chat_list.scroll_to_end();
                                    }
                                }
                                Event::SettingsResult(s) => {
                                    view.settings.relocating = false;
                                    view.settings.notice = s.clone();
                                    view.settings.logs.push(s);
                                }
                                Event::Relocated(library, workspace, raw) => {
                                    view.library = Some(library);
                                    view.workspace = workspace;
                                    if let Some(raw) = raw {
                                        view.apply_state(raw);
                                    }
                                    view.settings.relocating = false;
                                    view.settings.notice =
                                        "Library copied. Original folder retained.".into();
                                    view.settings.logs.push(view.settings.notice.clone());
                                    view.items = view
                                        .library
                                        .as_ref()
                                        .and_then(|l| l.list().ok())
                                        .unwrap_or_default();
                                    view.home.load_covers(view.library.as_ref(), &view.items);
                                }
                                Event::Status(s) => {
                                    if view.home.visible {
                                        view.home.notice = s.clone();
                                    }
                                    view.status = s;
                                }
                                Event::Error(s) => {
                                    if view.chat_ui.streaming {
                                        view.chat_ui.error = Some(s.clone());
                                    }
                                    if let Some(edit) = &mut view.inline_edit {
                                        edit.saving = false;
                                        edit.error = Some(s.clone());
                                    }
                                    if view.home.visible {
                                        view.home.notice = s.clone();
                                    }
                                    view.settings.logs.push(s.clone());
                                    if view.settings.open {
                                        view.settings.notice = s.clone();
                                    }
                                    view.status = s;
                                }
                            }
                        }
                        if view.settings.logs.len() > 100 {
                            view.settings.logs.drain(..view.settings.logs.len() - 100);
                        }
                        if disconnected {
                            view.busy = false;
                            view.home.cancellable_task = false;
                            view.chat_ui.streaming = false;
                            if !view.cancel.load(Ordering::Relaxed) {
                                view.compile_formulas(cx);
                            }
                        }
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
                if disconnected {
                    break;
                }
            }
        })
        .detach();
        cx.notify();
    }
}
