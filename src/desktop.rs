use easyread_gpui::{
    ai,
    annotation_panel::{self, AnnotationFilter},
    annotations,
    annotations::HighlightColor,
    formula,
    layout::PageLayout,
    margin_note::{MarginNote, MarginWriter},
    model::{ReaderState, text_hash},
    navigation::{heading_depth, split_heading_number},
    pdf_layout_cache::{self, PresentationCache},
    reading_motion::{ScrollPath, ScrollPosition},
    session::{
        Appearance, Draft, EditDraft, NoteDraft, Position, ReadingFont, ReadingMode, Session,
    },
    store::{Library, Workspace, now},
};
use gpui_kit::base::{Disableable, TextSelection};
use gpui_kit::{
    component::{
        Icon, Root, Sizable,
        button::{Button, ButtonCustomVariant, ButtonVariants},
        input::{Input, InputEvent, InputState, Textarea, TextareaState},
        slider::{Slider, SliderEvent, SliderState},
        text::{TextView, TextViewState},
    },
    prelude::FluentBuilder,
    *,
};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};
use uuid::Uuid;

#[cfg(all(test, feature = "desktop-ui-tests"))]
#[path = "desktop_annotation_scope_tests.rs"]
mod annotation_scope_tests;
#[path = "desktop_chat.rs"]
mod chat;
#[path = "desktop_chat_model.rs"]
mod chat_model;
#[cfg(all(test, feature = "desktop-ui-tests"))]
#[path = "desktop_chat_model_tests.rs"]
mod chat_model_tests;
#[path = "desktop_chat_style.rs"]
mod chat_style;
#[cfg(all(test, feature = "desktop-ui-tests"))]
#[path = "desktop_chat_tests.rs"]
mod chat_tests;
#[path = "desktop_code.rs"]
mod code;
#[path = "desktop_epub.rs"]
mod epub;
#[cfg(all(test, feature = "desktop-ui-tests"))]
#[path = "desktop_epub_stability_tests.rs"]
mod epub_stability_tests;
#[cfg(all(test, feature = "desktop-ui-tests"))]
#[path = "desktop_epub_tests.rs"]
mod epub_tests;
#[path = "desktop_home.rs"]
mod home;
#[path = "desktop_inline_edit.rs"]
mod inline_edit;
#[cfg(all(test, feature = "desktop-ui-tests"))]
#[path = "desktop_inline_edit_tests.rs"]
mod inline_edit_tests;
#[path = "desktop_math.rs"]
mod math;
#[cfg(all(test, feature = "desktop-ui-tests"))]
#[path = "desktop_note_hover_tests.rs"]
mod note_hover_tests;
#[cfg(all(test, feature = "desktop-ui-tests"))]
#[path = "desktop_note_selection_tests.rs"]
mod note_selection_tests;
#[cfg(all(test, feature = "desktop-ui-tests"))]
#[path = "desktop_placement_tests.rs"]
mod placement_tests;
#[cfg(all(test, feature = "desktop-ui-tests"))]
#[path = "desktop_progress_tests.rs"]
mod progress_tests;
#[path = "desktop_reading_mode.rs"]
mod reading_mode;
#[cfg(all(test, feature = "desktop-ui-tests"))]
#[path = "desktop_reading_mode_tests.rs"]
mod reading_mode_tests;
#[cfg(all(test, feature = "desktop-ui-tests"))]
#[path = "desktop_rendering_tests.rs"]
mod rendering_tests;
#[path = "desktop_settings.rs"]
mod settings;
#[cfg(all(test, feature = "desktop-ui-tests"))]
#[path = "desktop_source_navigation_tests.rs"]
mod source_navigation_tests;
#[path = "desktop_typography.rs"]
mod typography;
#[path = "desktop_updates.rs"]
mod updates;
#[cfg(all(test, feature = "desktop-ui-tests"))]
#[path = "desktop_updates_tests.rs"]
mod updates_tests;

const READER_BACKGROUND: u32 = 0xf6f3ec;

gpui_kit::assets::icon_assets!(
    SelectionIcons,
    [
        Highlighter,
        Underline,
        MessageSquare,
        CircleQuestionMark,
        BookOpen,
        List,
        File,
        Asterisk,
        Search,
        Plus,
        Moon,
        Sun,
        Settings,
        Star,
        Check,
        ChevronDown,
        Folder,
        Pin,
        Copy,
        Pencil,
        Trash
    ]
);

struct ReaderAssets;
impl AssetSource for ReaderAssets {
    fn load(&self, path: &str) -> anyhow::Result<Option<std::borrow::Cow<'static, [u8]>>> {
        if let Some(bytes) = SelectionIcons.load(path)? {
            return Ok(Some(bytes));
        }
        gpui_kit::assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> anyhow::Result<Vec<SharedString>> {
        let mut paths = gpui_kit::assets::Assets.list(path)?;
        paths.extend(SelectionIcons.list(path)?);
        paths.sort();
        paths.dedup();
        Ok(paths)
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Navigation {
    Library,
    Contents,
}
#[derive(Clone, Copy, PartialEq)]
enum Panel {
    Notes,
    Edit,
    Chat,
}
enum Event {
    Catalog(anyhow::Result<Vec<Value>>),
    Loaded(Workspace, Value),
    Refresh(Value),
    SavedDraft(String, Option<EditDraft>, Option<NoteDraft>, bool),
    InlineEditSaved(std::path::PathBuf, Result<Value, String>),
    Formula(String, Result<Arc<formula::FormulaLayout>, String>),
    InlineFormula(String, Result<Arc<formula::FormulaLayout>, String>),
    Delta(String),
    Status(String),
    Error(String),
    SettingsResult(String),
    Relocated(Library, Option<Workspace>, Option<Value>),
}
#[derive(Clone)]
struct Selection {
    block: usize,
    key: String,
    quote: String,
    prefix: String,
    suffix: String,
    position: Point<Pixels>,
    underline: bool,
}
type BlockImageResult = Result<(Arc<Image>, f32, u32), String>;

struct ImagePreview {
    image: Arc<Image>,
    aspect: f32,
    width: u32,
    zoom: f32,
    scroll: ScrollHandle,
}

struct MarginEditor {
    selection: Selection,
    note: MarginNote,
    changed_at: Instant,
    // Reserve the gutter while the new quote anchor is being painted. The
    // token also prevents an old frame callback from revealing another editor.
    placement_pending: Option<Instant>,
}

struct AnnotationScroll {
    path: ScrollPath,
    started: Instant,
    duration: Duration,
    generation: usize,
    refine: Option<Value>,
}

struct ReaderView {
    reader_focus: FocusHandle,
    home: home::HomeState,
    settings: settings::SettingsState,
    updates: updates::UpdateUi,
    raw: Value,
    state: ReaderState,
    epub_code_language: Option<&'static str>,
    epub_reflow: epub::ReflowState,
    epub_reflow_displayed: (usize, usize, usize),
    layout: PageLayout,
    library: Option<Library>,
    workspace: Option<Workspace>,
    items: Vec<Value>,
    session: Option<Session>,
    persisted_session: Option<Session>,
    editor_base: String,
    inline_edit: Option<inline_edit::InlineTranslationEdit>,
    restore_inputs: bool,
    draft_conflict: bool,
    reading_mode: ReadingMode,
    original_first: bool,
    reading_order_open: bool,
    reading_order_button_bounds: Option<Bounds<Pixels>>,
    reading_order_menu_bounds: Option<Bounds<Pixels>>,
    appearance: Appearance,
    appearance_open: bool,
    navigation: Option<Navigation>,
    pdf_outline: Vec<easyread_gpui::pdf::OutlineEntry>,
    navigation_focus: FocusHandle,
    navigation_scroll: ScrollHandle,
    navigation_button_bounds: [Option<Bounds<Pixels>>; 2],
    navigation_menu_bounds: Option<Bounds<Pixels>>,
    appearance_focus: FocusHandle,
    appearance_button_bounds: Option<Bounds<Pixels>>,
    appearance_menu_bounds: Option<Bounds<Pixels>>,
    font_slider: Entity<SliderState>,
    width_slider: Entity<SliderState>,
    spacing_slider: Entity<SliderState>,
    show_pdf: bool,
    panel: Panel,
    panel_open: bool,
    annotation_filter: AnnotationFilter,
    paper_notes_tab: bool,
    paper_note_input: Entity<TextareaState>,
    restore_paper_note: bool,
    selected: usize,
    active_reading_paragraph: Option<(usize, bool, usize)>,
    selection: Option<Selection>,
    annotation_popup: Option<(String, Point<Pixels>)>,
    margin_editor: Option<MarginEditor>,
    margin_input: Entity<TextareaState>,
    margin_writer: MarginWriter,
    margin_records: HashMap<String, Value>,
    margin_card_heights: HashMap<String, Pixels>,
    block_bounds: HashMap<usize, Bounds<Pixels>>,
    annotation_jump_generation: usize,
    navigation_note: Option<Value>,
    annotation_scroll: Option<AnnotationScroll>,
    hovered_annotation: Option<(String, usize)>,
    hovered_note_card: Option<String>,
    note_selection: Option<Selection>,
    note_kind: &'static str,
    text_views: HashMap<(usize, bool), (String, Entity<TextViewState>)>,
    pdf_code_html: HashMap<usize, (String, Option<easyread_gpui::pdf::CodePresentation>)>,
    pdf_code_loading: std::collections::HashSet<usize>,
    pdf_code_prepared: HashMap<usize, String>,
    pdf_code_cache: Option<PresentationCache>,
    pdf_code_active: bool,
    pdf_code_generation: usize,
    pdf_code_focus: usize,
    pdf_code_forward: bool,
    pdf_code_needs_plan: bool,
    // Per-block overrides for the secondary language (mode-relative).
    original_blocks: HashMap<usize, bool>,
    paragraph_languages: HashMap<usize, reading_mode::ParagraphExpansion>,
    unpaired_paragraphs: std::collections::HashSet<usize>,
    page: usize,
    page_block: Option<usize>,
    page_bounds: Option<(usize, Bounds<Pixels>)>,
    page_image: Option<Arc<Image>>,
    block_images: HashMap<usize, BlockImageResult>,
    image_loading: std::collections::HashSet<usize>,
    image_preview: Option<ImagePreview>,
    fonts: typography::Fonts,
    formulas: HashMap<String, Result<Arc<formula::FormulaLayout>, String>>,
    inline_formulas: HashMap<String, Result<Arc<formula::FormulaLayout>, String>>,
    rich_markdown_cache: std::cell::RefCell<math::MarkdownCache>,
    list: ListState,
    chat_list: ListState,
    chat_following: bool,
    chat_ui: chat::ChatUi,
    editor: Entity<TextareaState>,
    note: Entity<TextareaState>,
    quote: Entity<InputState>,
    question: Entity<TextareaState>,
    search: Entity<InputState>,
    tid: String,
    live: String,
    live_question: String,
    status: String,
    busy: bool,
    cancel: Arc<AtomicBool>,
}
impl Drop for ReaderView {
    fn drop(&mut self) {
        self.queue_margin_note();
        self.cancel.store(true, Ordering::Relaxed);
        self.flush_session();
    }
}
impl ReaderView {
    fn new(
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
    fn worker(
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
    /// A note/color change only changes decorations. Keep measured EPUB pages
    /// and the visible reader list until the underlying text-flow source changes.
    fn apply_state(&mut self, mut raw: Value) -> bool {
        if raw.get("chat").is_some()
            && let Ok(chat) = ai::normalize_chat_state(raw["chat"].clone())
        {
            raw["chat"] = chat;
        }
        let text_flow_changed = self.raw["paper"] != raw["paper"]
            || self.raw["reader"]["edits"] != raw["reader"]["edits"];
        if !text_flow_changed {
            if self.raw["layout"] != raw["layout"] {
                self.layout = PageLayout::from_state(&raw, &self.state);
            }
            // Notes and color live in raw JSON, not in ReaderState. Avoid
            // deserializing the entire book and replacing its layout for them.
            self.raw = raw;
            return false;
        }
        match ReaderState::from_value(raw.clone()) {
            Ok(state) => {
                self.clear_paragraph_layouts();
                self.epub_code_language = code::preferred_epub_language(&state.paper.blocks);
                self.layout = PageLayout::from_state(&raw, &state);
                self.raw = raw;
                self.state = state;
                self.epub_reflow.invalidate();
                self.pdf_code_needs_plan = true;
                true
            }
            Err(e) => {
                self.status = format!("数据错误：{e}");
                false
            }
        }
    }
    fn refresh_state(&mut self, raw: Value) {
        let offset = self.list.logical_scroll_top();
        if self.apply_state(raw) {
            self.list.reset(self.state.paper.blocks.len());
            self.list.scroll_to(offset);
        }
        self.live.clear();
        self.live_question.clear();
    }
    fn load_outline(&mut self, cx: &mut Context<Self>) {
        self.pdf_outline.clear();
        self.pdf_code_html.clear();
        self.clear_paragraph_layouts();
        self.pdf_code_loading.clear();
        self.pdf_code_prepared.clear();
        self.pdf_code_cache = None;
        self.pdf_code_active = false;
        self.pdf_code_needs_plan = true;
        self.pdf_code_generation = self.pdf_code_generation.wrapping_add(1);
        let Some(ws) = self.workspace.clone() else {
            return;
        };
        let Some(rel) = self.raw["paper"]["meta"]["pdf"].as_str() else {
            return;
        };
        let root = ws.root.clone();
        let pdf = ws.asset(rel);
        let index = self.list.logical_scroll_top().item_ix;
        self.update_pdf_focus(index);
        self.request_nearby_pdf_code(index, cx);
        cx.spawn(async move |this, cx| {
            let outline = cx
                .background_executor()
                .spawn(async move { pdf.and_then(|pdf| easyread_gpui::pdf::outline(&pdf)) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.workspace.as_ref().is_some_and(|ws| ws.root == root) {
                    match outline {
                        Ok(outline) => this.pdf_outline = outline,
                        Err(e) => this.status = format!("目录加载失败：{e:#}"),
                    }
                    cx.notify();
                }
            });
        })
        .detach();
    }
    fn contents(&self) -> Vec<(usize, String, usize)> {
        let epub_outline: Vec<_> = self.raw["paper"]["outline"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|entry| {
                let id = entry["block"].as_str()?;
                let index = self.state.paper.blocks.iter().position(|b| b.id == id)?;
                Some((
                    index,
                    entry["title"].as_str()?.to_owned(),
                    entry["depth"].as_u64().unwrap_or(0).min(6) as usize,
                ))
            })
            .collect();
        if !epub_outline.is_empty() {
            return epub_outline;
        }
        let headings: Vec<_> = self
            .state
            .paper
            .blocks
            .iter()
            .enumerate()
            .filter(|(_, b)| b.kind == "heading")
            .map(|(i, b)| {
                let title = self.state.text(&b.id, &b.zh, &b.en).to_owned();
                let level = self.raw["paper"]["blocks"][i]["level"].as_u64();
                let depth = heading_depth(&title, level);
                (i, title, depth)
            })
            .collect();
        if !headings.is_empty() {
            return headings;
        }
        self.pdf_outline
            .iter()
            .filter_map(|entry| {
                self.state
                    .paper
                    .blocks
                    .iter()
                    .position(|b| b.page >= entry.page)
                    .map(|i| {
                        (
                            i,
                            entry.title.clone(),
                            entry.depth.max(heading_depth(&entry.title, None)).min(6),
                        )
                    })
            })
            .collect()
    }
    fn load_page(&mut self) {
        self.page_bounds = None;
        self.page_image = None;
        if let Some(ws) = &self.workspace
            && let Some(rel) = self
                .state
                .paper
                .meta
                .pages
                .get(self.page.saturating_sub(1))
                .and_then(|p| p["img"].as_str())
        {
            match ws.asset(rel).and_then(|p| Ok(std::fs::read(p)?)) {
                Ok(bytes) => {
                    self.page_image = Some(Arc::new(Image::from_bytes(ImageFormat::Webp, bytes)))
                }
                Err(e) => self.status = format!("原页加载失败：{e}"),
            }
        }
    }
    fn follow_block(&mut self, index: usize) {
        let Some(block) = self.state.paper.blocks.get(index) else {
            return;
        };
        let page = self.source_page_number(index).unwrap_or(block.page.max(1));
        self.page_block = Some(index);
        if self.page != page {
            self.page = page;
            self.load_page();
        }
    }

    /// PDF uses the source layout's physical page, not the reading row number.
    /// EPUB has no physical pages: its number belongs to the published reflow.
    fn source_page_number(&self, index: usize) -> Option<usize> {
        let block = self.state.paper.blocks.get(index)?;
        if self.is_epub() {
            if !self.epub_reflow.complete {
                return None;
            }
            // A long block can span several pages. Prefer its visible fragment
            // before looking up its first page (also avoids a full-book scan).
            if self
                .epub_reflow
                .pages
                .get(self.epub_reflow.current)
                .is_some_and(|page| page.iter().any(|fragment| fragment.block == index))
            {
                return Some(self.epub_reflow.current + 1);
            }
            self.epub_reflow
                .pages
                .iter()
                .position(|page| page.iter().any(|fragment| fragment.block == index))
                .map(|page| page + 1)
        } else {
            Some(
                self.layout
                    .region(index)
                    .map(|r| r.page)
                    .unwrap_or(block.page.max(1)),
            )
        }
    }

    fn can_locate_source(&self, index: usize) -> bool {
        self.settings.committed.features.page
            && self.source_page_number(index).is_some_and(|page| {
                self.is_epub() || (page > 0 && page <= self.state.paper.meta.pages.len())
            })
    }

    fn locate_source(&mut self, index: usize, cx: &mut Context<Self>) {
        if !self.can_locate_source(index) {
            return;
        }
        self.queue_margin_note();
        self.margin_editor = None;
        self.panel_open = false;
        self.selection = None;
        self.annotation_popup = None;
        self.show_pdf = true;
        self.follow_block(index);
        // Opening the source pane replaces margin notes; keep the reader's
        // logical position while remeasuring the narrower column.
        self.reset_rows();
        cx.notify();
    }

    fn source_page_button(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let label = self
            .source_page_number(index)
            .map(|page| format!("p.{page}"))
            .unwrap_or_else(|| "p.…".into());
        div()
            .debug_selector(move || format!("source-page-gutter-{index}"))
            .child(
                Button::new(("locate-page", index))
                    .ghost()
                    .xsmall()
                    .h(px(24.))
                    .px(px(6.))
                    .rounded(px(7.))
                    .text_size(px(12.))
                    .text_color(rgb(0x969084))
                    .label(label)
                    .tooltip(if self.is_epub() {
                        "查看 EPUB 原文（页号随排版变化）"
                    } else {
                        "跳转到原 PDF 对应页面"
                    })
                    .disabled(!self.can_locate_source(index))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(move |this, _, _, cx| this.locate_source(index, cx))),
            )
            .into_any_element()
    }

    fn page_view(&self, cx: &mut Context<Self>) -> AnyElement {
        if self.is_epub() {
            let Some(index) = self.page_block else {
                return div().child("请选择要查看的 EPUB 原文").into_any_element();
            };
            return div()
                .debug_selector(|| "epub-source-content".into())
                .text_size(px(16.))
                .line_height(relative(1.7))
                .child(TextView::html(
                    ("epub-source", index),
                    self.reader_body_html(index, true),
                ))
                .into_any_element();
        }
        let Some(image) = self.page_image.clone() else {
            return div().p_3().child("没有可显示的原页").into_any_element();
        };
        let page = self.page;
        let meta = &self.state.paper.meta.pages[page - 1];
        let width = meta["w"]
            .as_f64()
            .filter(|n| n.is_finite() && *n > 0.)
            .unwrap_or(612.);
        let height = meta["h"]
            .as_f64()
            .filter(|n| n.is_finite() && *n > 0.)
            .unwrap_or(792.);
        let boxes = self
            .page_block
            .and_then(|i| self.layout.region(i))
            .filter(|r| r.page == page)
            .map(|r| r.boxes.clone())
            .unwrap_or_default();
        let weak = cx.entity().downgrade();
        div()
            .id("page-image")
            .relative()
            .w_full()
            .aspect_ratio((width / height) as f32)
            .flex_none()
            .cursor_pointer()
            .child(img(image).absolute().size_full())
            .child(
                canvas(
                    move |bounds, _, cx| {
                        let _ = weak.update(cx, |this, _| this.page_bounds = Some((page, bounds)));
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .children(boxes.into_iter().map(|b| {
                let [x0, y0, x1, y1] = b.0;
                div()
                    .absolute()
                    .left(relative(x0))
                    .top(relative(y0))
                    .w(relative(x1 - x0))
                    .h(relative(y1 - y0))
                    .border_2()
                    .border_color(rgb(0x9c5f1c))
                    .bg(rgba(0xecc45c35))
            }))
            .on_click(cx.listener(|this, event: &ClickEvent, window, cx| {
                let Some((page, bounds)) = this.page_bounds else {
                    return;
                };
                if page != this.page || bounds.size.width <= px(0.) || bounds.size.height <= px(0.)
                {
                    return;
                }
                let point = event.position();
                let x = f32::from(point.x - bounds.origin.x) / f32::from(bounds.size.width);
                let y = f32::from(point.y - bounds.origin.y) / f32::from(bounds.size.height);
                if let Some(index) = this.layout.block_at(page, x, y) {
                    // Offscreen variable-height rows still have estimates.
                    // Anchor directly by logical row rather than estimated pixels.
                    this.list.scroll_to(ListOffset {
                        item_ix: index,
                        offset_in_item: px(0.),
                    });
                    this.select(index, window, cx);
                }
            }))
            .into_any_element()
    }
    fn selected_key(&self) -> Option<String> {
        let b = self.state.paper.blocks.get(self.selected)?;
        Some(if matches!(b.kind.as_str(), "table" | "figure") {
            format!("{}#caption", b.id)
        } else {
            b.id.clone()
        })
    }
    fn selected_text(&self) -> String {
        self.text_at(self.selected)
    }
    fn text_at(&self, index: usize) -> String {
        let Some(b) = self.state.paper.blocks.get(index) else {
            return String::new();
        };
        let key = if matches!(b.kind.as_str(), "table" | "figure") {
            format!("{}#caption", b.id)
        } else {
            b.id.clone()
        };
        if matches!(b.kind.as_str(), "table" | "figure") {
            self.state.caption(b).into()
        } else {
            self.state.text(&key, &b.zh, &b.en).into()
        }
    }
    fn restored_panel(session: Option<&Session>) -> Panel {
        match session.map(|s| s.panel.as_str()) {
            Some("edit") => Panel::Edit,
            Some("chat") => Panel::Chat,
            _ => Panel::Notes,
        }
    }
    fn load_session(workspace: Option<&Workspace>) -> (Option<Session>, Option<String>) {
        let Some(ws) = workspace else {
            return (None, None);
        };
        if !ws.root.join(".native-reader.json").is_file() {
            return (None, None);
        }
        match Session::load(&ws.root) {
            Ok(session) => (Some(session), None),
            Err(e) => (None, Some(format!("恢复文件读取失败，已保留原文件：{e:#}"))),
        }
    }
    fn restored_position(&self) -> (usize, f32) {
        let ids: Vec<_> = self
            .state
            .paper
            .blocks
            .iter()
            .map(|b| b.id.as_str())
            .collect();
        self.session
            .as_ref()
            .and_then(|s| s.position(&self.raw["reader"]["progress"], &ids))
            .unwrap_or((self.selected.min(ids.len().saturating_sub(1)), 0.0))
    }
    fn current_note(&self, cx: &Context<Self>) -> NoteDraft {
        let quote = self.quote.read(cx).value().to_string();
        let context = self
            .note_selection
            .as_ref()
            .filter(|s| s.quote == quote && s.block == self.selected);
        NoteDraft {
            body: self.note.read(cx).value().to_string(),
            key: context
                .map(|s| s.key.clone())
                .or_else(|| self.selected_key())
                .unwrap_or_default(),
            quote,
            kind: self.note_kind.into(),
            prefix: context.map(|s| s.prefix.clone()).unwrap_or_default(),
            suffix: context.map(|s| s.suffix.clone()).unwrap_or_default(),
        }
    }
    fn capture_inputs(&mut self, cx: &Context<Self>) {
        if self.restore_inputs {
            return;
        }
        let Some(key) = self.selected_key() else {
            return;
        };
        let text = self.editor.read(cx).value().to_string();
        let existing = self
            .session
            .as_ref()
            .and_then(|s| s.drafts.get(&key))
            .and_then(|d| d.edit.as_ref());
        let edit = (text != self.editor_base).then(|| EditDraft {
            text,
            base: existing
                .map(|d| d.base.clone())
                .unwrap_or_else(|| text_hash(&self.editor_base)),
        });
        let note = self.current_note(cx);
        let note = (!note.body.is_empty() || !note.quote.is_empty()).then_some(note);
        if let Some(session) = &mut self.session {
            session.set_draft(key, Draft { edit, note });
        }
    }
    fn reading_progress(&self) -> f32 {
        let count = self.state.paper.blocks.len();
        if count == 0 {
            return 0.;
        }
        if self.is_epub() && self.epub_reflow.complete {
            let pages = self.epub_reflow.pages.len();
            return match pages {
                0 => 0.,
                1 => 1.,
                _ => (self.epub_reflow.current as f32 / (pages - 1) as f32).clamp(0., 1.),
            };
        }
        let anchor = self.list.logical_scroll_top();
        if !self.is_epub() {
            let viewport = self.list.viewport_bounds();
            if viewport.size.height > px(0.)
                && self
                    .list
                    .bounds_for_item(count - 1)
                    .is_some_and(|last| last.bottom() <= viewport.bottom() + px(1.))
            {
                return 1.;
            }
        }
        // Unmeasured virtual-list rows have zero pixel height, so a ratio of
        // total scroll pixels would report completion long before the end.
        // Use the logical block anchor plus its measured intra-block offset.
        let fraction = self
            .list
            .bounds_for_item(anchor.item_ix)
            .filter(|bounds| bounds.size.height > px(0.))
            .map(|bounds| {
                (f32::from(anchor.offset_in_item) / f32::from(bounds.size.height)).clamp(0., 1.)
            })
            .unwrap_or(0.);
        ((anchor.item_ix as f32 + fraction) / count as f32).clamp(0., 1.)
    }

    fn reading_progress_bar(&self, cx: &Context<Self>) -> AnyElement {
        let reader = cx.entity().downgrade();
        div()
            .id("reading-progress")
            .debug_selector(|| "reading-progress".into())
            .w_full()
            .h(px(1.854))
            .flex_shrink_0()
            .bg(rgb(0xe3ddd2))
            .child(
                canvas(
                    |_, _, _| {},
                    move |bounds, _, window, cx| {
                        // Paint after the body has been measured, including
                        // EPUB reflow and restored variable-height anchors.
                        let progress = reader
                            .read_with(cx, |this, _| this.reading_progress())
                            .unwrap_or(0.);
                        if progress > 0. {
                            window.paint_quad(fill(
                                Bounds::new(
                                    bounds.origin,
                                    size(bounds.size.width * progress, bounds.size.height),
                                ),
                                rgb(0x387487),
                            ));
                        }
                    },
                )
                .size_full(),
            )
            .into_any_element()
    }

    fn capture_position(&mut self) {
        let offset = self.list.logical_scroll_top();
        let Some(block) = self.state.paper.blocks.get(offset.item_ix) else {
            return;
        };
        let selected_key = self.selected_key();
        if let Some(session) = &mut self.session {
            session.selected_key = selected_key;
            session.panel = match self.panel {
                Panel::Edit => "edit",
                Panel::Notes => "notes",
                Panel::Chat => "chat",
            }
            .into();
            if !session.progress.as_ref().is_some_and(|p| {
                p.block == block.id && p.offset == f32::from(offset.offset_in_item)
            }) {
                session.progress = Some(Position {
                    block: block.id.clone(),
                    offset: f32::from(offset.offset_in_item).max(0.0),
                    at: now(),
                });
            }
            session.bilingual = self.reading_mode == ReadingMode::Bilingual;
            session.reading_mode = Some(self.reading_mode);
            session.original_first = self.original_first;
            session.show_pdf = self.show_pdf;
            session.appearance = self.appearance.clone();
        }
    }
    fn flush_session(&mut self) -> bool {
        if self.settings.relocating {
            return false;
        }
        if self.session == self.persisted_session {
            return false;
        }
        if let (Some(ws), Some(session)) = (&self.workspace, &self.session) {
            match session.save(&ws.root) {
                Ok(()) => self.persisted_session = self.session.clone(),
                Err(e) => {
                    self.status = format!("草稿/位置保存失败：{e:#}");
                    return true;
                }
            }
        }
        false
    }
    fn fill_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.restore_inputs = false;
        self.editor_base = self.selected_text();
        let draft = self
            .selected_key()
            .and_then(|key| self.session.as_ref()?.drafts.get(&key))
            .cloned()
            .unwrap_or_default();
        self.draft_conflict = draft
            .edit
            .as_ref()
            .is_some_and(|d| d.base != text_hash(&self.editor_base));
        let text = draft
            .edit
            .map(|d| d.text)
            .unwrap_or_else(|| self.editor_base.clone());
        let note = draft.note.unwrap_or_default();
        self.note_kind = if note.kind == "question" {
            "question"
        } else {
            "note"
        };
        self.note_selection = if note.quote.is_empty() {
            None
        } else {
            self.selected_key().map(|key| Selection {
                key: if note.key.is_empty() {
                    key
                } else {
                    note.key.clone()
                },
                block: self.selected,
                quote: note.quote.clone(),
                prefix: note.prefix.clone(),
                suffix: note.suffix.clone(),
                position: point(px(0.), px(0.)),
                underline: false,
            })
        };
        self.editor
            .update(cx, |s, cx| s.set_value(text, window, cx));
        self.quote
            .update(cx, |s, cx| s.set_value(note.quote, window, cx));
        self.note
            .update(cx, |s, cx| s.set_value(note.body, window, cx));
    }
    fn fill_paper_note(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let body = self.raw["reader"]["paper_note"]["body"]
            .as_str()
            .unwrap_or("")
            .to_owned();
        self.paper_note_input
            .update(cx, |state, cx| state.set_value(body, window, cx));
    }
    fn select(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.cancel_annotation_scroll();
        self.queue_margin_note();
        self.margin_editor = None;
        self.capture_inputs(cx);
        self.flush_session();
        self.selection = None;
        self.annotation_popup = None;
        self.note_selection = None;
        self.note_kind = "note";
        self.selected = index;
        self.follow_block(index);
        self.fill_editor(window, cx);
        self.panel = Panel::Notes;
        cx.notify();
    }
    fn save_ops(&mut self, ops: Vec<Value>, cx: &mut Context<Self>) {
        self.save_ops_with_draft(ops, None, cx);
    }
    fn save_ops_with_draft(
        &mut self,
        ops: Vec<Value>,
        saved: Option<(String, Option<EditDraft>, Option<NoteDraft>, bool)>,
        cx: &mut Context<Self>,
    ) {
        let Some(ws) = self.workspace.clone() else {
            return;
        };
        self.status = "正在保存……".into();
        self.worker(cx, move |_, tx| {
            match ws.reader_ops(&ops).and_then(|_| ws.state()) {
                Ok(raw) => {
                    let committed = saved.is_none()
                        || easyread_gpui::session::draft_ops_committed(&raw["reader"], &ops);
                    let _ = tx.send(Event::Refresh(raw));
                    if !committed {
                        let _ = tx.send(Event::Error(
                            "正文已有更新，保存未生效；草稿已保留，请核对后重试".into(),
                        ));
                        return;
                    }
                    if let Some((key, edit, note, revert)) = saved {
                        let _ = tx.send(Event::SavedDraft(key, edit, note, revert));
                    }
                    let _ = tx.send(Event::Status("已保存".into()));
                }
                Err(e) => {
                    let _ = tx.send(Event::Error(format!("保存失败：{e:#}")));
                }
            }
        });
    }
    fn save_edit(&mut self, revert: bool, cx: &mut Context<Self>) {
        let Some(b) = self.state.paper.blocks.get(self.selected) else {
            return;
        };
        if !matches!(
            b.kind.as_str(),
            "para" | "heading" | "note" | "figure" | "table"
        ) {
            self.status = "当前可修订段落、标题或图表题注".into();
            return;
        }
        let key = self.selected_key().unwrap();
        let (zh, en) = if matches!(b.kind.as_str(), "figure" | "table") {
            (&b.caption_zh, &b.caption_en)
        } else {
            (&b.zh, &b.en)
        };
        let base = text_hash(if zh.is_empty() { en } else { zh });
        let text = self.editor.read(cx).value().to_string();
        self.capture_inputs(cx);
        let draft = EditDraft {
            text: text.clone(),
            base: self
                .session
                .as_ref()
                .and_then(|s| s.drafts.get(&key))
                .and_then(|d| d.edit.as_ref())
                .map(|d| d.base.clone())
                .unwrap_or_else(|| text_hash(&self.editor_base)),
        };
        self.save_ops_with_draft(vec![json!({"op":"edit","block":key,"zh":if revert{Value::Null}else{json!(text)},"base":base,"at":now()})], Some((key, Some(draft), None, revert)), cx);
    }
    fn compile_formulas(&mut self, cx: &mut Context<Self>) {
        let mut display = std::collections::HashSet::new();
        let mut inline = std::collections::HashSet::new();
        for b in &self.state.paper.blocks {
            if b.kind == "math" && !b.tex.is_empty() && !self.formulas.contains_key(&b.tex) {
                display.insert(b.tex.clone());
            }
            if b.kind != "math" {
                let text = self.state.markdown(b, true);
                for math in formula::math_spans(&text) {
                    if math.display {
                        if !self.formulas.contains_key(&math.tex) {
                            display.insert(math.tex);
                        }
                    } else if !self.inline_formulas.contains_key(&math.tex) {
                        inline.insert(math.tex);
                    }
                }
            }
        }
        if display.is_empty() && inline.is_empty() {
            return;
        }
        self.worker(cx, move |cancel, tx| {
            for tex in display {
                if cancel.load(Ordering::Relaxed) {
                    break;
                }
                let result = formula::layout(&tex, true).map_err(|e| format!("{e:#}"));
                let _ = tx.send(Event::Formula(tex, result));
            }
            for tex in inline {
                if cancel.load(Ordering::Relaxed) {
                    break;
                }
                let result = formula::layout(&tex, false).map_err(|e| format!("{e:#}"));
                let _ = tx.send(Event::InlineFormula(tex, result));
            }
            let _ = tx.send(Event::Status("公式已编译".into()));
        });
    }
    fn pick(&mut self, migrate: bool, cx: &mut Context<Self>) {
        let Some(lib) = self.library.clone() else {
            return;
        };
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: !migrate,
            directories: migrate,
            multiple: false,
            prompt: Some(
                if migrate {
                    "复制旧论文目录"
                } else {
                    "导入 PDF / EPUB"
                }
                .into(),
            ),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = receiver.await
                && let Some(path) = paths.first()
            {
                let path = path.clone();
                let _ = this.update(cx, |view, cx| {
                    view.status = "正在导入……".into();
                    view.worker(cx, move |cancel, tx| {
                        let result = if migrate {
                            lib.migrate_copy(&path)
                        } else {
                            lib.import_file_cancellable(&path, None, &cancel, |done, total| {
                                let _ = tx.send(Event::Status(format!("导入 {done}/{total}")));
                            })
                        };
                        match result.and_then(Self::opened_workspace) {
                            Ok((ws, raw)) => {
                                let _ = tx.send(Event::Loaded(ws, raw));
                            }
                            Err(e) => {
                                let _ = tx.send(Event::Error(format!("导入失败：{e:#}")));
                            }
                        }
                    });
                    view.home.cancellable_task = !migrate;
                });
            }
        })
        .detach();
    }
    fn open(&mut self, id: String, cx: &mut Context<Self>) {
        let Some(lib) = self.library.clone() else {
            return;
        };
        self.worker(cx, move |_, tx| {
            match lib.workspace(&id).and_then(Self::opened_workspace) {
                Ok((ws, raw)) => {
                    let _ = tx.send(Event::Loaded(ws, raw));
                }
                Err(e) => {
                    let _ = tx.send(Event::Error(format!("打开失败：{e:#}")));
                }
            }
        });
    }
    fn opened_workspace(ws: Workspace) -> anyhow::Result<(Workspace, Value)> {
        let mut raw = ws.validated_state()?;
        raw["item"] = ws.update("item", |item| {
            item["opened"] = json!(now());
            if item["status"] != "done" {
                item["status"] = json!("reading");
            }
            Ok(())
        })?;
        Ok((ws, raw))
    }
    fn translate(&mut self, cx: &mut Context<Self>) {
        if self.busy || self.workspace.is_none() {
            return;
        }
        let pages = self.state.paper.meta.pages.len().max(
            self.state
                .paper
                .blocks
                .iter()
                .map(|b| b.page)
                .max()
                .unwrap_or(0),
        );
        if pages > self.settings.committed.ask_above || self.settings.committed.ask_above == 0 {
            self.settings.translation_confirm = true;
            cx.notify();
            return;
        }
        self.start_translate(cx);
    }
    fn start_translate(&mut self, cx: &mut Context<Self>) {
        let Some(ws) = self.workspace.clone() else {
            return;
        };
        let config = match self.settings.committed.connection(true) {
            Ok(c) => c,
            Err(e) => {
                self.status = e.to_string();
                return;
            }
        };
        let options = ai::TranslationOptions {
            pages_per_batch: self.settings.committed.pages_per_batch,
            parallel: self.settings.committed.parallel,
        };
        self.status = "正在翻译……".into();
        self.worker(cx, move |cancel, tx| {
            let outcome = ai::translate_batched(&ws, &config, options, &cancel, |done, total| {
                let _ = tx.send(Event::Status(format!("翻译 {done}/{total}")));
            });
            if let Ok(raw) = ws.state() {
                let _ = tx.send(Event::Refresh(raw));
            }
            let _ = tx.send(match outcome {
                Ok(_) => Event::Status("翻译已保存".into()),
                Err(e) => Event::Error(format!("翻译中断：{e:#}")),
            });
        });
        self.home.cancellable_task = true;
    }
    fn capture_selection(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        if self.inline_edit.is_some() {
            return;
        }
        let selected: Vec<_> = self
            .text_views
            .iter()
            .filter_map(|((index, original), (_, view))| {
                if !self.rendered_languages(*index).contains(original) {
                    return None;
                }
                let state = view.read(cx);
                // The HTML renderer appends a paragraph separator even for
                // a partial selection; it isn't part of the user's quote.
                let quote = state.selected_text().trim_end_matches('\n').to_owned();
                (!state.is_selecting() && !quote.trim().is_empty())
                    .then_some((*index, *original, quote))
            })
            .collect();
        self.selection = None;
        // Like the Python reader, annotations belong to one paragraph/item.
        if let [(index, original, quote)] = selected.as_slice() {
            let b = &self.state.paper.blocks[*index];
            let text = |key: &str, zh: &str, en: &str| {
                if *original {
                    if b.source_kind == "pdf-text" {
                        easyread_gpui::model::pdf_prose(en)
                    } else {
                        en.to_owned()
                    }
                } else {
                    let text = self.state.text(key, zh, en);
                    if b.source_kind == "pdf-text" && text == en {
                        easyread_gpui::model::pdf_prose(text)
                    } else {
                        text.to_owned()
                    }
                }
            };
            let sources: Vec<(String, String)> = match b.kind.as_str() {
                "para" | "heading" | "note" => vec![(b.id.clone(), text(&b.id, &b.zh, &b.en))],
                "figure" | "table" => {
                    let key = format!("{}#caption", b.id);
                    vec![(key.clone(), text(&key, &b.caption_zh, &b.caption_en))]
                }
                "list" => b
                    .items
                    .iter()
                    .enumerate()
                    .map(|(i, item)| {
                        let key = format!("{}#{i}", b.id);
                        (key.clone(), text(&key, &item.zh, &item.en))
                    })
                    .collect(),
                _ => vec![],
            };
            for (key, source) in sources {
                // Recovered PDF formulas are atomic TeX nodes in the rendered
                // text, not the separate glyphs in the stored extraction. Use
                // the same undecorated HTML as the selectable TextView, so
                // quotes and annotation contexts share its representation.
                // Lists still anchor each item to its own persisted key.
                let html = if b.kind == "list" {
                    formula::native_markdown_html(&source)
                } else {
                    self.reader_layout_html(*index, *original)
                };
                let text = annotations::plain_text(&html);
                let native = self
                    .active_reading_paragraph
                    .filter(|(block, language, _)| block == index && language == original)
                    .and_then(|(_, _, paragraph)| {
                        self.text_views
                            .get(&(*index, *original))?
                            .1
                            .read(cx)
                            .paragraph_text_context(paragraph)
                    });
                let range = if let Some((native_text, paragraph)) = native {
                    annotations::rendered_paragraph_quote(
                        &text,
                        &native_text,
                        paragraph,
                        Some(quote),
                    )
                    .map(|(_, range)| range)
                } else {
                    annotations::quote_range(&text, quote, "", "")
                };
                if let Some(range) = range {
                    let (prefix, suffix) = annotations::context(&text, range);
                    self.selection = Some(Selection {
                        block: *index,
                        key,
                        quote: quote.clone(),
                        prefix,
                        suffix,
                        position,
                        underline: false,
                    });
                    break;
                }
            }
        }
        // A single click also opens the paragraph toolbar. Text-range
        // selections take precedence, and stale/hidden views never hit-test.
        if selected.is_empty()
            && self.selection.is_none()
            && let Some((index, original, paragraph)) = self.active_reading_paragraph
        {
            let hit = self.rendered_languages(index).contains(&original)
                && self
                    .text_views
                    .get(&(index, original))
                    .is_some_and(|(_, view)| {
                        view.read(cx).paragraph_at(position) == Some(paragraph)
                    });
            let block = &self.state.paper.blocks[index];
            if hit
                && matches!(
                    block.kind.as_str(),
                    "para" | "heading" | "note" | "figure" | "table"
                )
            {
                let text = annotations::plain_text(&self.reader_layout_html(index, original));
                let scoped = self
                    .text_views
                    .get(&(index, original))
                    .and_then(|(_, view)| view.read(cx).paragraph_text_context(paragraph))
                    .and_then(|(native_text, range)| {
                        annotations::rendered_paragraph_quote(&text, &native_text, range, None)
                    });
                if let Some((quote, range)) = scoped {
                    let (prefix, suffix) = annotations::context(&text, range);
                    self.selection = Some(Selection {
                        block: index,
                        key: if matches!(block.kind.as_str(), "figure" | "table") {
                            format!("{}#caption", block.id)
                        } else {
                            block.id.clone()
                        },
                        quote,
                        prefix,
                        suffix,
                        position,
                        underline: false,
                    });
                }
            }
        }
        if let Some(selection) = &self.selection {
            self.annotation_popup = None;
            self.follow_block(selection.block);
        }
        cx.notify();
    }

    fn selection_action(
        &mut self,
        action: &'static str,
        color: HighlightColor,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if (action == "chat" && !self.settings.committed.features.ask_ai)
            || (action == "original" && !self.settings.committed.features.original)
            || (action == "edit" && !self.settings.committed.features.edit)
            || (action == "page" && !self.settings.committed.features.page)
        {
            return;
        }
        let Some(selection) = self.selection.take() else {
            return;
        };
        TextSelection::clear(window, cx);
        if action == "page" {
            self.locate_source(selection.block, cx);
        } else if action == "edit" {
            self.begin_inline_edit(selection.block, window, cx);
        } else if action == "copy" {
            cx.write_to_clipboard(ClipboardItem::new_string(selection.quote));
            self.status = "已复制".into();
        } else if action == "original" {
            self.toggle_block_language(selection.block, cx);
        } else {
            let annotation = self.annotation_popup.clone();
            self.select(selection.block, window, cx);
            if action == "highlight" || action == "underline" {
                let b = &self.state.paper.blocks[selection.block];
                let stamp = now();
                let mut note = json!({"id":format!("n{}",Uuid::new_v4().simple()),"anchor":b.id,"key":selection.key,
                    "quote":selection.quote,"prefix":selection.prefix,"suffix":selection.suffix,
                    "kind":"highlight","color":color.name(),"body":"","created":stamp,"updated":stamp});
                if action == "underline" {
                    note["style"] = json!("underline");
                }
                self.save_ops(vec![json!({"op":"note","note":note})], cx);
            } else if action == "chat" {
                self.add_chat_reference(ai::ChatReference {
                    anchor: self.state.paper.blocks[selection.block].id.clone(),
                    quote: selection.quote,
                });
                self.open_chat(window, cx);
            } else {
                self.annotation_popup = annotation;
                self.open_margin_note(selection, action, color, window, cx);
            }
        }
        cx.notify();
    }

    fn queue_margin_note(&mut self) {
        if self.settings.relocating {
            return;
        }
        if let (Some(editor), Some(workspace)) = (&mut self.margin_editor, &self.workspace)
            && let Some(snapshot) = editor.note.pending()
            && self.margin_writer.save(workspace.clone(), snapshot.clone())
        {
            self.margin_records.insert(
                snapshot["id"].as_str().unwrap().to_owned(),
                snapshot.clone(),
            );
            editor.note.queued(snapshot);
        }
    }

    fn receive_margin_notes(&mut self, cx: &mut Context<Self>) {
        while let Ok(saved) = self.margin_writer.results.try_recv() {
            if self
                .workspace
                .as_ref()
                .is_none_or(|ws| ws.root != saved.root)
            {
                continue;
            }
            let id = saved.snapshot["id"].as_str().unwrap_or("");
            if saved.result.is_ok() && self.margin_records.get(id) == Some(&saved.snapshot) {
                self.margin_records.remove(id);
            }
            if let Some(editor) = &mut self.margin_editor
                && editor.note.record["id"] == saved.snapshot["id"]
            {
                editor
                    .note
                    .acknowledge(saved.snapshot, saved.result.is_ok());
            }
            match saved.result {
                Ok(reader) => {
                    let mut raw = self.raw.clone();
                    raw["reader"] = reader;
                    for (id, record) in &self.margin_records {
                        if raw["reader"]["notes"][id]["updated"].as_str().unwrap_or("")
                            < record["updated"].as_str().unwrap_or("")
                        {
                            raw["reader"]["notes"][id] = record.clone();
                        }
                    }
                    self.apply_state(raw);
                    self.status = "笔记已自动保存".into();
                }
                Err(error) => self.status = error,
            }
            cx.notify();
        }
    }

    fn open_margin_note(
        &mut self,
        selection: Selection,
        kind: &'static str,
        color: HighlightColor,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.queue_margin_note();
        let anchor = &self.state.paper.blocks[selection.block].id;
        let existing = self.raw["reader"]["notes"].as_object().and_then(|notes| {
            notes
                .values()
                .chain(self.margin_records.values())
                .filter(|note| {
                    note["anchor"] == *anchor
                        && note["key"] == selection.key
                        && note["quote"] == selection.quote
                        && note["prefix"] == selection.prefix
                        && !note["deleted"].as_bool().unwrap_or(false)
                        && matches!(note["kind"].as_str(), Some("note" | "question"))
                })
                .max_by_key(|note| note["updated"].as_str().unwrap_or(""))
        });
        let stamp = now();
        let mut record = existing.cloned().unwrap_or_else(|| {
            json!({
                "id":format!("n{}",Uuid::new_v4().simple()), "anchor":anchor, "key":selection.key,
                "quote":selection.quote, "prefix":selection.prefix, "suffix":selection.suffix,
                "kind":kind, "color":color.name(), "body":"", "style":"underline",
                "created":stamp, "updated":stamp
            })
        });
        record["kind"] = json!(kind);
        record["updated"] = json!(stamp);
        // Existing marks remain independent of the attached note.
        if let Some((id, _)) = &self.annotation_popup {
            record["source_annotation"] = json!(id);
            record["decoration"] = json!("none");
            record["color"] = json!(color.name());
            record.as_object_mut().unwrap().remove("style");
        }
        self.annotation_popup = None;
        self.panel = Panel::Notes;
        self.panel_open = false;
        self.show_pdf = false;
        self.margin_input.update(cx, |state, cx| {
            state.set_value(record["body"].as_str().unwrap_or("").to_owned(), window, cx);
        });
        let mut note = MarginNote::new(record);
        if let Some(existing) = existing {
            note.queued(existing.clone());
            note.acknowledge(existing.clone(), true);
        }
        self.margin_editor = Some(MarginEditor {
            selection,
            note,
            changed_at: Instant::now(),
            placement_pending: Some(Instant::now()),
        });
        self.reset_rows();
        self.place_margin_editor(window, cx);
        cx.notify();
    }

    fn place_margin_editor(&self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(token) = self
            .margin_editor
            .as_ref()
            .and_then(|editor| editor.placement_pending)
        else {
            return;
        };
        let weak = cx.entity().downgrade();
        // The first render adds the gutter and quote anchor. Inline annotation
        // offsets are published during paint, not while block() builds the row.
        // Rebuild the cached row only after that frame has finished, so its
        // first visible placement uses the actual, newly reflowed quote line.
        window.on_next_frame(move |window, _| {
            window.on_next_frame(move |window, cx| {
                let _ = weak.update(cx, |this, cx| {
                    let Some(editor) = this
                        .margin_editor
                        .as_mut()
                        .filter(|editor| editor.placement_pending == Some(token))
                    else {
                        return;
                    };
                    let index = editor.selection.block;
                    if this.pdf_code_pending(index)
                        || [false, true].into_iter().any(|original| {
                            this.text_views
                                .get(&(index, original))
                                .is_some_and(|(_, view)| !view.read(cx).is_content_ready())
                        })
                    {
                        this.place_margin_editor(window, cx);
                        return;
                    }
                    this.margin_editor.as_mut().unwrap().placement_pending = None;
                    this.reset_rows();
                    this.margin_input
                        .read(cx)
                        .focus_handle(cx)
                        .focus(window, cx);
                    cx.notify();
                });
            });
        });
    }

    fn close_margin_note(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.queue_margin_note();
        self.margin_editor = None;
        self.hovered_note_card = None;
        self.reset_rows();
        window.blur(cx);
        cx.notify();
    }

    fn change_margin_note(
        &mut self,
        kind: Option<&'static str>,
        color: Option<HighlightColor>,
        cx: &mut Context<Self>,
    ) {
        if let Some(editor) = &mut self.margin_editor {
            if let Some(kind) = kind {
                editor.note.record["kind"] = json!(kind);
            }
            if let Some(color) = color {
                editor.note.record["color"] = json!(color.name());
            }
            editor.note.record["updated"] = json!(now());
            editor.changed_at = Instant::now();
        }
        // Margin editors live inside cached list rows; refresh their swatch
        // and quote border immediately, keeping the current reading offset.
        self.reset_rows();
        cx.notify();
    }

    fn margin_note_view(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let editor = self.margin_editor.as_ref()?;
        let color =
            HighlightColor::from_name(editor.note.record["color"].as_str().unwrap_or("yellow"));
        let question = editor.note.record["kind"] == "question";
        Some(
            div()
                .w(px(300.))
                .flex_shrink_0()
                .px(px(12.))
                .child(
                    div()
                        .id("margin-note")
                        .debug_selector(|| "margin-note".into())
                        .when(editor.placement_pending.is_some(), |d| d.invisible())
                        .border_l_2()
                        .border_color(rgb(0xaa681c))
                        .pl(px(12.))
                        .flex()
                        .flex_col()
                        .gap(px(6.))
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(self.annotation_header(&editor.note.record))
                        .child(
                            div()
                                .border_l_2()
                                .border_color(rgb(color.rgb()))
                                .pl(px(4.))
                                .text_size(px(13.))
                                .text_color(rgb(0x969084))
                                .max_h(px(78.))
                                .overflow_hidden()
                                .child(format!("“{}”", editor.selection.quote)),
                        )
                        .child(
                            div()
                                .debug_selector(|| "margin-note-input".into())
                                .border_1()
                                .border_color(rgb(0xaa681c))
                                .rounded(px(8.))
                                .bg(rgb(0xfbfaf7))
                                .p(px(6.))
                                .child(
                                    Textarea::new(&self.margin_input)
                                        .appearance(false)
                                        .bordered(false)
                                        .h(px(110.))
                                        .text_size(px(14.))
                                        .aria_label("My note text"),
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(3.))
                                .children([("note", "Notes"), ("question", "Question")].map(
                                    |(kind, label)| {
                                        Button::new(SharedString::from(format!("margin-{kind}")))
                                            .ghost()
                                            .h(px(24.))
                                            .px(px(6.))
                                            .rounded(px(12.))
                                            .text_size(px(11.))
                                            .text_color(rgb(if (kind == "question") == question {
                                                0xaa681c
                                            } else {
                                                0x969084
                                            }))
                                            .when((kind == "question") == question, |b| {
                                                b.bg(rgb(0xeee6da))
                                            })
                                            .label(label)
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.change_margin_note(Some(kind), None, cx);
                                            }))
                                    },
                                ))
                                .children(HighlightColor::ALL.map(|swatch| {
                                    Button::new(("margin-color", swatch as usize))
                                        .ghost()
                                        .p_0()
                                        .size(px(20.))
                                        .rounded(px(10.))
                                        .accessibility_label(swatch.label())
                                        .tooltip(swatch.label())
                                        .child(
                                            div()
                                                .size(px(20.))
                                                .rounded(px(10.))
                                                .border_1()
                                                .border_color(
                                                    rgb(if swatch == color {
                                                        0x928b7d
                                                    } else {
                                                        0x000000
                                                    })
                                                    .opacity(if swatch == color { 1. } else { 0. }),
                                                )
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .child(
                                                    div()
                                                        .size(px(14.))
                                                        .rounded(px(7.))
                                                        .border_1()
                                                        .border_color(rgba(0x928b7d55))
                                                        .bg(rgb(swatch.rgb())),
                                                ),
                                        )
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.change_margin_note(None, Some(swatch), cx);
                                        }))
                                })),
                        )
                        .when(question, |d| {
                            let id = editor.note.record["id"].as_str().unwrap_or("").to_owned();
                            d.child(
                                Button::new("margin-question-status")
                                    .debug_selector(|| "margin-question-status".into())
                                    .ghost()
                                    .xsmall()
                                    .label(if annotation_panel::answered(&editor.note.record) {
                                        "Mark as unanswered"
                                    } else {
                                        "Mark as answered"
                                    })
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.toggle_question_answered(&id, cx);
                                    })),
                            )
                        })
                        .child(
                            div()
                                .text_size(px(11.))
                                .text_color(rgb(0x969084))
                                .text_right()
                                .child(if editor.note.is_saved() {
                                    "Autosaved · Esc to close"
                                } else if editor.note.record["body"]
                                    .as_str()
                                    .unwrap_or("")
                                    .is_empty()
                                {
                                    "Autosave · Esc to close"
                                } else {
                                    "Autosaving… · Esc to close"
                                }),
                        ),
                )
                .into_any_element(),
        )
    }

    fn selection_toolbar(&self, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let selection = self.selection.as_ref()?;
        let underline = selection.underline;
        let can_toggle_original = self.selection_can_toggle_language(selection.block);
        let showing_original = self.selection_secondary_visible(selection.block);
        let secondary_original = !self.primary_original();
        let viewport = window.viewport_size();
        let source_label = format!(
            "{} p.{}",
            if self.is_epub() { "EPUB" } else { "PDF" },
            self.source_page_number(selection.block)
                .map(|page| page.to_string())
                .unwrap_or_else(|| "…".into()),
        );
        let features = &self.settings.committed.features;
        // Match the sum of the button groups, separators and outer padding.
        // Hidden actions must not leave empty space behind in either layout.
        let toolbar_width = |compact: bool| {
            let button = if compact { 24. } else { 28. };
            let gap = if compact { 0. } else { 2. };
            let group_width = |widths: &[f32]| {
                let count = widths.iter().filter(|&&width| width > 0.).count();
                widths.iter().sum::<f32>() + gap * count.saturating_sub(1) as f32
            };
            let notes = group_width(&[
                if compact { button } else { 60. },
                if compact { button } else { 82. },
                if features.ask_ai {
                    if compact { button } else { 70. }
                } else {
                    0.
                },
            ]);
            let tools = group_width(&[
                if features.original {
                    if compact { button } else { 80. }
                } else {
                    0.
                },
                if features.edit {
                    if compact { button } else { 76. }
                } else {
                    0.
                },
                if features.page {
                    if compact { button } else { 90. }
                } else {
                    0.
                },
                button,
            ]);
            14. // Outer padding and border.
                + button * 2. + 4. // Mark style group.
                + 4. + 4. * if compact { 20. } else { 22. } // Color group.
                + 2. * if compact { 7. } else { 9. } // Separators.
                + notes + tools
        };
        let full_width = toolbar_width(false);
        let compact = viewport.width < px(full_width + 16.);
        let width = px(toolbar_width(compact)).min((viewport.width - px(16.)).max(px(1.)));
        let left = (selection.position.x - width / 2.)
            .max(px(8.))
            .min((viewport.width - width - px(8.)).max(px(8.)));
        let top = if selection.position.y > px(76.) {
            selection.position.y - px(54.)
        } else {
            selection.position.y + px(16.)
        };
        let locked = self.busy || self.workspace.is_none();
        let icon_size = if compact { 15. } else { 16. };
        let button_size = if compact { 24. } else { 28. };
        let action_button =
            |action: &'static str, label: &'static str, icon: &'static str, full_width: f32| {
                let disabled = match action {
                    "highlight" | "underline" | "note" | "question" => locked,
                    "original" => !can_toggle_original,
                    "edit" => self.busy,
                    _ => false,
                };
                Button::new(SharedString::from(format!("selection-{action}")))
                    .ghost()
                    .accessibility_label(label)
                    .rounded(px(8.))
                    .w(px(if compact { button_size } else { full_width }))
                    .h(px(button_size))
                    .p_0()
                    .text_color(rgb(0x5f5a51))
                    .when(matches!(action, "highlight" | "underline"), |b| {
                        if (action == "underline") == underline {
                            b.bg(rgb(0xfbfaf7)).shadow(vec![
                                BoxShadow::new(px(0.), px(1.), rgba(0x52452c12).into())
                                    .blur_radius(px(3.)),
                            ])
                        } else {
                            b.text_color(rgb(0x928b7d))
                        }
                    })
                    .when(disabled, |b| b.opacity(0.4))
                    .disabled(disabled)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(4.))
                            .text_size(px(13.))
                            .child(Icon::default().path(icon).size(px(icon_size)))
                            .when(
                                !compact && !matches!(action, "highlight" | "underline" | "copy"),
                                |d| d.child(label),
                            ),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if matches!(action, "highlight" | "underline") {
                            if let Some(selection) = this.selection.as_mut() {
                                selection.underline = action == "underline";
                            }
                            cx.notify();
                        } else {
                            this.selection_action(action, HighlightColor::Yellow, window, cx)
                        }
                    }))
            };
        let separator = || {
            div()
                .flex_shrink_0()
                .w(px(1.))
                .h(px(if compact { 18. } else { 22. }))
                .mx(px(if compact { 3. } else { 4. }))
                .bg(rgb(0xe3ddcf))
        };
        Some(
            deferred(
                div()
                    .id("selection-toolbar")
                    .debug_selector(|| "selection-toolbar".into())
                    .absolute()
                    .left(left)
                    .top(top.max(px(8.)).min((viewport.height - px(56.)).max(px(8.))))
                    .w(width)
                    .h(px(if compact { 40. } else { 42. }))
                    .px(px(6.))
                    .flex()
                    .items_center()
                    .bg(rgb(0xfbfaf7))
                    .border_1()
                    .border_color(rgb(0xe2dccf))
                    .rounded(px(14.))
                    .shadow(vec![
                        BoxShadow::new(px(0.), px(3.), rgba(0x52452c16).into())
                            .blur_radius(px(12.)),
                    ])
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .flex()
                            .flex_shrink_0()
                            .items_center()
                            .p(px(2.))
                            .rounded(px(10.))
                            .bg(rgb(0xeee9df))
                            .child(action_button(
                                "highlight",
                                "高亮",
                                "icons/highlighter.svg",
                                button_size,
                            ))
                            .child(action_button(
                                "underline",
                                "下划线",
                                "icons/underline.svg",
                                button_size,
                            )),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_shrink_0()
                            .items_center()
                            .ml(px(4.))
                            .children(HighlightColor::ALL.map(|color| {
                                let label = format!(
                                    "{}{}",
                                    color.label(),
                                    if underline { "下划线" } else { "高亮" }
                                );
                                Button::new(("selection-color", color as usize))
                                    .ghost()
                                    .accessibility_label(label)
                                    .w(px(if compact { 20. } else { 22. }))
                                    .h(px(button_size))
                                    .p_0()
                                    .rounded(px(8.))
                                    .disabled(locked)
                                    .when(locked, |b| b.opacity(0.4))
                                    .child(
                                        div()
                                            .size(px(if compact { 15. } else { 18. }))
                                            .rounded(px(if underline { 2. } else { 20. }))
                                            .bg(rgb(if underline {
                                                color.line_rgb()
                                            } else {
                                                match color {
                                                    HighlightColor::Yellow => 0xf7e6bd,
                                                    HighlightColor::Green => 0xd4e4cd,
                                                    HighlightColor::Blue => 0xd2e0ef,
                                                    HighlightColor::Red => 0xf6d4d1,
                                                }
                                            }))
                                            .border_1()
                                            .border_color(rgba(0x6a604b38)),
                                    )
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.selection_action(
                                            if underline { "underline" } else { "highlight" },
                                            color,
                                            window,
                                            cx,
                                        )
                                    }))
                            })),
                    )
                    .child(separator())
                    .child(
                        div()
                            .flex()
                            .flex_shrink_0()
                            .items_center()
                            .gap(px(if compact { 0. } else { 2. }))
                            .child(action_button(
                                "note",
                                "Notes",
                                "icons/message-square.svg",
                                60.,
                            ))
                            .child(action_button(
                                "question",
                                "Question",
                                "icons/circle-question-mark.svg",
                                82.,
                            ))
                            .when(self.settings.committed.features.ask_ai, |d| {
                                d.child(action_button("chat", "Ask AI", "icons/asterisk.svg", 70.))
                            }),
                    )
                    .child(separator())
                    .child(
                        div()
                            .flex()
                            .flex_shrink_0()
                            .items_center()
                            .gap(px(if compact { 0. } else { 2. }))
                            .when(self.settings.committed.features.original, |d| {
                                d.child(
                                    div()
                                        .debug_selector(|| "selection-language-toggle".into())
                                        .child(
                                            Button::new("selection-original")
                                                .ghost()
                                                .accessibility_label(
                                                    if self.selection_is_display_equation(
                                                        selection.block,
                                                    ) {
                                                        "独立公式无需翻译"
                                                    } else {
                                                        match (secondary_original, showing_original)
                                                        {
                                                            (true, true) => "收起原文",
                                                            (true, false) => "显示原文",
                                                            (false, true) => "收起译文",
                                                            (false, false) => "显示译文",
                                                        }
                                                    },
                                                )
                                                .rounded(px(8.))
                                                .w(px(if compact { button_size } else { 80. }))
                                                .h(px(button_size))
                                                .p_0()
                                                .text_color(rgb(0x5f5a51))
                                                .disabled(!can_toggle_original)
                                                .when(!can_toggle_original, |b| b.opacity(0.4))
                                                .child(
                                                    div()
                                                        .flex()
                                                        .items_center()
                                                        .gap(px(4.))
                                                        .text_size(px(13.))
                                                        .child(div().text_size(px(12.)).child("En"))
                                                        .when(!compact, |d| d.child("/ 译文")),
                                                )
                                                .on_click(cx.listener(|this, _, window, cx| {
                                                    this.selection_action(
                                                        "original",
                                                        HighlightColor::Yellow,
                                                        window,
                                                        cx,
                                                    )
                                                })),
                                        ),
                                )
                            })
                            .when(self.settings.committed.features.edit, |d| {
                                d.child(
                                    div()
                                        .debug_selector(|| "selection-edit-translation".into())
                                        .child(action_button(
                                            "edit",
                                            "改译文",
                                            "icons/pencil.svg",
                                            76.,
                                        )),
                                )
                            })
                            .when(self.settings.committed.features.page, |d| {
                                d.child(
                                    div()
                                        .debug_selector(|| "selection-source-page".into())
                                        .child(
                                            Button::new("selection-page")
                                                .ghost()
                                                .accessibility_label(source_label.clone())
                                                .rounded(px(8.))
                                                .w(px(if compact { button_size } else { 90. }))
                                                .h(px(button_size))
                                                .p_0()
                                                .text_color(rgb(0x5f5a51))
                                                .disabled(!self.can_locate_source(selection.block))
                                                .child(
                                                    div()
                                                        .flex()
                                                        .items_center()
                                                        .gap(px(4.))
                                                        .text_size(px(13.))
                                                        .child(
                                                            Icon::default()
                                                                .path("icons/file.svg")
                                                                .size(px(icon_size)),
                                                        )
                                                        .when(!compact, |d| d.child(source_label)),
                                                )
                                                .on_click(cx.listener(|this, _, window, cx| {
                                                    this.selection_action(
                                                        "page",
                                                        HighlightColor::Yellow,
                                                        window,
                                                        cx,
                                                    )
                                                })),
                                        ),
                                )
                            })
                            .child(action_button("copy", "复制", "icons/copy.svg", button_size)),
                    ),
            )
            .with_priority(10)
            .into_any_element(),
        )
    }

    fn edit_annotation(
        &mut self,
        action: &'static str,
        color: HighlightColor,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((id, position)) = self.annotation_popup.clone() else {
            return;
        };
        let mut note = self.raw["reader"]["notes"][&id].clone();
        if !note.is_object() || note["deleted"].as_bool().unwrap_or(false) {
            return;
        }
        if action == "remove" {
            self.annotation_popup = None;
            self.save_ops(vec![json!({"op":"note_del","id":id,"at":now()})], cx);
        } else if matches!(action, "note" | "question") {
            let Some(block) = self
                .state
                .paper
                .blocks
                .iter()
                .position(|b| b.id == note["anchor"])
            else {
                return;
            };
            self.selection = Some(Selection {
                block,
                key: note["key"]
                    .as_str()
                    .unwrap_or(&self.state.paper.blocks[block].id)
                    .to_owned(),
                quote: note["quote"].as_str().unwrap_or("").to_owned(),
                prefix: note["prefix"].as_str().unwrap_or("").to_owned(),
                suffix: note["suffix"].as_str().unwrap_or("").to_owned(),
                position,
                underline: note["style"] == "underline",
            });
            self.selection_action(action, color, window, cx);
        } else {
            if action == "color" {
                note["color"] = json!(color.name());
            } else if action == "underline" {
                note["style"] = json!("underline");
            } else {
                note.as_object_mut().unwrap().remove("style");
            }
            note["updated"] = json!(now());
            self.save_ops(vec![json!({"op":"note","note":note})], cx);
        }
        cx.notify();
    }

    fn annotation_editor(&self, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (id, position) = self.annotation_popup.as_ref()?;
        let note = &self.raw["reader"]["notes"][id];
        if !note.is_object() || note["deleted"].as_bool().unwrap_or(false) {
            return None;
        }
        let underline = note["style"] == "underline";
        let color = HighlightColor::from_name(note["color"].as_str().unwrap_or("yellow"));
        let locked = self.busy || self.workspace.is_none();
        let viewport = window.viewport_size();
        let width = px(360.).min(viewport.width - px(16.));
        let left = (position.x - width / 2.)
            .max(px(8.))
            .min((viewport.width - width - px(8.)).max(px(8.)));
        let top = if position.y > px(166.) {
            position.y - px(156.)
        } else {
            position.y + px(18.)
        };
        let mode_button = |action: &'static str, label: &'static str, icon: &'static str| {
            Button::new(SharedString::from(format!("annotation-{action}")))
                .ghost()
                .w(px(30.))
                .h(px(30.))
                .p_0()
                .rounded(px(8.))
                .accessibility_label(label)
                .tooltip(label)
                .disabled(locked)
                .text_color(rgb(0x928b7d))
                .when((action == "underline") == underline, |b| {
                    b.bg(rgb(0xfbfaf7)).text_color(rgb(0x5f5a51)).shadow(vec![
                        BoxShadow::new(px(0.), px(1.), rgba(0x52452c12).into()).blur_radius(px(3.)),
                    ])
                })
                .child(Icon::default().path(icon).size(px(17.)))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.edit_annotation(action, color, window, cx)
                }))
        };
        Some(
            deferred(
                div()
                    .id("annotation-editor")
                    .absolute()
                    .left(left)
                    .top(
                        top.max(px(8.))
                            .min((viewport.height - px(152.)).max(px(8.))),
                    )
                    .w(width)
                    .p(px(14.))
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .bg(rgb(0xfbfaf7))
                    .border_1()
                    .border_color(rgb(0xe2dccf))
                    .rounded(px(14.))
                    .shadow(vec![
                        BoxShadow::new(px(0.), px(3.), rgba(0x52452c16).into())
                            .blur_radius(px(12.)),
                    ])
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .text_size(px(13.))
                            .text_color(rgb(0x928b7d))
                            .child("My highlight"),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(
                                div()
                                    .flex()
                                    .p(px(2.))
                                    .bg(rgb(0xeee9df))
                                    .rounded(px(10.))
                                    .child(mode_button(
                                        "highlight",
                                        "改为高亮",
                                        "icons/highlighter.svg",
                                    ))
                                    .child(mode_button(
                                        "underline",
                                        "改为下划线",
                                        "icons/underline.svg",
                                    )),
                            )
                            .child(div().flex().items_center().gap(px(2.)).children(
                                HighlightColor::ALL.map(|choice| {
                                    let label = format!("改为{}", choice.label());
                                    Button::new(("annotation-color", choice as usize))
                                        .ghost()
                                        .size(px(26.))
                                        .p_0()
                                        .accessibility_label(label.clone())
                                        .tooltip(label)
                                        .disabled(locked)
                                        .child(
                                            div()
                                                .w(px(26.))
                                                .h(px(if underline { 14. } else { 26. }))
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .rounded(px(if underline { 6. } else { 20. }))
                                                .border_2()
                                                .border_color(if choice == color {
                                                    rgb(0x928b7d)
                                                } else {
                                                    rgba(0x00000000)
                                                })
                                                .child(
                                                    div()
                                                        .w(px(18.))
                                                        .h(px(if underline { 5. } else { 18. }))
                                                        .rounded(px(if underline {
                                                            2.
                                                        } else {
                                                            20.
                                                        }))
                                                        .bg(rgb(if underline {
                                                            choice.line_rgb()
                                                        } else {
                                                            choice.rgb()
                                                        }))
                                                        .border_1()
                                                        .border_color(rgba(0x6a604b38)),
                                                ),
                                        )
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.edit_annotation("color", choice, window, cx)
                                        }))
                                }),
                            )),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .children(
                                [("note", "Add note", 78.), ("question", "Question", 84.)].map(
                                    |(action, label, width)| {
                                        Button::new(SharedString::from(format!(
                                            "annotation-{action}"
                                        )))
                                        .ghost()
                                        .w(px(width))
                                        .h(px(30.))
                                        .p_0()
                                        .border_1()
                                        .border_color(rgb(0xd4cbbb))
                                        .rounded(px(8.))
                                        .text_color(rgb(0x5f5a51))
                                        .text_size(px(14.))
                                        .label(label)
                                        .disabled(locked)
                                        .on_click(
                                            cx.listener(move |this, _, window, cx| {
                                                this.edit_annotation(action, color, window, cx)
                                            }),
                                        )
                                    },
                                ),
                            )
                            .child(
                                Button::new("annotation-remove")
                                    .ghost()
                                    .p_0()
                                    .w(px(140.))
                                    .h(px(30.))
                                    .text_size(px(14.))
                                    .text_color(rgb(0xc54129))
                                    .label("Remove highlight")
                                    .disabled(locked)
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.edit_annotation("remove", color, window, cx)
                                    })),
                            ),
                    ),
            )
            .with_priority(10)
            .into_any_element(),
        )
    }

    fn complete_images(&mut self, cx: &mut Context<Self>) {
        let Some(ws) = self.workspace.clone() else {
            return;
        };
        self.status = "正在补全 PDF 图片……".into();
        self.worker(cx, move |cancel, tx| {
            let result = ws.complete_pdf_images(&cancel, |done, total| {
                let _ = tx.send(Event::Status(format!("补全图片 {done}/{total}")));
            });
            match result.and_then(|count| Ok((count, ws.state()?))) {
                Ok((count, raw)) => {
                    let _ = tx.send(Event::Refresh(raw));
                    let _ = tx.send(Event::Status(format!(
                        "已补全 {count} 个图片区域，修订与笔记已保留"
                    )));
                }
                Err(e) => {
                    let _ = tx.send(Event::Error(format!("图片补全失败：{e:#}")));
                }
            }
        });
        self.home.cancellable_task = true;
    }
    fn request_block_image(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(ws) = self.workspace.clone() else {
            return;
        };
        if self.block_images.contains_key(&index) || !self.image_loading.insert(index) {
            return;
        }
        let source = self.raw["paper"]["blocks"][index]["src"]
            .as_str()
            .unwrap_or("")
            .to_owned();
        let source_kind = self.state.paper.blocks[index].source_kind.clone();
        let root = ws.root.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let load = || -> anyhow::Result<_> {
                        let path = ws.asset(&source)?;
                        let bytes = easyread_gpui::figure_image::display_bytes(
                            &source_kind,
                            std::fs::read(path)?,
                        )?;
                        let format = match image::guess_format(&bytes)? {
                            image::ImageFormat::Png => ImageFormat::Png,
                            image::ImageFormat::WebP => ImageFormat::Webp,
                            image::ImageFormat::Jpeg => ImageFormat::Jpeg,
                            image::ImageFormat::Gif => ImageFormat::Gif,
                            image::ImageFormat::Bmp => ImageFormat::Bmp,
                            image::ImageFormat::Tiff => ImageFormat::Tiff,
                            _ => anyhow::bail!("不支持的图片格式"),
                        };
                        let (w, h) = image::ImageReader::new(std::io::Cursor::new(&bytes))
                            .with_guessed_format()?
                            .into_dimensions()?;
                        anyhow::ensure!(w > 0 && h > 0, "图片尺寸无效");
                        Ok((
                            Arc::new(Image::from_bytes(format, bytes)),
                            w as f32 / h as f32,
                            w,
                        ))
                    };
                    load().map_err(|e| format!("{e:#}"))
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.workspace.as_ref().is_some_and(|ws| ws.root == root) {
                    this.block_images.insert(index, result);
                    this.image_loading.remove(&index);
                    this.list.remeasure_items(index..index + 1);
                    this.epub_reflow.invalidate_from(index);
                    cx.notify();
                }
            });
        })
        .detach();
    }
    fn decorate_reader_html(&self, index: usize, source: String) -> String {
        let Some(block) = self.state.paper.blocks.get(index) else {
            return source;
        };
        let notes: Vec<_> = self.raw["reader"]["notes"]
            .as_object()
            .map(|notes| {
                notes
                    .values()
                    .filter(|note| note["anchor"] == block.id)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        let source = annotations::decorate_interactive(&source, &notes);
        let source = self
            .margin_editor
            .as_ref()
            .filter(|e| e.selection.block == index)
            .map(|e| annotations::margin_editor_anchor(&source, &e.note.record))
            .unwrap_or(source);
        self.navigation_note
            .as_ref()
            .filter(|note| note["anchor"] == block.id)
            .map(|note| annotations::navigation_anchor(&source, note))
            .unwrap_or(source)
    }

    fn reader_html_view(
        &mut self,
        index: usize,
        original: bool,
        source: String,
        cx: &mut Context<Self>,
    ) -> Entity<TextViewState> {
        let source = easyread_gpui::typst_table::prepare_html(&source).unwrap_or(source);
        if let Some((old, view)) = self.text_views.get_mut(&(index, original)) {
            if *old != source {
                // Decoration-only updates keep selection; actual replacements
                // remain asynchronous, including while warming an offscreen page.
                view.update(cx, |state, cx| state.set_html_decorations(&source, cx));
                *old = source;
            }
            return view.clone();
        }
        let view = cx.new(|cx| {
            let mut state = TextViewState::html(&source, cx);
            state.set_markdown_extensions(
                Arc::new(
                    gpui_kit::base::text::MarkdownExtensions::default()
                        .plugin(math::MathPlugin {
                            max_width: self.appearance.text_width(),
                        })
                        .plugin(math::TablePlugin {
                            max_width: self.appearance.text_width(),
                        }),
                ),
                cx,
            );
            state
        });
        self.observe_reader_view(&view, index, original, cx);
        self.text_views
            .insert((index, original), (source, view.clone()));
        view
    }

    fn reader_figure_caption(&self, index: usize) -> bool {
        self.state
            .paper
            .blocks
            .get(index)
            .is_some_and(|block| block.kind == "figure")
            || self.raw["paper"]["blocks"]
                .as_array()
                .is_some_and(|blocks| {
                    easyread_gpui::formats::epub::is_figure_caption(blocks, index)
                })
    }

    // Layout source excludes selection/navigation/highlight decorations. Those
    // change hit targets and colors, not the book's text-flow revision.
    fn reader_layout_html(&self, index: usize, original: bool) -> String {
        if original == self.primary_original()
            && let Some(expansion) = self.paragraph_languages.get(&index)
        {
            return expansion.layout.render(&expansion.expanded);
        }
        let source = self.reader_language_layout_html(index, original);
        if original != self.primary_original() && self.show_secondary(index) {
            easyread_gpui::paragraph_translation::without_shared_content(&source)
        } else {
            source
        }
    }

    fn reader_language_layout_html(&self, index: usize, original: bool) -> String {
        let Some(block) = self.state.paper.blocks.get(index) else {
            return String::new();
        };
        // Resolve the source language independently: no reader edit or saved
        // translation layout may enter the original-language rendering path.
        let source = if original {
            self.pdf_code_html
                .get(&index)
                .filter(|(source, _)| source == &block.en)
                .and_then(|(_, presentation)| presentation.as_ref())
                .map(|presentation| presentation.html.clone())
                .or_else(|| {
                    easyread_gpui::formats::epub::source_html(
                        &self.raw["paper"]["blocks"][index],
                        block,
                        &self.state,
                        true,
                    )
                    .map(|html| code::epub_html(html, self.epub_code_language))
                })
                .unwrap_or_else(|| {
                    formula::native_markdown_html(&self.state.original_markdown(block))
                })
        } else {
            let active = self.state.text(&block.id, &block.zh, &block.en);
            let key = if matches!(block.kind.as_str(), "figure" | "table") {
                format!("{}#caption", block.id)
            } else {
                block.id.clone()
            };
            let active = if matches!(block.kind.as_str(), "figure" | "table") {
                self.state.caption(block)
            } else {
                active
            };
            let source_text = if matches!(block.kind.as_str(), "figure" | "table") {
                &block.caption_en
            } else {
                &block.en
            };
            easyread_gpui::paragraph_edit::edited_html(
                &self.raw["reader"]["edits"][&key],
                source_text,
                active,
            )
            .map(str::to_owned)
            .or_else(|| {
                easyread_gpui::translation_layout::saved_html(
                    &self.raw["paper"]["blocks"][index],
                    active,
                )
            })
            .or_else(|| {
                self.pdf_code_html
                    .get(&index)
                    .filter(|(source, _)| source == &block.en)
                    .and_then(|(_, presentation)| presentation.as_ref())
                    .and_then(|presentation| {
                        if active == block.en {
                            Some(presentation.html.clone())
                        } else if block.source_kind == "pdf-text" && active == block.zh {
                            easyread_gpui::translation_layout::legacy_html(
                                &block.en,
                                active,
                                &presentation.html,
                                block.page,
                            )
                        } else {
                            None
                        }
                    })
            })
            .or_else(|| {
                easyread_gpui::formats::epub::source_html(
                    &self.raw["paper"]["blocks"][index],
                    block,
                    &self.state,
                    false,
                )
                .map(|html| code::epub_html(html, self.epub_code_language))
            })
            .unwrap_or_else(|| formula::native_markdown_html(&self.state.markdown(block, false)))
        };
        // Keep alignment on the paragraph itself, including translated/edited
        // captions. The native inline flow then uses centered row geometry for
        // glyphs, selection, links and annotations, not just centered painting.
        if self.reader_figure_caption(index) {
            source.replace("<p>", "<p style=\"text-align: center\">")
        } else {
            source
        }
    }

    fn reader_body_html(&self, index: usize, original: bool) -> String {
        self.decorate_reader_html(index, self.reader_layout_html(index, original))
    }

    fn warm_pdf_text(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.reading_mode == ReadingMode::Bilingual {
            self.ensure_paragraph_layout(index);
        }
        let Some(block) = self.state.paper.blocks.get(index) else {
            return;
        };
        let recovered = self
            .pdf_code_html
            .get(&index)
            .is_some_and(|(source, _)| source == &block.en);
        let primary_pdf = self.state.text(&block.id, &block.zh, &block.en) == block.en;
        // Warm only visible sources. Translations need not wait for PDF recovery.
        for original in self.rendered_languages(index) {
            if recovered || (!original && !primary_pdf) {
                let source = self.reader_body_html(index, original);
                self.reader_html_view(index, original, source, cx);
            }
        }
    }

    fn observe_reader_view(
        &self,
        view: &Entity<TextViewState>,
        index: usize,
        original: bool,
        cx: &mut Context<Self>,
    ) {
        let mut measured_height = None;
        let mut ready_source = None;
        cx.observe(view, move |this, view, cx| {
            let Some((source, current)) = this.text_views.get(&(index, original)) else {
                return;
            };
            if current != &view {
                return;
            }
            let state = view.read(cx);
            let height = state.bounds().size.height;
            let height_changed = height > px(0.) && measured_height != Some(height);
            // A parser result can be ready without having painted yet. Dirty
            // the cached row then, not only after its height changes, otherwise
            // a loading row may never render the newly prepared TextView.
            let content_changed = state.is_content_ready() && ready_source.as_ref() != Some(source);
            if height_changed {
                measured_height = Some(height);
            }
            if content_changed {
                ready_source = Some(source.clone());
            }
            if height_changed || content_changed {
                this.list.remeasure_items(index..index + 1);
                // A parser's first ready/paint notification can arrive AFTER
                // offscreen pagination already consumed that exact source.
                // Only a genuinely different measured revision invalidates it;
                // first display and page-position changes must reuse the map.
                if content_changed && this.is_epub() {
                    let layout_source = this.reader_layout_html(index, original);
                    if !this
                        .epub_reflow
                        .matches_measured_source(index, original, &layout_source)
                    {
                        this.epub_reflow.invalidate_from(index);
                    }
                }
            }
            cx.notify();
        })
        .detach();
    }

    fn reader_text(
        &self,
        view: &Entity<TextViewState>,
        original: bool,
        caption: bool,
        english: bool,
        index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let secondary = original != self.primary_original() && self.has_original(index);
        let source_pending = self.pdf_code_pending(index)
            && (original
                || self.state.paper.blocks.get(index).is_some_and(|block| {
                    self.state.text(&block.id, &block.zh, &block.en) == block.en
                }));
        if source_pending || !view.read(cx).is_content_ready() {
            return self.reader_text_loading(index, original, cx);
        }
        // ReaderViews can also be opened through import/recovery/test paths.
        // Do not rely solely on the outer application's one-time setup.
        if !gpui_kit::base::TextViewDefaults::global(cx).has_code_block_highlighter() {
            let defaults = gpui_kit::base::TextViewDefaults::global(cx)
                .clone()
                .with_code_block_highlighter(code::highlight);
            cx.set_global(defaults);
        }
        let weak = cx.entity().downgrade();
        let text_size = self.appearance.font_size * typography::text_scale(secondary, caption);
        let family = self
            .fonts
            .reading(self.appearance.reading_font(), original || english);
        view.update(cx, |state, cx| {
            state.set_track_paragraphs(true, cx);
            state.set_track_page_breaks(self.is_epub());
        });
        let measured_width = f32::from(view.read(cx).bounds().size.width);
        let math_width = self.appearance.text_width().min(if measured_width > 0. {
            measured_width
        } else {
            self.appearance.text_width()
        });
        let text = gpui_kit::base::TextView::new(view)
            .markdown_extensions(gpui_kit::base::text::MarkdownExtensions::default()
                .plugin(math::MathPlugin { max_width: math_width })
                .plugin(math::TablePlugin { max_width: math_width }))
            .selectable(true)
            .style(typography::reader_rich_style(self, index, original, family.clone(), text_size, cx))
            .font_family(family)
            .font_weight(FontWeight::NORMAL)
            .text_color(rgb(if secondary || caption { typography::READER_MUTED_INK } else { typography::READER_INK }))
            .text_size(px(text_size))
            .line_height(relative(if secondary {
                typography::SECONDARY_LINE_HEIGHT
            } else if caption {
                typography::CAPTION_LINE_HEIGHT
            } else if self.state.paper.blocks.get(index).is_some_and(|b| b.kind == "heading") {
                1.5
            } else {
                self.appearance.line_spacing
            }))
            .when(self.reader_figure_caption(index), |text| {
                text.text_center()
            })
            .on_link_click(move |url, event, window, cx| {
                if let Some(id) = url.strip_prefix("easyread-annotation:") {
                    if id == "reader-navigation-target" || id == annotations::MARGIN_EDITOR_ANCHOR {
                        return;
                    }
                    if let ClickEvent::Mouse(click) = event
                        && click.up.button == MouseButton::Left
                    {
                        let _ = weak.update(cx, |this, cx| {
                            if !this.raw["reader"]["notes"][id]["deleted"]
                                .as_bool()
                                .unwrap_or(false)
                            {
                                this.selection = None;
                                this.annotation_popup = Some((id.to_owned(), click.up.position));
                                this.follow_block(index);
                                cx.notify();
                            }
                        });
                    }
                } else if let Some(id) = url.strip_prefix("easyread-image:") {
                    if matches!(event, ClickEvent::Keyboard(_)) || matches!(event, ClickEvent::Mouse(click) if click.up.button == MouseButton::Left) {
                        let _ = weak.update(cx, |this, cx| {
                            let figure = this.pdf_code_html.get(&index)
                                .and_then(|(_, p)| p.as_ref())
                                .and_then(|p| p.figures.iter().find(|f| f.id == id));
                            if let Some(figure) = figure {
                                let image = Arc::new(Image::from_bytes(ImageFormat::Png, figure.png.clone()));
                                let aspect = figure.width as f32 / figure.height as f32;
                                this.open_image_preview(image, aspect, figure.width, window, cx);
                            }
                        });
                    }
                } else if let Some(target) = url.strip_prefix("vellio-epub:") {
                    if matches!(event, ClickEvent::Keyboard(_)) || matches!(event, ClickEvent::Mouse(click) if click.up.button == MouseButton::Left) {
                        let _ = weak.update(cx, |this, cx| {
                            this.follow_epub_link(target, window, cx);
                        });
                    }
                } else if url.starts_with("easyread-pdf:") {
                    if matches!(event, ClickEvent::Keyboard(_)) || matches!(event, ClickEvent::Mouse(click) if click.up.button == MouseButton::Left) {
                        let _ = weak.update(cx, |this, cx| this.follow_pdf_link(url.to_string(), window, cx));
                    }
                } else if let ClickEvent::Mouse(click) = event
                    && matches!(click.up.button, MouseButton::Left | MouseButton::Middle)
                {
                    cx.open_url(url);
                } else if matches!(event, ClickEvent::Keyboard(_)) {
                    cx.open_url(url);
                }
            });
        let indicator_view = view.clone();
        let indicator_app = cx.entity().downgrade();
        let state = view.read(cx);
        let preview = self.hovered_note_card.as_ref().and_then(|id| {
            let note = self
                .margin_records
                .get(id)
                .or_else(|| self.raw["reader"]["notes"].get(id))?;
            if note["anchor"] != self.state.paper.blocks[index].id {
                return None;
            }
            let bounds = if note["quote"].as_str().unwrap_or("").is_empty()
                && original == self.primary_original()
            {
                state.bounds()
            } else {
                let offset = state
                    .annotation_offset(id)
                    .or_else(|| state.annotation_offset(note["source_annotation"].as_str()?))?;
                let paragraph = state.paragraph_at(point(
                    state.bounds().left() + px(4.),
                    state.bounds().top() + offset + px(2.),
                ))?;
                state.paragraph_bounds(paragraph)?
            };
            Some((
                (bounds.top() - state.bounds().top()).max(px(0.)),
                bounds.size.height,
            ))
        });
        let active = self
            .active_reading_paragraph
            .filter(|(block, is_original, _)| *block == index && *is_original == original)
            .and_then(|(_, _, paragraph)| state.paragraph_bounds(paragraph))
            .map(|bounds| {
                (
                    (bounds.top() - state.bounds().top()).max(px(0.)),
                    bounds.size.height,
                )
            });
        div()
            .id(("reading-paragraph", view.entity_id()))
            .flex_shrink_0()
            .debug_selector(|| "reader-text-ready".into())
            .on_mouse_down(MouseButton::Left, move |event, _, cx| {
                if let Some(paragraph) = indicator_view.read(cx).paragraph_at(event.position) {
                    let _ = indicator_app.update(cx, |this, cx| {
                        this.active_reading_paragraph = Some((index, original, paragraph));
                        if original == this.primary_original() && !this.show_secondary(index) {
                            // Single-language clicks need the same shared-math
                            // policy as default bilingual layouts, before the
                            // floating toolbar decides whether En is enabled.
                            this.ensure_paragraph_layout(index);
                        }
                        cx.notify();
                    });
                }
            })
            .relative()
            .w_full()
            .when_some(preview, |d, (top, height)| {
                d.child(
                    div()
                        .debug_selector(|| "annotation-paragraph-preview".into())
                        .absolute()
                        .left(px(-8.))
                        .right(px(-8.))
                        .top(top - px(4.))
                        .h(height + px(8.))
                        .rounded(px(5.))
                        .bg(rgb(0xecece6)),
                )
            })
            .child(text)
            .when_some(active, |d, (top, height)| {
                d.child(
                    div()
                        .absolute()
                        .left(px(-16.))
                        .top(top)
                        .h(height)
                        .w(px(3.))
                        .rounded(px(2.))
                        .bg(rgb(0x88a3ab)),
                )
            })
            .into_any_element()
    }

    fn has_original(&self, index: usize) -> bool {
        let block = &self.state.paper.blocks[index];
        match block.kind.as_str() {
            "figure" | "table" => {
                !block.caption_en.trim().is_empty()
                    && self.state.text(
                        &format!("{}#caption", block.id),
                        &block.caption_zh,
                        &block.caption_en,
                    ) != block.caption_en
            }
            "list" => block.items.iter().enumerate().any(|(i, item)| {
                !item.en.trim().is_empty()
                    && self
                        .state
                        .text(&format!("{}#{i}", block.id), &item.zh, &item.en)
                        != item.en
            }),
            _ => {
                !block.en.trim().is_empty()
                    && self.state.text(&block.id, &block.zh, &block.en) != block.en
            }
        }
    }

    fn follow_epub_link(&mut self, target: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.raw["paper"]["link_targets"][target].as_str() else {
            self.status = "EPUB 链接目标未包含在阅读内容中".into();
            cx.notify();
            return;
        };
        let Some(index) = self.state.paper.blocks.iter().position(|b| b.id == id) else {
            return;
        };
        self.list.scroll_to(ListOffset {
            item_ix: index,
            offset_in_item: px(0.),
        });
        self.select(index, window, cx);
        cx.notify();
    }

    fn follow_pdf_link(&mut self, url: String, window: &Window, cx: &mut Context<Self>) {
        let Some(ws) = self.workspace.as_ref() else {
            return;
        };
        let Some(rel) = self.raw["paper"]["meta"]["pdf"].as_str() else {
            return;
        };
        let Ok(pdf) = ws.asset(rel) else {
            return;
        };
        let root = ws.root.clone();
        cx.spawn_in(window, async move |this, cx| {
            let target = cx.background_executor().spawn(async move {
                easyread_gpui::pdf::link_destination(&pdf, &url)
            }).await;
            let _ = cx.update(|window, cx| {
                let _ = this.update(cx, |this, cx| {
                    if !this.workspace.as_ref().is_some_and(|ws| ws.root == root) { return; }
                    let Ok((page, quote)) = target else { this.status = "无法读取 PDF 链接目标".into(); cx.notify(); return; };
                    let Some(index) = this.state.paper.blocks.iter().position(|b| b.page == page && b.source_kind == "pdf-text") else { return; };
                    if quote.is_empty() {
                        this.list.scroll_to(ListOffset { item_ix: index, offset_in_item: px(0.) });
                        this.select(index, window, cx);
                    } else {
                        let anchor = this.state.paper.blocks[index].id.clone();
                        this.jump_to_annotation(json!({"id":"reader-navigation-target", "anchor":anchor, "quote":quote, "pdf_figure":quote.starts_with("Figure ") || quote.starts_with("Table ")}), window, cx);
                    }
                });
            });
        }).detach();
    }

    fn reader_text_loading(&self, index: usize, original: bool, cx: &Context<Self>) -> AnyElement {
        let font = self.appearance.font_size;
        let chars_per_line = (self.appearance.text_width() / (font * 0.52)).max(1.);
        let characters = self
            .state
            .paper
            .blocks
            .get(index)
            .map_or(0, |b| b.en.chars().count());
        let estimate =
            ((characters as f32 / chars_per_line).ceil() * font * self.appearance.line_spacing
                + 64.)
                .clamp(160., 3000.);
        let height = self
            .text_views
            .get(&(index, original))
            .map(|(_, view)| view.read(cx).bounds().size.height)
            .filter(|height| *height > px(0.))
            .unwrap_or(px(estimate));
        div()
            .id(("reader-text-loading", index * 2 + usize::from(original)))
            .debug_selector(|| "reader-text-loading".into())
            .w_full()
            .h(height)
            .text_size(px(13.))
            .text_color(rgb(0x969084))
            .child("正在准备本页排版……")
            .into_any_element()
    }

    fn pdf_code_pending(&self, index: usize) -> bool {
        (self.pdf_code_cache.is_some() || self.pdf_code_loading.contains(&index))
            && self.state.paper.blocks.get(index).is_some_and(|block| {
                block.source_kind == "pdf-text"
                    && block.kind == "para"
                    && !self
                        .pdf_code_html
                        .get(&index)
                        .is_some_and(|(source, _)| source == &block.en)
            })
    }

    fn update_pdf_focus(&mut self, index: usize) {
        let focus = self
            .state
            .paper
            .blocks
            .get(index)
            .filter(|b| b.source_kind != "pdf-text" || b.kind != "para")
            .and_then(|block| {
                self.state.paper.blocks.iter().position(|b| {
                    b.page == block.page && b.source_kind == "pdf-text" && b.kind == "para"
                })
            })
            .unwrap_or(index);
        if focus != self.pdf_code_focus {
            self.pdf_code_forward = focus > self.pdf_code_focus;
            self.pdf_code_focus = focus;
            self.pdf_code_needs_plan = true;
        }
    }

    fn nearby_pdf_indices(&self) -> Vec<usize> {
        let eligible: Vec<_> = self
            .state
            .paper
            .blocks
            .iter()
            .enumerate()
            .filter_map(|(i, b)| (b.source_kind == "pdf-text" && b.kind == "para").then_some(i))
            .collect();
        pdf_layout_cache::lookahead(&eligible, self.pdf_code_focus, self.pdf_code_forward)
    }

    fn request_nearby_pdf_code(&mut self, index: usize, cx: &mut Context<Self>) {
        // Do not inspect ListState here: its render callback holds its mutable
        // borrow. main_view updates focus before entering the list renderer.
        if self
            .rendered_languages(index)
            .iter()
            .any(|original| !self.text_views.contains_key(&(index, *original)))
        {
            self.warm_pdf_text(index, cx);
        }
        if self.pdf_code_needs_plan {
            for nearby in self.nearby_pdf_indices() {
                if self
                    .rendered_languages(nearby)
                    .iter()
                    .any(|original| !self.text_views.contains_key(&(nearby, *original)))
                {
                    self.warm_pdf_text(nearby, cx);
                }
            }
        }
        self.pump_pdf_code(cx);
    }

    fn pump_pdf_code(&mut self, cx: &mut Context<Self>) {
        if self.pdf_code_active || !self.pdf_code_needs_plan {
            return;
        }
        let Some(ws) = self.workspace.as_ref() else {
            return;
        };
        let Some(rel) = self.raw["paper"]["meta"]["pdf"].as_str() else {
            return;
        };
        let Ok(pdf) = ws.asset(rel) else {
            return;
        };
        let root = ws.root.clone();
        if self.pdf_code_cache.is_none() {
            let Ok(cache) = PresentationCache::new(&pdf, &root.join(".native-pdf-layout-cache"))
            else {
                return;
            };
            self.pdf_code_cache = Some(cache);
        }
        let eligible: Vec<_> = self
            .state
            .paper
            .blocks
            .iter()
            .enumerate()
            .filter_map(|(i, b)| (b.source_kind == "pdf-text" && b.kind == "para").then_some(i))
            .collect();
        let ready = self
            .pdf_code_html
            .iter()
            .filter_map(|(i, (source, _))| {
                self.state
                    .paper
                    .blocks
                    .get(*i)
                    .is_some_and(|b| &b.en == source)
                    .then_some(*i)
            })
            .collect();
        let prepared = self
            .pdf_code_prepared
            .iter()
            .filter_map(|(i, source)| {
                self.state
                    .paper
                    .blocks
                    .get(*i)
                    .is_some_and(|b| &b.en == source)
                    .then_some(*i)
            })
            .collect();
        let batch = pdf_layout_cache::next_batch(
            &eligible,
            self.pdf_code_focus,
            self.pdf_code_forward,
            &ready,
            &prepared,
        );
        self.pdf_code_needs_plan = false;
        if batch.is_empty() {
            return;
        }
        let requests: Vec<_> = batch
            .into_iter()
            .map(|i| {
                let b = &self.state.paper.blocks[i];
                self.pdf_code_loading.insert(i);
                (i, b.id.clone(), b.page, b.en.clone())
            })
            .collect();
        let nearby = self.nearby_pdf_indices();
        let inputs: Vec<_> = requests
            .iter()
            .map(|(index, _, page, source)| {
                (
                    *page,
                    source.clone(),
                    nearby.contains(index) || self.pdf_code_html.contains_key(index),
                )
            })
            .collect();
        let cache = self.pdf_code_cache.clone().unwrap();
        let generation = self.pdf_code_generation;
        self.pdf_code_active = true;
        cx.spawn(async move |this, cx| {
            let results = cx
                .background_executor()
                .spawn(async move { cache.prepare_selective(&pdf, &inputs) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if generation != this.pdf_code_generation
                    || !this.workspace.as_ref().is_some_and(|ws| ws.root == root)
                {
                    return;
                }
                this.pdf_code_active = false;
                let nearby = this.nearby_pdf_indices();
                let mut visible_changed = false;
                for ((index, id, page, source), result) in requests.into_iter().zip(results) {
                    this.pdf_code_loading.remove(&index);
                    if !this
                        .state
                        .paper
                        .blocks
                        .get(index)
                        .is_some_and(|b| b.id == id && b.page == page && b.en == source)
                    {
                        continue;
                    }
                    this.pdf_code_prepared.insert(index, source.clone());
                    let presentation = match result {
                        Ok(pdf_layout_cache::PreparedPage::OnDisk) => continue,
                        Ok(pdf_layout_cache::PreparedPage::Loaded(presentation)) => presentation,
                        Err(_) => None,
                    };
                    // Prepare the whole book on disk, but do not retain every
                    // page's raster previews/AST in memory. Keep the lookahead
                    // window and pages already read, as before.
                    if nearby.contains(&index) || this.pdf_code_html.contains_key(&index) {
                        this.pdf_code_html.insert(index, (source, presentation));
                        this.paragraph_languages.remove(&index);
                        this.unpaired_paragraphs.remove(&index);
                        this.warm_pdf_text(index, cx);
                        let end = this
                            .state
                            .paper
                            .blocks
                            .iter()
                            .rposition(|b| b.page == page)
                            .map_or(index + 1, |i| i + 1);
                        this.list.remeasure_items(index..end);
                        visible_changed = true;
                    }
                }
                if visible_changed {
                    cx.notify();
                }
                // Continue through the book while idle. A jump/scroll changes
                // focus before the next batch, rather than queuing PDF waiters.
                this.pdf_code_needs_plan = true;
                this.pump_pdf_code(cx);
            });
        })
        .detach();
    }

    fn reader_suppresses_pdf_figure(&self, index: usize) -> bool {
        let Some(figure) = self
            .state
            .paper
            .blocks
            .get(index)
            .filter(|b| b.source_kind == "pdf-image")
        else {
            return false;
        };
        self.state
            .paper
            .blocks
            .iter()
            .enumerate()
            .any(|(parent, text)| {
                if text.source_kind != "pdf-text" || text.page != figure.page {
                    return false;
                }
                let active = self.state.text(&text.id, &text.zh, &text.en);
                let raw = &self.raw["paper"]["blocks"][parent];
                // Persisted native layout already includes the image, even before
                // original PDF recovery finishes warming after reopening the book.
                // Companion images are omitted even on the conservative
                // block fallback, so only the primary flow can own a figure.
                let languages = [self.primary_original()];
                if languages.contains(&false)
                    && raw["native_translation_layout"]["inline_figures"]
                        .as_array()
                        .is_some_and(|ids| ids.iter().any(|id| id == &figure.id))
                    && easyread_gpui::translation_layout::saved_html(raw, active).is_some()
                {
                    return true;
                }
                self.pdf_code_html
                    .get(&parent)
                    .filter(|(source, _)| source == &text.en)
                    .and_then(|(_, p)| p.as_ref())
                    .is_some_and(|p| {
                        if !p.inline_figures.contains(&figure.id) {
                            return false;
                        }
                        if languages.contains(&true) || active == text.en {
                            return true;
                        }
                        // Legacy repair only embeds source-backed captioned image
                        // groups. Suppress duplicates only when ALL source previews
                        // really occur in the translated layout, not just because
                        // the original page contains them.
                        if p.figures.is_empty() {
                            return false;
                        }
                        if !languages.contains(&false) {
                            return false;
                        }
                        let html = self.reader_layout_html(parent, false);
                        p.figures
                            .iter()
                            .all(|f| html.contains(&format!("easyread-image:{}", f.id)))
                    })
            })
    }

    fn block(&mut self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        self.request_nearby_pdf_code(index, cx);
        if self.reading_mode == ReadingMode::Bilingual {
            self.ensure_paragraph_layout(index);
        }
        if self.reader_suppresses_pdf_figure(index) {
            return div().into_any_element();
        }
        if self
            .state
            .paper
            .blocks
            .get(index)
            .is_some_and(|b| b.kind == "figure")
        {
            self.request_block_image(index, cx);
        }
        let block = self.state.paper.blocks.get(index).cloned();
        let Some(b) = block.as_ref() else {
            return div().into_any_element();
        };
        let caption =
            matches!(b.kind.as_str(), "figure" | "table") || self.reader_figure_caption(index);
        let english = if matches!(b.kind.as_str(), "figure" | "table") {
            b.caption_zh.is_empty()
        } else {
            b.zh.is_empty()
        };
        let mut content = div().flex().flex_col().gap_3().min_w_0();
        if b.kind == "figure" {
            let max_image_height = if self.is_epub() {
                self.epub_reflow
                    .size
                    .map(|s| f32::from(s.height) - 110.)
                    .unwrap_or(600.)
                    .max(40.)
            } else {
                f32::MAX
            };
            let media = match self.block_images.get(&index) {
                Some(Ok((image, aspect, width))) => div()
                    .relative()
                    .w_full()
                    .max_w(px((*width as f32)
                        .min(self.appearance.text_width())
                        .min(max_image_height * *aspect)))
                    .mx_auto()
                    .aspect_ratio(*aspect)
                    .child(
                        img(image.clone())
                            .id(("figure-image", index))
                            .debug_selector(move || format!("reader-figure-image-{index}"))
                            .absolute()
                            .size_full()
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _, window, cx| {
                                if let Some(Ok((image, aspect, width))) =
                                    this.block_images.get(&index)
                                {
                                    this.open_image_preview(
                                        image.clone(),
                                        *aspect,
                                        *width,
                                        window,
                                        cx,
                                    );
                                    cx.stop_propagation();
                                }
                            })),
                    )
                    .into_any_element(),
                Some(Err(error)) => div()
                    .p_4()
                    .border_1()
                    .border_color(rgb(0xd3cbbb))
                    .text_size(px(13.))
                    .text_color(rgb(0xb0472f))
                    .child(format!("图片无法显示：{error}"))
                    .into_any_element(),
                None => div()
                    .w_full()
                    .mx_auto()
                    .map(|d| {
                        let raw = &self.raw["paper"]["blocks"][index];
                        match raw["width"]
                            .as_f64()
                            .zip(raw["height"].as_f64())
                            .filter(|(w, h)| *w > 0. && *h > 0.)
                        {
                            Some((w, h)) => d
                                .max_w(px((w as f32)
                                    .min(self.appearance.text_width())
                                    .min(max_image_height * (w / h) as f32)))
                                .aspect_ratio((w / h) as f32),
                            None => d.h(px(180.)),
                        }
                    })
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(13.))
                    .text_color(rgb(0x8c8579))
                    .child("正在加载图片……")
                    .into_any_element(),
            };
            content = content.child(media);
        }
        if b.kind == "math" {
            match self.formulas.get(&b.tex) {
                Some(Ok(_)) => {
                    let source = format!(
                        "<p style=\"text-align:center\">{}</p>",
                        formula::native_html(&b.tex, true)
                    );
                    let view = self.reader_html_view(index, false, source, cx);
                    content =
                        content.child(self.reader_text(&view, false, false, false, index, cx));
                }
                Some(Err(e)) => {
                    content = content.child(TextView::markdown(
                        ("formula-error", index),
                        format!("```latex\n{}\n```\n排版失败：{e}", b.tex),
                    ))
                }
                None => {
                    content = content.child(TextView::markdown(
                        ("formula-pending", index),
                        format!("```latex\n{}\n```", b.tex),
                    ))
                }
            }
        } else if b.kind != "figure" || !self.state.caption(b).trim().is_empty() {
            for (position, original) in self.rendered_languages(index).into_iter().enumerate() {
                let source = self.reader_body_html(index, original);
                let view = self.reader_html_view(index, original, source, cx);
                let text = if let Some(inline) = self.inline_edit_flow(index, original, cx) {
                    inline
                } else {
                    self.reader_text(&view, original, caption, original || english, index, cx)
                };
                if position == 0 {
                    content = content.child(text);
                } else {
                    content = content.child(div().mt_2().text_color(rgb(0x77716a)).child(text));
                }
            }
        }
        let note_count = self.raw["reader"]["notes"]
            .as_object()
            .map(|notes| {
                notes
                    .values()
                    .filter(|n| n["anchor"] == b.id && !n["deleted"].as_bool().unwrap_or(false))
                    .count()
            })
            .unwrap_or(0);
        let margin = if !self.tool_panel_visible()
            && !self.show_pdf
            && (self.appearance.margin_notes || self.margin_editor.is_some())
        {
            let editing_id = self
                .margin_editor
                .as_ref()
                .filter(|editor| editor.selection.block == index)
                .and_then(|editor| editor.note.record["id"].as_str())
                .map(str::to_owned);
            let cards: Vec<_> = self
                .all_notes()
                .into_iter()
                .filter(|note| {
                    note["anchor"] == b.id
                        && self.appearance.margin_notes
                        && annotation_panel::personal(note)
                        && editing_id.as_deref() != note["id"].as_str()
                })
                .collect();
            let mut entries: Vec<_> = cards
                .iter()
                .map(|note| {
                    (
                        note["id"].as_str().unwrap_or("").to_owned(),
                        self.note_offset(note, index, cx),
                        self.annotation_card(note, false, cx),
                    )
                })
                .collect();
            if self.appearance.margin_notes {
                for entry in self.raw["discussion"]["entries"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|entry| {
                        entry["anchor"] == b.id
                            && entry["kind"] != "reply"
                            && entry["deleted"] != true
                    })
                {
                    entries.push((
                        entry["id"].as_str().unwrap_or("ai-entry").to_owned(),
                        self.note_offset(entry, index, cx),
                        self.discussion_card(entry, false, cx),
                    ));
                }
            }
            if let Some(editor) = self
                .margin_editor
                .as_ref()
                .filter(|editor| editor.selection.block == index)
                && let Some(view) = self.margin_note_view(cx)
            {
                entries.push((
                    editor.note.record["id"].as_str().unwrap_or("").to_owned(),
                    self.note_offset(&editor.note.record, index, cx),
                    view,
                ));
            }
            entries.sort_by(|a, b| {
                a.1.partial_cmp(&b.1)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then(a.0.cmp(&b.0))
            });
            let mut cursor = px(0.);
            let mut column = div()
                .w(px(300.))
                .flex_shrink_0()
                .pt(px(34.))
                .pb_5()
                .flex()
                .flex_col();
            for (id, offset, element) in entries {
                let gap = (offset - cursor).max(px(0.));
                cursor += gap
                    + self
                        .margin_card_heights
                        .get(&id)
                        .copied()
                        .unwrap_or(px(160.))
                    + px(18.);
                column = column.child(div().mt(gap).mb(px(18.)).child(element));
            }
            Some(column.into_any_element())
        } else {
            None
        };
        let gutter_width = if self.settings.committed.features.page {
            48.
        } else {
            0.
        };
        let row_width = self.appearance.text_width()
            + 48.
            + gutter_width
            + if margin.is_some() { 324. } else { 0. };
        let gutter_top = self
            .active_reading_paragraph
            .filter(|(block, _, _)| *block == index)
            .and_then(|(_, original, paragraph)| {
                let bounds = self
                    .text_views
                    .get(&(index, original))?
                    .1
                    .read(cx)
                    .paragraph_bounds(paragraph)?;
                Some((bounds.top() - self.block_bounds.get(&index)?.top()).max(px(12.)))
            })
            .unwrap_or(px(12.));
        let emphasized = self.panel_open
            && self.panel == Panel::Notes
            && self.hovered_annotation.as_ref().map(|(_, block)| *block) == Some(index);
        let text = div()
            .w_full()
            .max_w(px(self.appearance.text_width() + 48.))
            .mx_auto()
            .px_6()
            .pt_3()
            .pb_5()
            .border_l_2()
            .border_color(if self.show_pdf && self.page_block == Some(index) {
                rgb(0x9c5f1c)
            } else {
                rgb(0xf6f3ec)
            })
            // Selecting a block for note/edit state must not tint the page.
            // Only an explicit annotation hover changes its background.
            .when(emphasized, |d| d.bg(rgb(0xecece6)).rounded(px(10.)))
            .when(
                self.state.stale(&b.id, &b.zh, &b.en) || note_count > 0,
                |d| {
                    let mut metadata = Vec::new();
                    if self.state.stale(&b.id, &b.zh, &b.en) {
                        metadata.push("保留修订，译者稿已更新".to_owned());
                    }
                    if note_count > 0 {
                        metadata.push(format!("{note_count} 条标记"));
                    }
                    d.child(
                        div()
                            .mb_3()
                            .text_size(px(11.))
                            .text_color(rgb(0x9c5f1c))
                            .child(metadata.join(" · ")),
                    )
                },
            )
            .child(content)
            .into_any_element();
        let row = div()
            .w_full()
            .max_w(px(row_width))
            .flex_shrink_0()
            .flex()
            .items_start()
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .relative()
                    .pr(px(gutter_width))
                    .child(text)
                    .when(self.settings.committed.features.page, |d| {
                        d.child(
                            div()
                                .absolute()
                                .right(px(0.))
                                .top(gutter_top)
                                .child(self.source_page_button(index, cx)),
                        )
                    }),
            )
            .when_some(margin, |d, margin| d.child(margin));
        // Distribute spare reading width 2:1 so the left margin is wider.
        // The body and its notes remain one column group when tools are open.
        let weak = cx.entity().downgrade();
        div()
            .id(("reader-block", index))
            .relative()
            .w_full()
            .overflow_hidden()
            .flex()
            .items_start()
            .child(
                canvas(
                    move |bounds, _, cx| {
                        let _ = weak.update(cx, |this, _| {
                            this.block_bounds.insert(index, bounds);
                        });
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .child(div().flex_1().flex_grow(2.))
            .child(row)
            .child(div().flex_1())
            .into_any_element()
    }
    fn reset_rows(&mut self) {
        self.epub_reflow.invalidate();
        self.epub_reflow_displayed = (usize::MAX, usize::MAX, usize::MAX);
        self.cancel_annotation_scroll();
        self.hovered_annotation = None;
        let offset = self.list.logical_scroll_top();
        self.list.reset(self.state.paper.blocks.len());
        self.list.scroll_to(offset);
    }

    fn appearance_changed(&mut self, cx: &mut Context<Self>) {
        self.selection = None;
        self.annotation_popup = None;
        self.margin_card_heights.clear();
        self.reset_rows();
        self.capture_position();
        cx.notify();
    }

    fn navigation_button(
        &self,
        navigation: Navigation,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let slot = if navigation == Navigation::Library {
            0
        } else {
            1
        };
        let next = if self.navigation == Some(navigation) {
            None
        } else {
            Some(navigation)
        };
        let weak = cx.entity().downgrade();
        div()
            .relative()
            .flex_shrink_0()
            .child(
                canvas(
                    move |bounds, _, cx| {
                        let _ = weak.update(cx, |this, _| {
                            this.navigation_button_bounds[slot] = Some(bounds)
                        });
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .child(
                Button::new(if slot == 0 {
                    "library-toggle"
                } else {
                    "contents-toggle"
                })
                .ghost()
                .small()
                .w(px(32.))
                .h(px(32.))
                .rounded(px(8.))
                .icon(
                    Icon::default()
                        .path(if slot == 0 {
                            "icons/book-open.svg"
                        } else {
                            "icons/list.svg"
                        })
                        .size(px(19.)),
                )
                .accessibility_label(if slot == 0 { "文献库" } else { "目录" })
                .when(slot == 0, |b| b.bg(rgb(0x326677)).text_color(rgb(0xfffaf0)))
                .when(slot == 1 && self.navigation == Some(navigation), |b| {
                    b.bg(rgba(0x38748720)).text_color(rgb(0x387487))
                })
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.navigation = next;
                    this.reading_order_open = false;
                    if slot == 0 && this.library.is_some() {
                        this.show_home(window, cx);
                        return;
                    }
                    this.appearance_open = false;
                    this.selection = None;
                    this.annotation_popup = None;
                    if next.is_some() {
                        this.navigation_focus.focus(window, cx);
                    } else {
                        window.blur(cx);
                    }
                    cx.notify();
                })),
            )
    }
    fn navigation_menu(&self, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let navigation = self.navigation?;
        let weak = cx.entity().downgrade();
        let mut entries = div()
            .id("navigation-entries")
            .h(px(480.).min((window.viewport_size().height - px(150.)).max(px(120.))))
            .track_scroll(&self.navigation_scroll)
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(px(4.));
        if navigation == Navigation::Contents {
            let mut headings = self.contents();
            let page_only = headings.is_empty();
            if page_only {
                let mut last_page = None;
                for (i, b) in self.state.paper.blocks.iter().enumerate() {
                    if last_page != Some(b.page) {
                        headings.push((i, format!("第 {} 页", b.page.max(1)), 0));
                        last_page = Some(b.page);
                    }
                }
                entries = entries.child(
                    div()
                        .p_2()
                        .text_size(px(12.))
                        .text_color(rgb(0x969084))
                        .child("这本书尚无结构化章节，按页导航"),
                );
            }
            let current = self.list.logical_scroll_top().item_ix;
            let active = headings
                .iter()
                .enumerate()
                .rfind(|(_, (i, _, _))| *i <= current)
                .map(|(entry, _)| entry);
            if headings.is_empty() {
                entries = entries.child(div().p_2().child("暂无目录"));
            }
            for (entry, (index, title, depth)) in headings.into_iter().enumerate() {
                let (number, body) = split_heading_number(&title);
                let label = div()
                    .w_full()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .pl(px(depth as f32 * 16.))
                    .when_some(number, |d, number| {
                        d.child(
                            div()
                                .flex_shrink_0()
                                .text_color(rgb(0x969084))
                                .child(number.to_owned()),
                        )
                    })
                    .child(div().flex_1().min_w_0().truncate().child(body.to_owned()));
                entries = entries.child(
                    Button::new(("contents-entry", entry))
                        .ghost()
                        .small()
                        .w_full()
                        .justify_start()
                        .accessibility_label(title.clone())
                        .child(label)
                        .when(active == Some(entry), |b| {
                            b.bg(rgba(0x38748720)).text_color(rgb(0x387487))
                        })
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.navigation = None;
                            this.list.scroll_to(ListOffset {
                                item_ix: index,
                                offset_in_item: px(0.),
                            });
                            this.select(index, window, cx);
                            window.blur(cx);
                        })),
                );
            }
        } else {
            for (i, item) in self.items.iter().enumerate() {
                let id = item["id"].as_str().unwrap_or("").to_owned();
                let title = item["meta"]["title_zh"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .or_else(|| item["meta"]["title_en"].as_str())
                    .unwrap_or("未命名")
                    .to_owned();
                entries = entries.child(
                    Button::new(("paper", i))
                        .ghost()
                        .small()
                        .w_full()
                        .justify_start()
                        .accessibility_label(title.clone())
                        .child(div().w_full().truncate().child(title))
                        .disabled(self.busy)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.navigation = None;
                            this.open(id.clone(), cx);
                        })),
                );
            }
            entries = entries.child(
                div()
                    .mt_2()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        Button::new("import")
                            .label("导入 PDF / EPUB")
                            .disabled(self.busy || self.library.is_none())
                            .on_click(cx.listener(|this, _, _, cx| this.pick(false, cx))),
                    )
                    .child(
                        Button::new("migrate")
                            .label("迁入旧论文")
                            .disabled(self.busy || self.library.is_none())
                            .on_click(cx.listener(|this, _, _, cx| this.pick(true, cx))),
                    )
                    .when(self.settings.committed.features.original, |d| {
                        d.child(
                            Button::new("bilingual")
                                .label(if self.reading_mode == ReadingMode::Bilingual {
                                    "仅原文"
                                } else {
                                    "中英对照"
                                })
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.toggle_bilingual(cx);
                                })),
                        )
                    })
                    .child(
                        Button::new("complete-images")
                            .label("补全图片")
                            .disabled(
                                self.busy
                                    || self.workspace.is_none()
                                    || !self
                                        .state
                                        .paper
                                        .blocks
                                        .iter()
                                        .any(|b| b.source_kind == "pdf-text"),
                            )
                            .on_click(cx.listener(|this, _, _, cx| this.complete_images(cx))),
                    )
                    .child(
                        Button::new("translate")
                            .label("翻译")
                            .disabled(self.busy || self.workspace.is_none())
                            .on_click(cx.listener(|this, _, _, cx| this.translate(cx))),
                    ),
            );
        }
        let search = div()
            .mt_2()
            .flex()
            .gap_2()
            .child(Input::new(&self.search))
            .child(Button::new("find").label("查找").on_click(cx.listener(
                |this, _, window, cx| {
                    let q = this.search.read(cx).value().to_string();
                    if q.is_empty() {
                        return;
                    }
                    if let Some(i) = this
                        .state
                        .paper
                        .blocks
                        .iter()
                        .position(|b| this.state.markdown(b, true).contains(&q))
                    {
                        this.navigation = None;
                        this.list.scroll_to(ListOffset {
                            item_ix: i,
                            offset_in_item: px(0.),
                        });
                        this.select(i, window, cx);
                        window.blur(cx);
                    } else {
                        this.status = "未找到".into();
                        cx.notify();
                    }
                },
            )));
        Some(
            div()
                .id("navigation-menu")
                .occlude()
                .absolute()
                .left(px(16.))
                .top(px(62.))
                .w(px(360.).min((window.viewport_size().width - px(32.)).max(px(220.))))
                .track_focus(&self.navigation_focus)
                .p(px(12.))
                .flex()
                .flex_col()
                .gap_2()
                .bg(rgb(0xfbfaf7))
                .border_1()
                .border_color(rgb(0xe2dccf))
                .rounded(px(12.))
                .shadow(vec![
                    BoxShadow::new(px(0.), px(3.), rgba(0x52452c20).into()).blur_radius(px(14.)),
                ])
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(
                    canvas(
                        move |bounds, _, cx| {
                            let _ = weak
                                .update(cx, |this, _| this.navigation_menu_bounds = Some(bounds));
                        },
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .size_full(),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .px_2()
                        .child(if navigation == Navigation::Contents {
                            "目录"
                        } else {
                            "文献库"
                        })
                        .child(
                            Button::new("close-navigation")
                                .ghost()
                                .small()
                                .label("×")
                                .accessibility_label("关闭导航")
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.navigation = None;
                                    window.blur(cx);
                                    cx.notify();
                                })),
                        ),
                )
                .child(search)
                .child(entries)
                .into_any_element(),
        )
    }
    fn appearance_menu(&self, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.appearance_open {
            return None;
        }
        let width = px(320.).min((window.viewport_size().width - px(24.)).max(px(240.)));
        let weak = cx.entity().downgrade();
        let slider_row = |label: &'static str, slider: &Entity<SliderState>, value: String| {
            div()
                .flex()
                .items_center()
                .gap(px(12.))
                .h(px(48.))
                .child(
                    div()
                        .w(px(56.))
                        .flex_shrink_0()
                        .text_color(rgb(0x969084))
                        .child(label),
                )
                .child(
                    div().flex_1().min_w_0().child(
                        Slider::new(slider)
                            .bg(rgb(0x326879))
                            .text_color(rgb(0x326879)),
                    ),
                )
                .child(div().w(px(60.)).flex_shrink_0().text_right().child(value))
        };
        let choices = div().flex().flex_col().gap(px(8.)).child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .h(px(44.))
                .child(
                    div()
                        .w(px(64.))
                        .text_color(rgb(0x969084))
                        .child("Margin\nnotes"),
                )
                .child(
                    div()
                        .flex()
                        .p(px(2.))
                        .rounded(px(10.))
                        .bg(rgb(0xeee9df))
                        .children(["Show", "Collapse"].into_iter().enumerate().map(
                            |(option, text)| {
                                let active = self.appearance.margin_notes == (option == 0);
                                Button::new(("appearance-choice", 4 + option))
                                    .ghost()
                                    .small()
                                    .h(px(30.))
                                    .px(px(11.))
                                    .rounded(px(8.))
                                    .text_color(rgb(if active { 0x23211d } else { 0x969084 }))
                                    .when(active, |b| {
                                        b.bg(rgb(0xfbfaf7)).shadow(vec![
                                            BoxShadow::new(px(0.), px(1.), rgba(0x52452c16).into())
                                                .blur_radius(px(2.)),
                                        ])
                                    })
                                    .label(text)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.appearance.margin_notes = option == 0;
                                        this.appearance_changed(cx);
                                        this.flush_session();
                                    }))
                            },
                        )),
                ),
        );
        Some(
            div()
                .id("appearance-menu")
                .occlude()
                .relative()
                .track_focus(&self.appearance_focus)
                .absolute()
                .right(px(12.))
                .top(px(62.))
                .w(width)
                .p(px(16.))
                .flex()
                .flex_col()
                .gap(px(8.))
                .bg(rgb(0xfbfaf7))
                .border_1()
                .border_color(rgb(0xe2dccf))
                .rounded(px(14.))
                .text_size(px(13.))
                .text_color(rgb(0x5f5a51))
                .shadow(vec![
                    BoxShadow::new(px(0.), px(3.), rgba(0x52452c20).into()).blur_radius(px(14.)),
                ])
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(
                    canvas(
                        move |bounds, _, cx| {
                            let _ = weak
                                .update(cx, |this, _| this.appearance_menu_bounds = Some(bounds));
                        },
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .size_full(),
                )
                .child(slider_row(
                    "Font\nsize",
                    &self.font_slider,
                    format!("{:.0} px", self.appearance.font_size),
                ))
                .child(slider_row(
                    "Line\nwidth",
                    &self.width_slider,
                    format!("{:.0} chars", self.appearance.line_width),
                ))
                .child(slider_row(
                    "Line\nspacing",
                    &self.spacing_slider,
                    format!("{:.1}", self.appearance.line_spacing),
                ))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(6.))
                        .py(px(6.))
                        .child(div().text_color(rgb(0x969084)).child("字体"))
                        .child(
                            div()
                                .flex()
                                .p(px(2.))
                                .rounded(px(10.))
                                .bg(rgb(0xeee9df))
                                .children(ReadingFont::ALL.into_iter().enumerate().map(
                                    |(i, font)| {
                                        let active = self.appearance.reading_font() == font;
                                        Button::new(("reading-font", i))
                                            .ghost()
                                            .small()
                                            .flex_1()
                                            .h(px(30.))
                                            .px(px(6.))
                                            .rounded(px(8.))
                                            .text_color(rgb(if active {
                                                0x23211d
                                            } else {
                                                0x969084
                                            }))
                                            .when(active, |b| {
                                                b.bg(rgb(0xfbfaf7)).shadow(vec![
                                                    BoxShadow::new(
                                                        px(0.),
                                                        px(1.),
                                                        rgba(0x52452c16).into(),
                                                    )
                                                    .blur_radius(px(2.)),
                                                ])
                                            })
                                            .label(font.label())
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.appearance.set_font(font);
                                                this.appearance_changed(cx);
                                                this.flush_session();
                                            }))
                                    },
                                )),
                        )
                        .child(div().text_size(px(11.)).text_color(rgb(0x969084)).child(
                            match self.appearance.reading_font() {
                                ReadingFont::NotoSerifSc => "Noto Serif SC / 思源宋体 · 默认",
                                ReadingFont::SystemSerif => "系统中文宋体 / 英文衬线字体",
                                ReadingFont::SansSerif => "系统无衬线字体",
                            },
                        )),
                )
                .child(choices)
                .child(
                    div()
                        .mt(px(6.))
                        .pt(px(8.))
                        .border_t_1()
                        .border_color(rgb(0xe3ddd2))
                        .flex()
                        .items_center()
                        .child(
                            Button::new("reset-appearance")
                                .ghost()
                                .small()
                                .px_0()
                                .label("Reset")
                                .text_color(rgb(0x326879))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.appearance = Appearance::default();
                                    this.reading_mode = ReadingMode::Original;
                                    this.original_first = true;
                                    this.original_blocks.clear();
                                    this.clear_paragraph_layouts();
                                    this.appearance_changed(cx);
                                    this.flush_session();
                                })),
                        )
                        .child(div().flex_1())
                        .child(
                            div()
                                .text_size(px(11.))
                                .text_color(rgb(0x969084))
                                .child("Esc to close"),
                        ),
                )
                .into_any_element(),
        )
    }

    fn all_notes(&self) -> Vec<Value> {
        let mut notes = annotation_panel::notes(&self.raw["reader"], &self.margin_records);
        // Legacy EasyRead questions derive their status from attached replies.
        // Preserve an explicit manual Unanswered status in the native reader.
        for note in &mut notes {
            if note["kind"] == "question"
                && !note["answered"].is_boolean()
                && self.raw["discussion"]["entries"]
                    .as_array()
                    .is_some_and(|entries| {
                        entries.iter().any(|entry| {
                            entry["reply_to"] == note["id"]
                                && entry["deleted"] != true
                                && !entry["body"].as_str().unwrap_or("").trim().is_empty()
                        })
                    })
            {
                note["answered"] = json!(true);
            }
        }
        notes
    }

    fn note_offset(&self, note: &Value, index: usize, cx: &Context<Self>) -> Pixels {
        let origin = self
            .text_views
            .get(&(index, self.primary_original()))
            .map(|(_, view)| view.read(cx).bounds().origin.y)
            .unwrap_or(px(0.));
        self.annotation_y(note, index, cx)
            .map(|y| (y - origin).max(px(0.)))
            .unwrap_or(px(0.))
    }

    fn annotation_y(&self, note: &Value, index: usize, cx: &Context<Self>) -> Option<Pixels> {
        for original in self.rendered_languages(index) {
            if let Some((_, view)) = self.text_views.get(&(index, original)) {
                let view = view.read(cx);
                let editor_anchor = self
                    .margin_editor
                    .as_ref()
                    .filter(|editor| editor.note.record["id"] == note["id"])
                    .map(|_| annotations::MARGIN_EDITOR_ANCHOR);
                for id in [
                    editor_anchor,
                    note["id"].as_str(),
                    note["source_annotation"].as_str(),
                ]
                .into_iter()
                .flatten()
                {
                    if let Some(offset) = view.annotation_offset(id) {
                        let y = view.bounds().origin.y + offset;
                        if note["pdf_figure"] == true {
                            let position = point(view.bounds().origin.x + px(4.), y + px(2.));
                            if let Some(previous) = view
                                .paragraph_at(position)
                                .and_then(|i| i.checked_sub(1))
                                .and_then(|i| view.paragraph_bounds(i))
                            {
                                return Some(previous.origin.y);
                            }
                        }
                        return Some(y);
                    }
                }
            }
        }
        None
    }

    fn jump_to_annotation(&mut self, note: Value, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self
            .state
            .paper
            .blocks
            .iter()
            .position(|block| block.id == note["anchor"])
        else {
            return;
        };
        let offset = self
            .annotation_y(&note, index, cx)
            .zip(self.block_bounds.get(&index))
            .map(|(y, bounds)| (y - bounds.origin.y - px(80.)).max(px(0.)))
            .unwrap_or(px(0.));
        let hovered = self
            .hovered_annotation
            .clone()
            .filter(|(_, block)| *block == index);
        self.select(index, window, cx);
        self.appearance_open = false;
        self.navigation = None;
        // Reveal an original-language quote when it is absent from the translated text.
        let block = &self.state.paper.blocks[index];
        let quote = note["quote"].as_str().unwrap_or("");
        let matches = |markdown: String| {
            annotations::quote_range(
                &annotations::plain_text(&annotations::html(&markdown)),
                quote,
                note["prefix"].as_str().unwrap_or(""),
                note["suffix"].as_str().unwrap_or(""),
            )
            .is_some()
        };
        if !quote.is_empty() {
            let in_original = matches(self.state.original_markdown(block));
            let in_translation = matches(self.state.markdown(block, false));
            if (self.primary_original() && !in_original && in_translation)
                || (!self.primary_original() && !in_translation && in_original)
            {
                self.paragraph_languages.remove(&index);
                self.original_blocks.insert(index, true);
            }
        }
        self.reset_rows();
        // The jump rebuilds rows but does not end the bar's actual hover.
        // Keep its preview until the pointer leaves; never retain click focus.
        self.hovered_annotation = hovered;
        self.navigation_note = Some(note.clone());
        window.blur(cx);
        self.start_annotation_scroll(
            index,
            offset,
            Duration::from_millis(380),
            Some(note),
            window,
            cx,
        );
    }

    fn cancel_annotation_scroll(&mut self) {
        self.annotation_jump_generation = self.annotation_jump_generation.wrapping_add(1);
        self.annotation_scroll = None;
        self.navigation_note = None;
    }

    fn start_annotation_scroll(
        &mut self,
        index: usize,
        offset: Pixels,
        duration: Duration,
        refine: Option<Value>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let count = self.state.paper.blocks.len();
        if count == 0 {
            return;
        }
        let current = self.list.logical_scroll_top();
        let heights = self
            .state
            .paper
            .blocks
            .iter()
            .enumerate()
            .map(|(i, block)| {
                self.block_bounds
                    .get(&i)
                    .map(|bounds| f32::from(bounds.size.height))
                    .unwrap_or_else(|| {
                        // Unpainted rows have no geometry. Use prose estimates until
                        // they enter the viewport, then refine the destination below.
                        let text = self.state.markdown(block, false);
                        let lines: f32 = text
                            .lines()
                            .map(|line| {
                                let chars: f32 = line
                                    .chars()
                                    .map(|c| if c.is_ascii() { 0.5 } else { 1. })
                                    .sum();
                                (chars / self.appearance.line_width).ceil().max(1.)
                            })
                            .sum();
                        56. + lines * self.appearance.font_size * self.appearance.line_spacing
                            + if block.kind == "figure" {
                                self.appearance.text_width() * 0.65
                            } else {
                                0.
                            }
                    })
                    .max(1.)
            })
            .collect();
        self.annotation_scroll = Some(AnnotationScroll {
            path: ScrollPath::new(
                heights,
                ScrollPosition {
                    block: current.item_ix.min(count - 1),
                    offset: f32::from(current.offset_in_item).max(0.),
                },
                ScrollPosition {
                    block: index,
                    offset: f32::from(offset).max(0.),
                },
            ),
            started: Instant::now(),
            duration,
            generation: self.annotation_jump_generation,
            refine,
        });
        self.schedule_annotation_frame(window, cx);
        cx.notify();
    }

    fn schedule_annotation_frame(&self, window: &mut Window, cx: &Context<Self>) {
        let weak = cx.entity().downgrade();
        let generation = self.annotation_jump_generation;
        window.on_next_frame(move |window, cx| {
            let _ = weak.update(cx, |this, cx| {
                if this.annotation_jump_generation == generation {
                    this.tick_annotation_scroll(window, cx);
                }
            });
        });
    }

    fn tick_annotation_scroll(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(motion) = self.annotation_scroll.take() else {
            return;
        };
        let progress = motion.started.elapsed().as_secs_f32() / motion.duration.as_secs_f32();
        let position = motion.path.sample(progress);
        self.list.scroll_to(ListOffset {
            item_ix: position.block,
            offset_in_item: px(position.offset),
        });
        if progress < 1. {
            self.annotation_scroll = Some(motion);
            self.schedule_annotation_frame(window, cx);
        } else if let Some(note) = motion.refine {
            self.refine_annotation_scroll(note, motion.generation, window, cx);
        } else {
            self.capture_position();
        }
        cx.notify();
    }

    fn refine_annotation_scroll(
        &self,
        note: Value,
        generation: usize,
        window: &mut Window,
        cx: &Context<Self>,
    ) {
        let weak = cx.entity().downgrade();
        // Virtualized rows have no current quote geometry until they have painted.
        window.on_next_frame(move |window, _| {
            window.on_next_frame(move |window, cx| {
                let _ = weak.update(cx, |this, cx| {
                    if this.annotation_jump_generation != generation {
                        return;
                    }
                    let index = this.selected;
                    let target =
                        json!({"id":"reader-navigation-target", "pdf_figure":note["pdf_figure"]});
                    let offset = this
                        .annotation_y(&target, index, cx)
                        .or_else(|| this.annotation_y(&note, index, cx))
                        .zip(this.block_bounds.get(&index))
                        .map(|(y, bounds)| (y - bounds.origin.y - px(80.)).max(px(0.)))
                        .unwrap_or(px(0.));
                    // The geometry anchor is only needed for this jump; restore
                    // all of the source annotation's normal click behavior.
                    this.navigation_note = None;
                    this.start_annotation_scroll(
                        index,
                        offset,
                        Duration::from_millis(160),
                        None,
                        window,
                        cx,
                    );
                });
            });
        });
    }

    fn annotation_jump_bar(&self, note: &Value, short: bool, cx: &mut Context<Self>) -> Button {
        let record = note.clone();
        let bar_id = format!(
            "annotation-jump-{}-{}",
            if short { "quote" } else { "card" },
            note["id"].as_str().unwrap_or("")
        );
        let hover_id = bar_id.clone();
        let index = self
            .state
            .paper
            .blocks
            .iter()
            .position(|block| block.id == note["anchor"]);
        Button::new(SharedString::from(bar_id))
            // Text buttons keep normal, hover and pressed backgrounds transparent.
            .text()
            .absolute()
            .left(px(-5.))
            .top_0()
            .h_full()
            .w(px(12.))
            .p_0()
            .rounded_none()
            .accessibility_label(if short {
                "Go to highlighted passage"
            } else {
                "Go to annotation passage"
            })
            .disabled(index.is_none())
            .on_hover(cx.listener(move |this, hovered, _, cx| {
                if *hovered {
                    this.hovered_annotation = index.map(|index| (hover_id.clone(), index));
                } else if this
                    .hovered_annotation
                    .as_ref()
                    .is_some_and(|(id, _)| *id == hover_id)
                {
                    this.hovered_annotation = None;
                }
                cx.notify();
            }))
            .on_click(cx.listener(move |this, _, window, cx| {
                cx.stop_propagation();
                this.jump_to_annotation(record.clone(), window, cx);
            }))
    }

    fn show_panel(&mut self, panel: Panel, window: &mut Window, cx: &mut Context<Self>) {
        if panel == Panel::Edit {
            self.begin_inline_edit(self.selected, window, cx);
            return;
        }
        if (panel == Panel::Chat && !self.settings.committed.features.ask_ai)
            || (panel == Panel::Edit && !self.settings.committed.features.edit)
        {
            return;
        }
        if panel == Panel::Chat {
            if self.panel_open && self.panel == Panel::Chat {
                self.close_chat(window, cx);
            } else {
                self.open_chat(window, cx);
            }
            return;
        }
        self.chat_ui.closing = false;
        self.chat_ui.generation = self.chat_ui.generation.wrapping_add(1);
        self.appearance_open = false;
        self.navigation = None;
        self.queue_margin_note();
        self.margin_editor = None;
        self.panel_open = !self.panel_open || self.panel != panel;
        self.panel = panel;
        self.selection = None;
        self.annotation_popup = None;
        window.blur(cx);
        self.reset_rows();
        cx.notify();
    }

    fn open_note(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.queue_margin_note();
        self.hovered_note_card = None;
        let Some(record) = self.all_notes().into_iter().find(|note| note["id"] == id) else {
            return;
        };
        let Some(block) = self
            .state
            .paper
            .blocks
            .iter()
            .position(|block| block.id == record["anchor"])
        else {
            return;
        };
        let keep_position =
            !self.panel_open && !self.show_pdf && self.list.logical_scroll_top().item_ix == block;
        let quote_offset = self.note_offset(&record, block, cx);
        self.select(block, window, cx);
        self.panel_open = false;
        self.show_pdf = false;
        if !keep_position {
            self.list.scroll_to(ListOffset {
                item_ix: block,
                offset_in_item: (quote_offset - px(80.)).max(px(0.)),
            });
        }
        if !annotation_panel::personal(&record) {
            self.annotation_popup = Some((id.to_owned(), point(px(400.), px(280.))));
        } else {
            let selection = Selection {
                block,
                key: record["key"]
                    .as_str()
                    .unwrap_or(&self.state.paper.blocks[block].id)
                    .to_owned(),
                quote: record["quote"].as_str().unwrap_or("").to_owned(),
                prefix: record["prefix"].as_str().unwrap_or("").to_owned(),
                suffix: record["suffix"].as_str().unwrap_or("").to_owned(),
                position: point(px(0.), px(0.)),
                underline: record["style"] == "underline",
            };
            self.margin_input.update(cx, |state, cx| {
                state.set_value(record["body"].as_str().unwrap_or("").to_owned(), window, cx)
            });
            let mut note = MarginNote::new(record.clone());
            note.queued(record.clone());
            note.acknowledge(record, true);
            self.margin_editor = Some(MarginEditor {
                selection,
                note,
                changed_at: Instant::now(),
                placement_pending: Some(Instant::now()),
            });
            self.place_margin_editor(window, cx);
        }
        self.reset_rows();
        cx.notify();
    }

    fn add_paragraph_note(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(block) = self.state.paper.blocks.get(self.selected) else {
            return;
        };
        let selection = Selection {
            block: self.selected,
            key: block.id.clone(),
            quote: String::new(),
            prefix: String::new(),
            suffix: String::new(),
            position: point(px(0.), px(0.)),
            underline: false,
        };
        self.open_margin_note(selection, "note", HighlightColor::Yellow, window, cx);
    }

    fn ask_annotation_ai(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || self.chat_ui.streaming {
            return;
        }
        self.queue_margin_note();
        let Some(note) = self.all_notes().into_iter().find(|note| note["id"] == id) else {
            return;
        };
        self.chat_ui.refs.clear();
        self.add_chat_reference(ai::ChatReference {
            anchor: note["anchor"].as_str().unwrap_or("").to_owned(),
            quote: note["quote"].as_str().unwrap_or("").to_owned(),
        });
        self.open_chat(window, cx);
        let question = note["kind"] == "question";
        if question && annotation_panel::answered(&note) {
            // Already answered: open a follow-up with the original context,
            // without re-sending the same question or erasing its reply.
            self.chat_ui.pending_note = None;
            self.question
                .update(cx, |input, cx| input.set_value("", window, cx));
            cx.notify();
            return;
        }
        let text = if question {
            note["body"].as_str().unwrap_or("").to_owned()
        } else {
            format!(
                "这是我读这里时写的笔记，请点评：我理解得对不对、有没有漏掉或想错的地方、还可以往哪想。\n\n我的笔记：{}",
                note["body"].as_str().unwrap_or("")
            )
        };
        self.chat_ui.pending_note = Some(id.to_owned());
        self.question
            .update(cx, |input, cx| input.set_value(text, window, cx));
        // Note-card actions send immediately, as in EasyRead's askModel.
        // Missing model credentials leave the draft and reference available to retry.
        self.ask(window, cx);
        cx.notify();
    }

    fn toggle_note_kind(&mut self, id: &str, cx: &mut Context<Self>) {
        if let Some(mut note) = self.all_notes().into_iter().find(|note| note["id"] == id) {
            note["kind"] = json!(if note["kind"] == "question" {
                "note"
            } else {
                "question"
            });
            // Preserve the answer/status when switching back and forth.
            if note["kind"] == "question" && !note["answered"].is_boolean() {
                note["answered"] = json!(annotation_panel::answered(&note));
            }
            note["updated"] = json!(now());
            self.save_ops(vec![json!({"op":"note","note":note})], cx);
        }
    }

    fn toggle_question_answered(&mut self, id: &str, cx: &mut Context<Self>) {
        if let Some(editor) = &mut self.margin_editor
            && editor.note.record["id"] == id
            && editor.note.record["kind"] == "question"
        {
            editor.note.record["answered"] =
                json!(!annotation_panel::answered(&editor.note.record));
            editor.note.record["updated"] = json!(now());
            editor.changed_at = Instant::now();
            self.reset_rows();
            cx.notify();
            return;
        }
        if let Some(mut note) = self
            .all_notes()
            .into_iter()
            .find(|note| note["id"] == id && note["kind"] == "question")
        {
            note["answered"] = json!(!annotation_panel::answered(&note));
            note["updated"] = json!(now());
            self.save_ops(vec![json!({"op":"note","note":note})], cx);
        }
    }

    fn annotation_replies(&self, id: &str, cx: &mut Context<Self>) -> Option<AnyElement> {
        let replies: Vec<_> = self.raw["discussion"]["entries"]
            .as_array()?
            .iter()
            .filter(|entry| entry["reply_to"] == id && entry["deleted"] != true)
            .cloned()
            .collect();
        if replies.is_empty() {
            return None;
        }
        Some(
            div()
                .flex()
                .flex_col()
                .gap(px(10.))
                .w_full()
                .children(
                    replies
                        .iter()
                        .map(|entry| self.discussion_card(entry, false, cx)),
                )
                .into_any_element(),
        )
    }

    fn discussion_card(&self, entry: &Value, drawer: bool, cx: &mut Context<Self>) -> AnyElement {
        let body = entry["body"].as_str().unwrap_or("").to_owned();
        let copy = body.clone();
        let id = entry["id"].as_str().unwrap_or("ai-entry").to_owned();
        div()
            .debug_selector(|| "annotation-ai-reply".into())
            .relative()
            .w_full()
            .border_l_2()
            .border_color(rgb(0x387487))
            .pl(px(12.))
            .py(px(4.))
            .when(drawer, |d| {
                d.child(self.annotation_jump_bar(entry, false, cx))
            })
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(rgb(0x387487))
                    .child(format!("AI · {}", entry["by"].as_str().unwrap_or("AI"))),
            )
            .child(
                self.rich_markdown(format!("reply-body-{id}"), &body, 240., cx)
                    .selectable(true)
                    .text_size(px(13.5))
                    .line_height(relative(1.72)),
            )
            .child(
                Button::new(SharedString::from(format!("reply-copy-{id}")))
                    .ghost()
                    .xsmall()
                    .label("复制")
                    .on_click(move |_, _, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(copy.clone()))
                    }),
            )
            .into_any_element()
    }

    fn annotation_header(&self, note: &Value) -> AnyElement {
        let answered = annotation_panel::answered(note);
        div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(6.))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(6.))
                    .text_size(px(12.))
                    .text_color(rgb(0xaa681c))
                    .child(annotation_panel::label(note))
                    .when(note["kind"] == "question", |d| {
                        d.child(
                            div()
                                .debug_selector(move || {
                                    if answered {
                                        "annotation-answered"
                                    } else {
                                        "annotation-unanswered"
                                    }
                                    .into()
                                })
                                .rounded(px(8.))
                                .px(px(5.))
                                .text_size(px(10.))
                                .text_color(rgb(if answered { 0x578164 } else { 0xaa681c }))
                                .bg(rgba(if answered { 0x57816415 } else { 0xaa681c15 }))
                                .child(if answered { "Answered" } else { "Unanswered" }),
                        )
                    }),
            )
            .when_some(annotation_panel::timestamp(note), |d, timestamp| {
                d.child(
                    div()
                        .debug_selector(|| "annotation-time".into())
                        .flex_shrink_0()
                        .text_size(px(12.))
                        .text_color(rgb(0x969084))
                        .child(timestamp),
                )
            })
            .into_any_element()
    }

    fn delete_annotation(&mut self, id: &str, cx: &mut Context<Self>) {
        if self
            .margin_editor
            .as_ref()
            .is_some_and(|editor| editor.note.record["id"] == id)
        {
            self.queue_margin_note();
            self.margin_editor = None;
        }
        self.margin_records.remove(id);
        self.save_ops(vec![json!({"op":"note_del","id":id,"at":now()})], cx);
    }

    fn set_hovered_note_card(&mut self, id: Option<String>, cx: &mut Context<Self>) {
        if self.hovered_note_card != id {
            self.hovered_note_card = id;
            // Rebuild the cached reader rows in both layouts, without changing
            // text measurements, EPUB pagination or the current scroll offset.
            let offset = self.list.logical_scroll_top();
            self.list.reset(self.state.paper.blocks.len());
            self.list.scroll_to(offset);
            cx.notify();
        }
    }

    fn annotation_card(&self, note: &Value, drawer: bool, cx: &mut Context<Self>) -> AnyElement {
        let id = note["id"].as_str().unwrap_or("").to_owned();
        let weak = cx.entity().downgrade();
        let measure_id = id.clone();
        let personal = annotation_panel::personal(note);
        let quote = note["quote"].as_str().unwrap_or("").to_owned();
        let body = note["body"].as_str().unwrap_or("").to_owned();
        let color = HighlightColor::from_name(note["color"].as_str().unwrap_or("yellow"));
        let edit_id = id.clone();
        let ai_id = id.clone();
        let ai_background: Hsla = rgba(0x38748712).into();
        let question_id = id.clone();
        let answer_id = id.clone();
        let question = note["kind"] == "question";
        let answered = annotation_panel::answered(note);
        let delete_id = id.clone();
        let hover_id = id.clone();
        let move_id = id.clone();
        let show_actions = self.hovered_note_card.as_ref() == Some(&id);
        div()
            .px(px(if drawer { 0. } else { 12. }))
            .min_w_0()
            .child(
                div()
                    .id(SharedString::from(format!("annotation-card-{id}")))
                    // Reveal only on mouse movement, not when a saved card
                    // replaces the editor underneath a stationary pointer.
                    .on_mouse_move(cx.listener(move |this, _, _, cx| {
                        this.set_hovered_note_card(Some(move_id.clone()), cx);
                    }))
                    .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                        if !*hovered && this.hovered_note_card.as_ref() == Some(&hover_id) {
                            this.set_hovered_note_card(None, cx);
                        }
                    }))
                    .relative()
                    .border_l_2()
                    .border_color(rgb(0xaa681c))
                    .pl(px(12.))
                    .py(px(4.))
                    .flex()
                    .flex_col()
                    .items_start()
                    .gap(px(8.))
                    // Let the window selection layer begin intentional drags
                    // in the saved body. Keep release out of reader-only tools.
                    .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .when(drawer, |d| {
                        d.child(self.annotation_jump_bar(note, false, cx))
                    })
                    .child(
                        canvas(
                            move |bounds, _, cx| {
                                let _ = weak.update(cx, |this, cx| {
                                    if this.margin_card_heights.get(&measure_id).is_none_or(|old| {
                                        (*old - bounds.size.height).abs() > px(0.5)
                                    }) {
                                        this.margin_card_heights
                                            .insert(measure_id.clone(), bounds.size.height);
                                        cx.notify();
                                    }
                                });
                            },
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .size_full(),
                    )
                    .child(self.annotation_header(note))
                    .when(!quote.is_empty(), |d| {
                        d.child(
                            div()
                                .debug_selector(|| "annotation-card-quote".into())
                                .relative()
                                .w_full()
                                .text_size(px(13.))
                                .font_family(self.fonts.serif_en.clone())
                                .line_height(relative(1.6))
                                .text_color(rgb(0x969084))
                                .border_l_2()
                                .border_color(rgb(color.rgb()))
                                .pl(px(4.))
                                .when(drawer, |d| {
                                    d.child(self.annotation_jump_bar(note, true, cx))
                                })
                                .child(
                                    self.rich_markdown(
                                        format!("annotation-quote-{id}"),
                                        &quote,
                                        240.,
                                        cx,
                                    )
                                    .selectable(true)
                                    .w_full()
                                    .text_size(px(13.))
                                    .font_family(self.fonts.serif_en.clone())
                                    .line_height(relative(1.6))
                                    .text_color(rgb(0x969084)),
                                ),
                        )
                    })
                    .when(!body.is_empty(), |d| {
                        d.child(
                            div()
                                .debug_selector(|| "annotation-card-body".into())
                                .w_full()
                                .child(
                                    self.rich_markdown(
                                        format!("annotation-body-{id}"),
                                        &body,
                                        240.,
                                        cx,
                                    )
                                    .selectable(true)
                                    .w_full()
                                    .text_size(px(14.))
                                    .text_color(rgb(0x625e55)),
                                ),
                        )
                    })
                    .when_some(self.annotation_replies(&id, cx), |d, replies| {
                        d.child(replies)
                    })
                    .when(personal && !body.trim().is_empty(), |d| {
                        d.when(self.settings.committed.features.ask_ai, |d| {
                            d.child(
                                Button::new(SharedString::from(format!("annotation-ai-{id}")))
                                    .debug_selector(move || {
                                        if question {
                                            "annotation-ai-answer"
                                        } else {
                                            "annotation-ai-feedback"
                                        }
                                        .into()
                                    })
                                    .custom(
                                        ButtonCustomVariant::new(cx)
                                            .color(ai_background)
                                            .foreground(rgb(0x387487).into())
                                            .hover(ai_background)
                                            .active(ai_background),
                                    )
                                    // Custom variants soften their idle color; keep
                                    // the same surface in idle, hover and pressed states.
                                    .bg(ai_background)
                                    .xsmall()
                                    .h(px(26.))
                                    .w(px(184.))
                                    .max_w_full()
                                    .px(px(10.))
                                    .rounded(px(13.))
                                    .text_size(px(12.))
                                    .label(if question {
                                        "✳  Ask AI to answer"
                                    } else {
                                        "✳  Ask AI for feedback"
                                    })
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.ask_annotation_ai(&ai_id, window, cx)
                                    })),
                            )
                        })
                    })
                    .child(
                        // Reserve the toolbar height so hover cannot move the
                        // card (or the following notes) under the pointer.
                        div().when(!show_actions, |d| d.invisible()).child(
                            div()
                                .debug_selector(|| "annotation-card-actions".into())
                                .flex()
                                .flex_wrap()
                                .gap(px(6.))
                                .child(
                                    Button::new(SharedString::from(format!(
                                        "annotation-edit-{id}"
                                    )))
                                    .ghost()
                                    .xsmall()
                                    .h(px(22.))
                                    .px(px(2.))
                                    .text_size(px(11.))
                                    .text_color(rgb(0x969084))
                                    .label("Edit")
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.open_note(&edit_id, window, cx)
                                    })),
                                )
                                .child(
                                    Button::new(SharedString::from(format!(
                                        "annotation-question-{id}"
                                    )))
                                    .ghost()
                                    .xsmall()
                                    .h(px(22.))
                                    .px(px(2.))
                                    .text_size(px(11.))
                                    .text_color(rgb(0x969084))
                                    .debug_selector(move || {
                                        if question {
                                            "annotation-make-note"
                                        } else {
                                            "annotation-make-question"
                                        }
                                        .into()
                                    })
                                    .label(if question {
                                        "Make it a note"
                                    } else {
                                        "Make it a question"
                                    })
                                    .disabled(self.busy)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.toggle_note_kind(&question_id, cx)
                                    })),
                                )
                                .when(question, |d| {
                                    d.child(
                                        Button::new(SharedString::from(format!(
                                            "annotation-answer-{id}"
                                        )))
                                        .ghost()
                                        .xsmall()
                                        .h(px(22.))
                                        .px(px(2.))
                                        .text_size(px(11.))
                                        .text_color(rgb(0x969084))
                                        .debug_selector(move || {
                                            if answered {
                                                "annotation-mark-unanswered"
                                            } else {
                                                "annotation-mark-answered"
                                            }
                                            .into()
                                        })
                                        .label(if answered {
                                            "Mark as unanswered"
                                        } else {
                                            "Mark as answered"
                                        })
                                        .disabled(self.busy)
                                        .on_click(
                                            cx.listener(move |this, _, _, cx| {
                                                this.toggle_question_answered(&answer_id, cx)
                                            }),
                                        ),
                                    )
                                })
                                .child(
                                    Button::new(SharedString::from(format!(
                                        "annotation-delete-{id}"
                                    )))
                                    .ghost()
                                    .xsmall()
                                    .h(px(22.))
                                    .px(px(2.))
                                    .text_size(px(11.))
                                    .text_color(rgb(0x969084))
                                    .label("Delete")
                                    .disabled(self.busy)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.delete_annotation(&delete_id, cx)
                                    })),
                                ),
                        ),
                    ),
            )
            .into_any_element()
    }

    fn notes(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut notes = self.all_notes();
        let discussion = self.raw["discussion"]["entries"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        for (i, mut entry) in discussion.into_iter().enumerate() {
            if entry.is_object()
                && !entry["deleted"].as_bool().unwrap_or(false)
                && entry["kind"] != "reply"
            {
                if !entry["id"].is_string() {
                    entry["id"] = json!(format!("ai-entry-{i}"));
                }
                entry["kind"] = json!("ai");
                notes.push(entry);
            }
        }
        notes.sort_by_key(|note| {
            (
                self.state
                    .paper
                    .blocks
                    .iter()
                    .position(|block| block.id == note["anchor"])
                    .unwrap_or(usize::MAX),
                note["created"].as_str().unwrap_or("").to_owned(),
                note["id"].as_str().unwrap_or("").to_owned(),
            )
        });
        let count = notes.len();
        let mut groups: Vec<(String, Vec<Value>)> = Vec::new();
        for note in notes
            .into_iter()
            .filter(|note| annotation_panel::matches(note, self.annotation_filter))
        {
            let index = self
                .state
                .paper
                .blocks
                .iter()
                .position(|block| block.id == note["anchor"]);
            let heading = index
                .and_then(|index| {
                    self.state.paper.blocks[..=index]
                        .iter()
                        .rev()
                        .find(|block| block.kind == "heading")
                })
                .map(|block| {
                    annotations::plain_text(&annotations::html(&self.state.markdown(block, false)))
                })
                .unwrap_or_default();
            let page = index
                .map(|index| self.state.paper.blocks[index].page)
                .unwrap_or(0);
            let title = if page > 0 {
                format!(
                    "Page {page}{}{}",
                    if heading.is_empty() { "" } else { " · " },
                    heading
                )
            } else {
                "Other annotations".into()
            };
            if groups.last().is_none_or(|(last, _)| *last != title) {
                groups.push((title, Vec::new()));
            }
            groups.last_mut().unwrap().1.push(note);
        }
        div()
            .flex()
            .flex_col()
            .gap(px(18.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(
                        div()
                            .flex()
                            .p(px(2.))
                            .rounded(px(9.))
                            .bg(rgb(0xeee9df))
                            .children(
                                [
                                    (false, format!("Annotations {count}")),
                                    (true, "Paper notes".to_owned()),
                                ]
                                .map(|(paper, label)| {
                                    Button::new(SharedString::from(format!(
                                        "annotation-tab-{paper}"
                                    )))
                                    .ghost()
                                    .xsmall()
                                    .h(px(28.))
                                    .px(px(10.))
                                    .text_size(px(12.))
                                    .text_color(rgb(if paper == self.paper_notes_tab {
                                        0x5f5a51
                                    } else {
                                        0x969084
                                    }))
                                    .when(paper == self.paper_notes_tab, |b| {
                                        b.bg(rgb(0xfbfaf7)).shadow(vec![
                                            BoxShadow::new(px(0.), px(1.), rgba(0x52452c12).into())
                                                .blur_radius(px(2.)),
                                        ])
                                    })
                                    .label(label)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.paper_notes_tab = paper;
                                        cx.notify();
                                    }))
                                }),
                            ),
                    )
                    .child(div().flex_1())
                    .child(
                        Button::new("export-annotations")
                            .ghost()
                            .xsmall()
                            .h(px(28.))
                            .px(px(4.))
                            .text_size(px(12.))
                            .label("Export")
                            .on_click(cx.listener(|this, _, _, cx| this.export_annotations(cx))),
                    )
                    .child(
                        Button::new("close-notes-panel")
                            .ghost()
                            .xsmall()
                            .h(px(28.))
                            .w(px(22.))
                            .p_0()
                            .label("×")
                            .accessibility_label("Close Notes")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.show_panel(Panel::Notes, window, cx)
                            })),
                    ),
            )
            .when(!self.paper_notes_tab, |d| {
                d.child(
                    div().flex().flex_wrap().gap(px(5.)).children(
                        [
                            (AnnotationFilter::All, "All"),
                            (AnnotationFilter::MyNote, "My note"),
                            (AnnotationFilter::Highlights, "Highlights"),
                            (AnnotationFilter::Ai, "AI"),
                            (AnnotationFilter::Unanswered, "Unanswered"),
                        ]
                        .into_iter()
                        .enumerate()
                        .map(|(i, (filter, label))| {
                            Button::new(("annotation-filter", i))
                                .ghost()
                                .xsmall()
                                .h(px(24.))
                                .px(px(9.))
                                .rounded(px(12.))
                                .border_1()
                                .border_color(rgb(if self.annotation_filter == filter {
                                    0x77716a
                                } else {
                                    0xe3ddd2
                                }))
                                .text_size(px(11.))
                                .text_color(rgb(if self.annotation_filter == filter {
                                    0x5f5a51
                                } else {
                                    0x969084
                                }))
                                .label(label)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.annotation_filter = filter;
                                    cx.notify();
                                }))
                        }),
                    ),
                )
                .child(
                    Button::new("add-paragraph-note")
                        .ghost()
                        .xsmall()
                        .h(px(28.))
                        .px(px(10.))
                        .border_1()
                        .border_color(rgb(0xd4cbbb))
                        .rounded(px(7.))
                        .text_size(px(12.))
                        .w(px(300.))
                        .max_w_full()
                        .label("+  Add a note to the current paragraph")
                        .on_click(
                            cx.listener(|this, _, window, cx| this.add_paragraph_note(window, cx)),
                        ),
                )
                .when(groups.is_empty(), |d| {
                    d.child(
                        div()
                            .text_size(px(13.))
                            .text_color(rgb(0x969084))
                            .child("No annotations in this filter."),
                    )
                })
                .children(groups.into_iter().map(|(title, notes)| {
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(12.))
                        .child(
                            div()
                                .pb(px(7.))
                                .border_b_1()
                                .border_color(rgb(0xe3ddd2))
                                .text_size(px(11.))
                                .text_color(rgb(0x969084))
                                .child(title),
                        )
                        .children(notes.iter().map(|note| {
                            if note["kind"] == "ai" {
                                div()
                                    .relative()
                                    .border_l_2()
                                    .border_color(rgb(0x387487))
                                    .pl(px(12.))
                                    .flex()
                                    .flex_col()
                                    .gap(px(6.))
                                    .child(self.annotation_jump_bar(note, false, cx))
                                    .child(
                                        div()
                                            .text_size(px(12.))
                                            .text_color(rgb(0x387487))
                                            .child("AI"),
                                    )
                                    .child(
                                        TextView::markdown(
                                            SharedString::from(format!(
                                                "discussion-{}",
                                                note["id"].as_str().unwrap_or("")
                                            )),
                                            note["body"].as_str().unwrap_or("").to_owned(),
                                        )
                                        .selectable(true),
                                    )
                                    .into_any_element()
                            } else {
                                if note["kind"] == "ai" {
                                    self.discussion_card(note, true, cx)
                                } else {
                                    self.annotation_card(note, true, cx)
                                }
                            }
                        }))
                }))
            })
            .when(self.paper_notes_tab, |d| {
                d.child(
                    div()
                        .text_size(px(13.))
                        .text_color(rgb(0x969084))
                        .child("Notes about the whole book"),
                )
                .child(Textarea::new(&self.paper_note_input).h(px(300.)))
                .child(
                    Button::new("save-paper-note")
                        .ghost()
                        .xsmall()
                        .border_1()
                        .border_color(rgb(0xd4cbbb))
                        .label("Save paper notes")
                        .disabled(self.busy || self.workspace.is_none())
                        .on_click(cx.listener(|this, _, _, cx| {
                            let body = this.paper_note_input.read(cx).value().to_string();
                            this.save_ops(
                                vec![json!({"op":"paper_note","body":body,"at":now()})],
                                cx,
                            );
                        })),
                )
            })
            .into_any_element()
    }

    fn export_annotations(&mut self, cx: &mut Context<Self>) {
        let mut text = format!("# {}\n\n", self.state.title());
        for note in self.all_notes() {
            text.push_str(&format!(
                "## {}\n\n> {}\n\n{}\n\n",
                annotation_panel::label(&note),
                note["quote"].as_str().unwrap_or(""),
                note["body"].as_str().unwrap_or("")
            ));
        }
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        self.status = "已将全部批注导出为 Markdown 并复制到剪贴板".into();
        cx.notify();
    }
}
impl Render for ReaderView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.settings.translation_confirm {
            return self.translation_confirmation_view(cx);
        }
        let background = self.main_view(window, cx);
        if !self.settings.open {
            return background;
        }
        let settings = self.settings_modal(window, cx);
        div()
            .id("reader-with-settings")
            .relative()
            .size_full()
            .child(background)
            .child(settings)
            .into_any_element()
    }
}
impl ReaderView {
    fn open_image_preview(
        &mut self,
        image: Arc<Image>,
        aspect: f32,
        width: u32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.selection = None;
        self.annotation_popup = None;
        self.appearance_open = false;
        self.navigation = None;
        TextSelection::clear(window, cx);
        self.reader_focus.focus(window, cx);
        self.image_preview = Some(ImagePreview {
            image,
            aspect,
            width,
            zoom: 1.,
            scroll: ScrollHandle::new(),
        });
        cx.notify();
    }

    fn image_preview_view(&self, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let preview = self.image_preview.as_ref()?;
        let available = window.viewport_size();
        let viewport_width = (f32::from(available.width) - 96.).max(1.);
        let viewport_height = (f32::from(available.height) - 144.).max(1.);
        let fit_width = viewport_width
            .min(viewport_height * preview.aspect)
            .min(preview.width as f32);
        let width = fit_width * preview.zoom;
        let height = width / preview.aspect;
        Some(
            div()
                .id("image-preview-overlay")
                .absolute()
                .inset_0()
                .size_full()
                .bg(rgba(0x19171399))
                .p(px(24.))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
                .on_click(cx.listener(|this, _, _, cx| {
                    this.image_preview = None;
                    cx.notify();
                }))
                .child(
                    div()
                        .id("image-preview")
                        .size_full()
                        .flex()
                        .flex_col()
                        .min_h_0()
                        .bg(rgb(0xf6f3ec))
                        .rounded(px(10.))
                        .overflow_hidden()
                        .on_click(|_, _, cx| cx.stop_propagation())
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .p(px(12.))
                                .child(div().flex_1().text_size(px(14.)).child("图片预览"))
                                .child(
                                    Button::new("image-zoom-out")
                                        .ghost()
                                        .label("−")
                                        .accessibility_label("缩小图片")
                                        .disabled(preview.zoom <= 1.)
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            if let Some(p) = this.image_preview.as_mut() {
                                                p.zoom = (p.zoom / 1.5).max(1.);
                                            }
                                            cx.notify();
                                        })),
                                )
                                .child(
                                    Button::new("image-zoom-fit")
                                        .ghost()
                                        .label(format!("{:.0}%", preview.zoom * 100.))
                                        .accessibility_label("适合窗口")
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            if let Some(p) = this.image_preview.as_mut() {
                                                p.zoom = 1.;
                                                p.scroll.set_offset(point(px(0.), px(0.)));
                                            }
                                            cx.notify();
                                        })),
                                )
                                .child(
                                    Button::new("image-zoom-in")
                                        .ghost()
                                        .label("+")
                                        .accessibility_label("放大图片")
                                        .disabled(preview.zoom >= 4.)
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            if let Some(p) = this.image_preview.as_mut() {
                                                p.zoom = (p.zoom * 1.5).min(4.);
                                            }
                                            cx.notify();
                                        })),
                                )
                                .child(
                                    Button::new("image-preview-close")
                                        .ghost()
                                        .label("×")
                                        .accessibility_label("关闭图片预览")
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.image_preview = None;
                                            this.reader_focus.focus(window, cx);
                                            cx.notify();
                                        })),
                                ),
                        )
                        .child(
                            div()
                                .id("image-preview-scroll")
                                .flex_1()
                                .min_h_0()
                                .w_full()
                                .overflow_scroll()
                                .track_scroll(&preview.scroll)
                                .child(
                                    div()
                                        .flex()
                                        .justify_center()
                                        .items_center()
                                        .min_w_full()
                                        .min_h_full()
                                        .w(px(width + 48.))
                                        .h(px(height + 48.))
                                        .child(
                                            img(preview.image.clone()).w(px(width)).h(px(height)),
                                        ),
                                ),
                        ),
                )
                .into_any_element(),
        )
    }

    fn main_view(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        if self.home.visible {
            return self.home_view(window, cx);
        }
        if !self.settings.open
            && (window.focused(cx).is_none()
                || self.home.focus.is_focused(window)
                || self.settings.focus.is_focused(window))
        {
            self.reader_focus.focus(window, cx);
        }
        for (slider, value) in [
            (&self.font_slider, self.appearance.font_size),
            (&self.width_slider, self.appearance.line_width),
            (&self.spacing_slider, self.appearance.line_spacing),
        ] {
            if (slider.read(cx).value().end() - value).abs() > 0.001 {
                slider.update(cx, |slider, cx| slider.set_value(value, window, cx));
            }
        }
        let reading_order_menu = self.reading_order_menu(window, cx);
        let appearance_menu = self.appearance_menu(window, cx);
        let appearance_will_open = !self.appearance_open;
        if self.restore_inputs {
            self.fill_editor(window, cx);
        }
        let toolbar = self.selection_toolbar(window, cx);
        let annotation_popup = self.annotation_editor(window, cx);
        if self.restore_paper_note {
            self.fill_paper_note(window, cx);
            self.restore_paper_note = false;
        }
        let margin_open = self.margin_editor.is_some();
        let weak = cx.entity().downgrade();
        let navigation_menu = self.navigation_menu(window, cx);
        let reading_index = self.list.logical_scroll_top().item_ix;
        self.update_pdf_focus(reading_index);
        let chapter = self
            .contents()
            .into_iter()
            .rfind(|(i, _, _)| *i <= reading_index)
            .map(|(_, title, _)| title)
            .unwrap_or_else(|| {
                self.state
                    .paper
                    .blocks
                    .get(reading_index)
                    .map(|b| format!("第 {} 页", b.page.max(1)))
                    .unwrap_or_default()
            });
        let title = self.state.title().to_owned();
        let show_status = !matches!(
            self.status.as_str(),
            "" | "就绪 · 草稿与阅读位置自动保存"
                | "论文已打开 · 已恢复草稿与阅读位置"
                | "公式已编译"
                | "笔记已自动保存"
        );
        window.set_window_title(&if title.is_empty() {
            "Vellio".to_owned()
        } else {
            format!("{title} · Vellio")
        });
        let panel = match self.panel {
            Panel::Notes => self.notes(cx),
            Panel::Edit => div()
                .flex()
                .flex_col()
                .gap_3()
                .child("修订译文")
                .when(self.draft_conflict, |d| {
                    d.child(
                        div()
                            .text_color(rgb(0xa34f21))
                            .child("正文已更新；已保留旧草稿，请核对后保存。"),
                    )
                })
                .child(Textarea::new(&self.editor).h(px(280.)))
                .child(
                    Button::new("save-edit")
                        .primary()
                        .label("保存修订")
                        .disabled(self.busy || self.workspace.is_none())
                        .on_click(cx.listener(|this, _, _, cx| this.save_edit(false, cx))),
                )
                .child(
                    Button::new("revert-edit")
                        .label("恢复译者稿")
                        .disabled(self.busy || self.workspace.is_none())
                        .on_click(cx.listener(|this, _, _, cx| this.save_edit(true, cx))),
                )
                .into_any_element(),
            Panel::Chat => div().into_any_element(),
        };
        let page_view = self.page_view(cx);
        let image_preview = self.image_preview_view(window, cx);
        div()
            .id("reader")
            .text_size(px(15.))
            .line_height(relative(typography::UI_LINE_HEIGHT))
            .track_focus(&self.reader_focus)
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(READER_BACKGROUND))
            .text_color(rgb(typography::READER_INK))
            .capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if this.inline_edit.is_some() {
                    if event.keystroke.key == "escape" {
                        this.cancel_inline_edit(window, cx);
                        cx.stop_propagation();
                    } else if event.keystroke.key == "enter"
                        && (event.keystroke.modifiers.control || event.keystroke.modifiers.platform)
                    {
                        this.save_inline_edit(cx);
                        cx.stop_propagation();
                    }
                    // Other keys belong to the textarea, never reader shortcuts.
                    return;
                }
                if this.image_preview.is_some() {
                    if event.keystroke.key == "escape" {
                        this.image_preview = None;
                        this.reader_focus.focus(window, cx);
                        cx.notify();
                    }
                    cx.stop_propagation();
                } else if event.keystroke.key == "escape" && this.reading_order_open {
                    this.reading_order_open = false;
                    this.reader_focus.focus(window, cx);
                    cx.notify();
                    cx.stop_propagation();
                } else if event.keystroke.key == "escape" && this.appearance_open {
                    this.appearance_open = false;
                    window.blur(cx);
                    this.flush_session();
                    cx.notify();
                    cx.stop_propagation();
                } else if event.keystroke.key == "escape" && this.navigation.is_some() {
                    this.navigation = None;
                    window.blur(cx);
                    cx.notify();
                    cx.stop_propagation();
                } else if event.keystroke.key == "escape" && this.margin_editor.is_some() {
                    this.close_margin_note(window, cx);
                    cx.stop_propagation();
                } else if event.keystroke.key == "escape"
                    && this.panel == Panel::Chat
                    && (this.chat_ui.model_open
                        || this.chat_ui.style_open
                        || this.chat_ui.list_open)
                {
                    this.chat_ui.model_open = false;
                    this.chat_ui.style_open = false;
                    this.chat_ui.list_open = false;
                    this.question.read(cx).focus_handle(cx).focus(window, cx);
                    cx.notify();
                    cx.stop_propagation();
                } else if event.keystroke.key == "escape"
                    && this.panel == Panel::Chat
                    && this.question.read(cx).focus_handle(cx).is_focused(window)
                {
                    window.blur(cx);
                    cx.stop_propagation();
                } else if event.keystroke.key == "escape" && this.panel_open {
                    this.show_panel(this.panel, window, cx);
                    cx.stop_propagation();
                } else if event.keystroke.key == "escape" && this.selection.is_some() {
                    this.selection = None;
                    TextSelection::clear(window, cx);
                    cx.stop_propagation();
                    cx.notify();
                } else if !this.chat_ui.model_open && !this.chat_ui.style_open {
                    this.reader_shortcut(event, window, cx);
                }
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    let in_reading_order = this
                        .reading_order_button_bounds
                        .is_some_and(|bounds| bounds.contains(&event.position))
                        || (this.reading_order_open
                            && this
                                .reading_order_menu_bounds
                                .is_some_and(|bounds| bounds.contains(&event.position)));
                    if !in_reading_order {
                        this.reading_order_open = false;
                    }
                    let in_appearance = this
                        .appearance_button_bounds
                        .is_some_and(|bounds| bounds.contains(&event.position))
                        || (this.appearance_open
                            && this
                                .appearance_menu_bounds
                                .is_some_and(|bounds| bounds.contains(&event.position)));
                    if !in_appearance {
                        this.appearance_open = false;
                    }
                    let in_navigation = this
                        .navigation_button_bounds
                        .iter()
                        .flatten()
                        .any(|bounds| bounds.contains(&event.position))
                        || (this.navigation.is_some()
                            && this
                                .navigation_menu_bounds
                                .is_some_and(|bounds| bounds.contains(&event.position)));
                    if !in_navigation {
                        this.navigation = None;
                    }
                    if this.margin_editor.is_some() {
                        this.close_margin_note(window, cx);
                    }
                    this.selection = None;
                    this.annotation_popup = None;
                    cx.notify();
                }),
            )
            .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, _, cx| {
                if this.is_epub()
                    && !this.appearance_open
                    && !this.reading_order_open
                    && this.navigation.is_none()
                {
                    this.epub_wheel(event, cx);
                }
                if this.list.viewport_bounds().contains(&event.position) {
                    this.cancel_annotation_scroll();
                }
                this.selection = None;
                this.annotation_popup = None;
                cx.notify();
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|_, event: &MouseUpEvent, window, cx| {
                    let position = event.position;
                    let weak = cx.entity().downgrade();
                    // Selection geometry is projected into text ranges during
                    // prepaint. Read it after the release frame has painted.
                    window.on_next_frame(move |window, _| {
                        window.on_next_frame(move |_, cx| {
                            let _ =
                                weak.update(cx, |this, cx| this.capture_selection(position, cx));
                        });
                    });
                }),
            )
            .child(
                div()
                    .h(px(56.))
                    .flex_shrink_0()
                    .px_4()
                    .flex()
                    .items_center()
                    .gap_2()
                    // Toolbar releases must not schedule body text selection
                    // capture while a panel button is changing the layout.
                    .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .border_b_1()
                    .border_color(rgb(0xe3ddd2))
                    .child(self.navigation_button(Navigation::Library, cx))
                    .child(self.navigation_button(Navigation::Contents, cx))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .items_center()
                            .gap(px(10.))
                            .child(
                                div()
                                    .min_w_0()
                                    .max_w(px(420.))
                                    .truncate()
                                    .text_size(px(14.))
                                    .line_height(relative(1.2))
                                    .font_weight(FontWeight::MEDIUM)
                                    .font_family(self.fonts.title(&title))
                                    .child(if title.is_empty() {
                                        "Vellio".to_owned()
                                    } else {
                                        title
                                    }),
                            )
                            .when(!chapter.is_empty(), |d| {
                                d.child(div().text_color(rgb(0x969084)).child("·")).child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .truncate()
                                        .text_size(px(12.5))
                                        .text_color(rgb(0x969084))
                                        .child(chapter),
                                )
                            }),
                    )
                    .when(show_status, |d| {
                        d.child(
                            div()
                                .debug_selector(|| "reader-status".into())
                                .max_w(px(180.))
                                .truncate()
                                .text_size(px(12.))
                                .text_color(rgb(0x969084))
                                .child(self.status.clone()),
                        )
                    })
                    .when(self.settings.committed.features.original, |d| {
                        d.child(self.reading_mode_control(cx))
                    })
                    .when(self.busy && self.home.cancellable_task, |d| {
                        d.child(
                            div().debug_selector(|| "reader-cancel-task".into()).child(
                                Button::new("cancel")
                                    .ghost()
                                    .small()
                                    .label("取消")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.cancel.store(true, Ordering::Relaxed);
                                        this.status = "正在取消……".into();
                                        cx.notify();
                                    })),
                            ),
                        )
                    })
                    .when(self.settings.committed.features.page, |d| {
                        d.child(
                            Button::new("pdf-toggle")
                                .ghost()
                                .small()
                                .icon(Icon::default().path("icons/file.svg").size(px(17.)))
                                .label(if self.is_epub() { "EPUB" } else { "PDF" })
                                .rounded(px(9.))
                                .text_color(rgb(0x625e55))
                                .when(self.show_pdf, |b| {
                                    b.bg(rgba(0x38748720)).text_color(rgb(0x387487))
                                })
                                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if this.show_pdf {
                                        this.show_pdf = false;
                                        this.reset_rows();
                                        cx.notify();
                                    } else {
                                        let index = this.list.logical_scroll_top().item_ix;
                                        this.locate_source(index, cx);
                                    }
                                })),
                        )
                    })
                    .child(
                        Button::new("notes-tab")
                            .ghost()
                            .small()
                            .icon(
                                Icon::default()
                                    .path("icons/message-square.svg")
                                    .size(px(17.)),
                            )
                            .label("Notes")
                            .rounded(px(9.))
                            .text_color(rgb(0x625e55))
                            .when(self.panel_open && self.panel == Panel::Notes, |b| {
                                b.bg(rgba(0x38748720)).text_color(rgb(0x387487))
                            })
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.show_panel(Panel::Notes, window, cx)
                            })),
                    )
                    .when(self.settings.committed.features.ask_ai, |d| {
                        d.child(
                            Button::new("chat-tab")
                                .ghost()
                                .small()
                                .icon(Icon::default().path("icons/asterisk.svg").size(px(17.)))
                                .label("Ask AI")
                                .rounded(px(9.))
                                .text_color(rgb(0x625e55))
                                .when(self.panel_open && self.panel == Panel::Chat, |b| {
                                    b.bg(rgba(0x38748720)).text_color(rgb(0x387487))
                                })
                                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.show_panel(Panel::Chat, window, cx)
                                })),
                        )
                    })
                    .child(
                        div()
                            .relative()
                            .child({
                                let weak = cx.entity().downgrade();
                                canvas(
                                    move |bounds, _, cx| {
                                        let _ = weak.update(cx, |this, _| {
                                            this.appearance_button_bounds = Some(bounds)
                                        });
                                    },
                                    |_, _, _, _| {},
                                )
                                .absolute()
                                .size_full()
                            })
                            .child(
                                Button::new("appearance-toggle")
                                    .ghost()
                                    .small()
                                    .label("Aa")
                                    .accessibility_label("Reading appearance")
                                    .rounded(px(9.))
                                    .text_color(rgb(0x625e55))
                                    .when(self.appearance_open, |b| {
                                        b.bg(rgba(0x38748720)).text_color(rgb(0x387487))
                                    })
                                    .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                        cx.stop_propagation()
                                    })
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.navigation = None;
                                        this.reading_order_open = false;
                                        this.appearance_open = appearance_will_open;
                                        this.selection = None;
                                        this.annotation_popup = None;
                                        if this.appearance_open {
                                            this.appearance_focus.focus(window, cx);
                                        } else {
                                            window.blur(cx);
                                            this.flush_session();
                                        }
                                        cx.notify();
                                    })),
                            ),
                    ),
            )
            .child(self.reading_progress_bar(cx))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .relative()
                    .overflow_hidden()
                    .child(
                        div()
                            .debug_selector(|| "reader-column".into())
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .when(self.state.paper.blocks.is_empty(), |d| {
                                d.child(div().p_8().child(if self.workspace.is_some() {
                                    "该 PDF 没有可提取文字，可在原页查看；扫描件的 OCR 尚未接入。"
                                } else {
                                    "导入 PDF / EPUB 开始阅读，或迁入已有论文。"
                                }))
                            })
                            .child(if self.is_epub() {
                                self.epub_pages_view(cx)
                            } else {
                                list(self.list.clone(), move |i, _, cx| {
                                    weak.update(cx, |this, cx| this.block(i, cx))
                                        .unwrap_or_else(|_| div().into_any_element())
                                })
                                .w_full()
                                .flex_1()
                                .min_h_0()
                                .into_any_element()
                            }),
                    )
                    .when(self.show_pdf && !margin_open && !self.panel_open, |d| {
                        d.child(
                            div()
                                .id("pdf-panel")
                                .debug_selector(|| "source-panel".into())
                                .w(px(260.))
                                .min_h_0()
                                .overflow_y_scroll()
                                .border_l_1()
                                .border_color(rgb(0xe3ddd2))
                                .p_3()
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap_2()
                                        .when(self.is_epub(), |d| {
                                            d.child(
                                                div()
                                                    .flex_1()
                                                    .text_size(px(13.))
                                                    .child(format!("EPUB p.{} · 原文", self.page)),
                                            )
                                        })
                                        .when(!self.is_epub(), |d| {
                                            d.child(
                                                Button::new("prev-page")
                                                    .label("上一页")
                                                    .disabled(self.page <= 1)
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.page_block = None;
                                                        this.page -= 1;
                                                        this.load_page();
                                                        cx.notify();
                                                    })),
                                            )
                                            .child(format!(
                                                "{} / {}",
                                                self.page,
                                                self.state.paper.meta.pages.len()
                                            ))
                                            .child(
                                                Button::new("next-page")
                                                    .label("下一页")
                                                    .disabled(
                                                        self.page
                                                            >= self.state.paper.meta.pages.len(),
                                                    )
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.page_block = None;
                                                        this.page += 1;
                                                        this.load_page();
                                                        cx.notify();
                                                    })),
                                            )
                                        })
                                        .child(
                                            Button::new("close-source-panel")
                                                .ghost()
                                                .xsmall()
                                                .label("×")
                                                .accessibility_label("Close source")
                                                .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                                    cx.stop_propagation()
                                                })
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.show_pdf = false;
                                                    this.reset_rows();
                                                    cx.notify();
                                                })),
                                        ),
                                )
                                .child(page_view),
                        )
                    })
                    .when(
                        self.tool_panel_visible() && self.panel == Panel::Chat,
                        |d| d.child(self.chat_column(window, cx)),
                    )
                    .when(self.panel_open && self.panel != Panel::Chat, |d| {
                        d.child(
                            div()
                                .w(px(420.))
                                .flex_shrink_0()
                                .min_h_0()
                                .flex()
                                .flex_col()
                                .border_l_1()
                                .border_color(rgb(0xe3ddd2))
                                .bg(rgb(0xfbfaf7))
                                .p(px(16.))
                                .gap(px(12.))
                                .when(self.panel != Panel::Notes, |d| {
                                    d.child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(8.))
                                            .child(div().flex_1().text_size(px(13.)).child(
                                                if self.panel == Panel::Chat {
                                                    "Ask AI"
                                                } else {
                                                    "Edit translation"
                                                },
                                            ))
                                            .child(
                                                Button::new("close-tool-panel")
                                                    .ghost()
                                                    .w(px(24.))
                                                    .h(px(28.))
                                                    .p_0()
                                                    .label("×")
                                                    .accessibility_label("Close panel")
                                                    .on_click(cx.listener(
                                                        |this, _, window, cx| {
                                                            this.show_panel(this.panel, window, cx)
                                                        },
                                                    )),
                                            ),
                                    )
                                })
                                .child(
                                    div()
                                        .id("notes-scroll")
                                        .flex_1()
                                        .min_h_0()
                                        .when(self.panel != Panel::Chat, |d| d.overflow_y_scroll())
                                        .child(panel),
                                ),
                        )
                    }),
            )
            .when_some(toolbar, |d, toolbar| d.child(toolbar))
            .when_some(annotation_popup, |d, popup| d.child(popup))
            .when_some(appearance_menu, |d, menu| d.child(menu))
            .when_some(reading_order_menu, |d, menu| d.child(menu))
            .when_some(navigation_menu, |d, menu| d.child(menu))
            .when_some(image_preview, |d, preview| d.child(preview))
            .into_any_element()
    }
}
pub fn run(raw: Value, library: Option<Library>, workspace: Option<Workspace>) {
    gpui_kit::application()
        .with_assets(ReaderAssets)
        .run(move |cx| {
            #[cfg(target_os = "macos")]
            set_dock_icon();
            cx.text_system()
                .add_fonts(vec![
                    std::borrow::Cow::Borrowed(include_bytes!(
                        "../assets/fonts/NotoSerifSC-Regular.ttf"
                    )),
                    std::borrow::Cow::Borrowed(include_bytes!(
                        "../assets/fonts/NotoSerifSC-SemiBold.ttf"
                    )),
                ])
                .expect("Cannot load bundled Noto Serif SC fonts");
            gpui_kit::init(cx);
            typography::apply_theme(false, cx);
            let text_style =
                gpui_kit::base::TextViewStyle::from_theme(&gpui_kit::base::Theme::global(cx))
                    .with_link(rgb(annotations::LINK_COLOR).into())
                    .with_link_underline(false);
            cx.set_global(
                gpui_kit::base::TextViewDefaults::new()
                    .with_style(text_style)
                    .with_code_block_highlighter(code::highlight),
            );
            cx.on_window_closed(|cx, _| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                        None,
                        size(px(1400.), px(900.)),
                        cx,
                    ))),
                    ..Default::default()
                },
                |window, cx| {
                    let view = cx.new(|cx| ReaderView::new(raw, library, workspace, window, cx));
                    cx.new(|cx| Root::new(view, window, cx))
                },
            )
            .expect("Cannot open Vellio window");
            cx.activate(true);
        });
}

#[cfg(target_os = "macos")]
fn set_dock_icon() {
    use objc2::{AnyThread, MainThreadMarker};
    use objc2_app_kit::{NSApplication, NSImage};
    use objc2_foundation::NSData;

    // Direct executable launches and stale LaunchServices entries can otherwise
    // leave the running app with macOS's generic placeholder icon.
    let main_thread = MainThreadMarker::new().expect("App startup must run on the main thread");
    let data = NSData::with_bytes(include_bytes!("../assets/icons/Vellio.icns"));
    let Some(icon) = NSImage::initWithData(NSImage::alloc(), &data).filter(|icon| icon.isValid())
    else {
        eprintln!("Cannot decode the embedded Vellio Dock icon");
        return;
    };
    // SAFETY: A valid, non-null image is supplied on the application main thread.
    unsafe {
        NSApplication::sharedApplication(main_thread).setApplicationIconImage(Some(&icon));
    }
}
