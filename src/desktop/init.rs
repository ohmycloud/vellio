//! Reader construction, restored inputs and UI subscriptions.
use super::*;

impl ReaderView {
    pub(super) fn new(
        mut raw: Value,
        library: Option<Library>,
        workspace: Option<Workspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        if raw.get("chat").is_some()
            && let Ok(chat) = ai::normalize_chat_state(raw["chat"].clone())
        {
            raw["chat"] = chat;
        }
        let state = ReaderState::from_value(raw.clone()).unwrap();
        let epub_code_language = code::preferred_epub_language(&state.paper.blocks);
        let layout = PageLayout::from_state(&raw, &state);
        let items = library
            .as_ref()
            .and_then(|l| l.list().ok())
            .unwrap_or_default();
        let (session, recovery_status) = Self::load_session(workspace.as_ref());
        let appearance = session
            .as_ref()
            .map(|s| s.appearance.clone())
            .unwrap_or_default();
        let (reading_index, offset) = session
            .as_ref()
            .and_then(|s| {
                s.position(
                    &raw["reader"]["progress"],
                    &state
                        .paper
                        .blocks
                        .iter()
                        .map(|b| b.id.as_str())
                        .collect::<Vec<_>>(),
                )
            })
            .unwrap_or_else(|| {
                (
                    state
                        .paper
                        .blocks
                        .iter()
                        .position(|b| {
                            Some(b.id.as_str()) == raw["reader"]["progress"]["block"].as_str()
                        })
                        .unwrap_or(0),
                    0.0,
                )
            });
        let selected = session
            .as_ref()
            .and_then(|s| s.selected_key.as_ref())
            .and_then(|key| {
                state
                    .paper
                    .blocks
                    .iter()
                    .position(|b| b.id == key.trim_end_matches("#caption"))
            })
            .unwrap_or(reading_index);
        let page = layout
            .region(reading_index)
            .map(|r| r.page)
            .unwrap_or_else(|| {
                state
                    .paper
                    .blocks
                    .get(reading_index)
                    .map(|b| b.page.max(1))
                    .unwrap_or(1)
            });
        let list = ListState::new(state.paper.blocks.len(), ListAlignment::Top, px(600.));
        list.scroll_to(ListOffset {
            item_ix: reading_index,
            offset_in_item: px(offset),
        });
        let chat_list = ListState::new(0, ListAlignment::Bottom, px(300.));
        let weak = cx.entity().downgrade();
        let reading_view = weak.clone();
        list.set_scroll_handler(move |_, window, _| {
            let reading_view = reading_view.clone();
            window.on_next_frame(move |_, cx| {
                let _ = reading_view.update(cx, |this, cx| {
                    // Read the logical anchor after variable-height rows have
                    // been measured, rather than the pre-layout estimate.
                    this.follow_block(this.list.logical_scroll_top().item_ix);
                    this.selection = None;
                    this.annotation_popup = None;
                    this.capture_position();
                    cx.notify();
                });
            });
        });
        chat_list.set_scroll_handler(move |event, _, cx| {
            let _ = weak.update(cx, |this, _| this.chat_following = event.is_following_tail);
        });
        let tid = raw["chat"]["threads"]
            .as_array()
            .and_then(|t| t.last())
            .and_then(|t| t["id"].as_str())
            .unwrap_or("t-native")
            .to_owned();
        let fonts = typography::Fonts::resolve(cx);
        let home = home::HomeState::new(
            library.as_ref(),
            &items,
            library.is_some() && workspace.is_none(),
            window,
            cx,
        );
        let settings = settings::SettingsState::new(library.as_ref(), window, cx);
        let appearance = if session.is_some() {
            appearance
        } else {
            settings.committed.appearance.clone()
        };
        let reading_mode = session
            .as_ref()
            .map(Session::resolved_reading_mode)
            .unwrap_or(if settings.committed.bilingual {
                ReadingMode::Bilingual
            } else {
                ReadingMode::Original
            });
        let original_first = session.as_ref().is_none_or(|s| s.original_first);
        let session = if workspace.is_some() && session.is_none() && recovery_status.is_none() {
            Some(Session {
                appearance: appearance.clone(),
                bilingual: reading_mode == ReadingMode::Bilingual,
                reading_mode: Some(reading_mode),
                original_first,
                show_pdf: false,
                ..Default::default()
            })
        } else {
            session
        };
        let mut view = Self {
            reader_focus: cx.focus_handle(),
            home,
            settings,
            raw,
            state,
            epub_code_language,
            epub_reflow: epub::ReflowState::default(),
            epub_reflow_displayed: (0, 0, 0),
            layout,
            library,
            workspace,
            items,
            reading_mode,
            original_first,
            reading_order_open: false,
            reading_order_button_bounds: None,
            reading_order_menu_bounds: None,
            font_slider: cx.new(|_| {
                SliderState::new()
                    .min(14.)
                    .max(32.)
                    .step(1.)
                    .default_value(appearance.font_size)
            }),
            width_slider: cx.new(|_| {
                SliderState::new()
                    .min(25.)
                    .max(60.)
                    .step(1.)
                    .default_value(appearance.line_width)
            }),
            spacing_slider: cx.new(|_| {
                SliderState::new()
                    .min(1.2)
                    .max(2.6)
                    .step(0.1)
                    .default_value(appearance.line_spacing)
            }),
            appearance,
            appearance_open: false,
            navigation: None,
            pdf_outline: Vec::new(),
            navigation_focus: cx.focus_handle(),
            navigation_scroll: ScrollHandle::new(),
            navigation_button_bounds: [None, None],
            navigation_menu_bounds: None,
            appearance_focus: cx.focus_handle(),
            appearance_button_bounds: None,
            appearance_menu_bounds: None,
            show_pdf: session.as_ref().is_some_and(|s| s.show_pdf),
            panel: Self::restored_panel(session.as_ref()),
            panel_open: false,
            annotation_filter: AnnotationFilter::All,
            paper_notes_tab: false,
            paper_note_input: cx.new(|cx| {
                TextareaState::new(window, cx)
                    .rows(10)
                    .placeholder("Notes about this book…")
            }),
            restore_paper_note: false,
            persisted_session: session.clone(),
            session,
            editor_base: String::new(),
            inline_edit: None,
            restore_inputs: false,
            draft_conflict: false,

            selected,
            active_reading_paragraph: None,
            selection: None,
            annotation_popup: None,
            margin_editor: None,
            margin_writer: MarginWriter::default(),
            margin_records: HashMap::new(),
            margin_card_heights: HashMap::new(),
            discussion_scrolls: Default::default(),
            block_bounds: HashMap::new(),
            annotation_jump_generation: 0,
            navigation_note: None,
            annotation_scroll: None,
            hovered_annotation: None,
            hovered_note_card: None,
            margin_input: cx.new(|cx| {
                TextareaState::new(window, cx).rows(4).placeholder(
                    "Write your understanding,\nquestions or ideas… (supports\n$math$, **bold**)",
                )
            }),
            note_selection: None,
            note_kind: "note",
            text_views: HashMap::new(),
            pdf_code_html: HashMap::new(),
            pdf_code_loading: std::collections::HashSet::new(),
            pdf_code_prepared: HashMap::new(),
            pdf_code_cache: None,
            pdf_code_active: false,
            pdf_code_generation: 0,
            pdf_code_focus: reading_index,
            pdf_code_forward: true,
            pdf_code_needs_plan: true,
            original_blocks: HashMap::new(),
            paragraph_languages: HashMap::new(),
            unpaired_paragraphs: std::collections::HashSet::new(),
            page,
            page_block: Some(reading_index),
            page_bounds: None,
            page_image: None,
            block_images: HashMap::new(),
            image_loading: std::collections::HashSet::new(),
            image_preview: None,
            fonts,
            formulas: HashMap::new(),
            inline_formulas: HashMap::new(),
            rich_markdown_cache: Default::default(),
            updates: updates::UpdateUi::load(),
            list,
            chat_list,
            chat_following: true,
            chat_ui: chat::ChatUi::new(window, cx),
            editor: cx.new(|cx| {
                TextareaState::new(window, cx)
                    .rows(8)
                    .placeholder("修订此段译文")
            }),
            note: cx.new(|cx| {
                TextareaState::new(window, cx)
                    .rows(3)
                    .placeholder("写下笔记或问题")
            }),
            quote: cx.new(|cx| InputState::new(window, cx).placeholder("可粘贴该段中的引文")),
            question: cx.new(|cx| {
                TextareaState::new(window, cx)
                    .auto_grow(1, 7)
                    .submit_on_enter(true)
                    .placeholder("问点什么…")
            }),
            search: cx.new(|cx| InputState::new(window, cx).placeholder("搜索正文")),
            tid,
            live: String::new(),
            live_question: String::new(),
            status: recovery_status.unwrap_or_else(|| "就绪 · 草稿与阅读位置自动保存".into()),
            busy: false,
            cancel: Arc::new(AtomicBool::new(false)),
        };
        let current_thread = view.tid.clone();
        view.select_chat_thread(&current_thread, cx);
        view.enforce_features();
        view.start_update_checks(cx);
        view.load_page();
        view.load_outline(cx);
        view.fill_editor(window, cx);
        view.fill_paper_note(window, cx);
        view.compile_formulas(cx);
        cx.subscribe(&view.home.rename_input, |_, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                cx.notify();
            }
        })
        .detach();
        cx.subscribe(&view.home.search, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.home.scroll = ScrollHandle::new();
                cx.notify();
            }
        })
        .detach();
        for (i, slider) in [&view.font_slider, &view.width_slider, &view.spacing_slider]
            .into_iter()
            .enumerate()
        {
            cx.subscribe(slider, move |this, _, event: &SliderEvent, cx| {
                if let SliderEvent::Change(value) = event {
                    match i {
                        0 => this.appearance.font_size = value.end(),
                        1 => this.appearance.line_width = value.end(),
                        _ => this.appearance.line_spacing = value.end(),
                    }
                    this.appearance.normalize();
                    this.appearance_changed(cx);
                } else if matches!(event, SliderEvent::Release(_)) {
                    this.flush_session();
                }
            })
            .detach();
        }
        cx.subscribe(&view.margin_input, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change)
                && let Some(editor) = &mut this.margin_editor
            {
                let body = this.margin_input.read(cx).value().to_string();
                if editor.note.record["body"] != body {
                    editor.note.record["body"] = json!(body);
                    editor.note.record["updated"] = json!(now());
                    editor.changed_at = Instant::now();
                }
                cx.notify();
            }
        })
        .detach();
        for input in [&view.editor, &view.note] {
            cx.subscribe(input, |this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    this.capture_inputs(cx);
                    cx.notify();
                }
            })
            .detach();
        }
        cx.subscribe_in(
            &view.question,
            window,
            |this, _, event: &InputEvent, window, cx| match event {
                InputEvent::PressEnter { shift: false, .. } => this.ask(window, cx),
                InputEvent::Change => cx.notify(),
                _ => {}
            },
        )
        .detach();
        cx.subscribe(&view.quote, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.capture_inputs(cx);
                cx.notify();
            }
        })
        .detach();
        cx.on_app_quit(|this, cx| {
            this.queue_margin_note();
            this.capture_inputs(cx);
            this.capture_position();
            this.flush_session();
            async {}
        })
        .detach();
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(400))
                    .await;
                if this
                    .update(cx, |this, cx| {
                        if this.margin_editor.as_ref().is_some_and(|editor| {
                            editor.changed_at.elapsed() >= Duration::from_millis(600)
                        }) {
                            this.queue_margin_note();
                        }
                        if this.settings.relocating {
                            return;
                        }
                        this.receive_margin_notes(cx);
                        this.capture_inputs(cx);
                        this.capture_position();
                        if this.flush_session() {
                            cx.notify();
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        view
    }
}
