//! Full settings surface with fixed header/footer and independently scrolling tabs.
use super::*;
use easyread_gpui::settings::{self as preferences, Preferences, SHORTCUTS};
use gpui_kit::base::Dialog as Modal;
use gpui_kit::component::{
    IndexPath,
    select::{Select, SelectEvent, SelectState},
    switch::Switch,
};
use std::path::PathBuf;

const INK: u32 = 0x26231f;
const MUTED: u32 = 0x8c8579;
const BORDER: u32 = 0xd8cfbf;
const ACCENT: u32 = 0x306778;
const PAPER: u32 = 0xfbfaf7;
const TABS: [&str; 5] = [
    "Model",
    "Reading",
    "Sidebar",
    "Cloud library",
    "Keyboard shortcuts",
];
pub(super) struct SettingsState {
    pub open: bool,
    pub committed: Preferences,
    draft: Preferences,
    tab: usize,
    scroll: ScrollHandle,
    selects: HashMap<&'static str, Entity<SelectState<Vec<String>>>>,
    pub focus: FocusHandle,
    recording: Option<String>,
    model_edit: Option<ai::ModelProfile>,
    model_menu: Option<String>,
    inputs: HashMap<&'static str, Entity<InputState>>,
    drives: Vec<(String, PathBuf)>,
    pub notice: String,
    log_open: bool,
    pub logs: Vec<String>,
    pub relocating: bool,
    pub translation_confirm: bool,
}
impl SettingsState {
    pub fn new(lib: Option<&Library>, window: &mut Window, cx: &mut Context<ReaderView>) -> Self {
        let (mut committed, notice) = match Preferences::load(lib.map(|l| l.root.as_path())) {
            Ok(p) => (p, String::new()),
            Err(e) => (
                Preferences::default(),
                format!("Settings could not be read: {e}"),
            ),
        };
        if committed.models.is_empty() {
            for (provider, id, name, model) in [
                (
                    ai::Provider::ClaudeCode,
                    "claude-opus",
                    "Claude Opus",
                    "opus",
                ),
                (
                    ai::Provider::ClaudeCode,
                    "claude-sonnet",
                    "Claude Sonnet",
                    "sonnet",
                ),
                (
                    ai::Provider::CodexCli,
                    "codex-default",
                    "Codex default",
                    "default",
                ),
            ] {
                if ai::cli_executable(provider, "").is_ok() {
                    committed.models.push(ai::ModelProfile {
                        id: id.into(),
                        name: name.into(),
                        provider,
                        model: model.into(),
                        ..Default::default()
                    });
                }
            }
            if let Some(first) = committed.models.first() {
                committed.translation_model = first.id.clone();
                committed.ask_model = first.id.clone();
            }
        }
        let inputs = [
            ("name", "Display name"),
            ("model", "Model ID or CLI alias"),
            ("base", "https://api.openai.com/v1"),
            ("key", "API key environment variable"),
            ("executable", "Auto-detect (or absolute path)"),
            ("folder", "New folder name"),
        ]
        .map(|(id, placeholder)| {
            (
                id,
                cx.new(|cx| InputState::new(window, cx).placeholder(placeholder)),
            )
        })
        .into_iter()
        .collect();
        Self {
            open: false,
            draft: committed.clone(),
            committed,
            tab: 0,
            scroll: ScrollHandle::new(),
            selects: HashMap::new(),
            focus: cx.focus_handle(),
            recording: None,
            model_edit: None,
            model_menu: None,
            inputs,
            drives: preferences::cloud_drives(),
            notice,
            log_open: false,
            logs: vec![],
            relocating: false,
            translation_confirm: false,
        }
    }
}
fn section(title: impl Into<SharedString>) -> Div {
    div()
        .mt(px(26.))
        .mb(px(18.))
        .text_size(px(12.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgb(MUTED))
        .child(title.into())
}
fn muted(text: impl Into<SharedString>) -> Div {
    div()
        .text_size(px(12.5))
        .line_height(relative(1.6))
        .text_color(rgb(MUTED))
        .child(text.into())
}
fn outline(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Button {
    Button::new(id)
        .label(label)
        .text_size(px(13.5))
        .h(px(36.))
        .px(px(14.))
        .rounded(px(9.))
        .bg(rgb(PAPER))
        .border_color(rgb(BORDER))
        .text_color(rgb(0x625e55))
}
fn primary(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Button {
    Button::new(id)
        .primary()
        .label(label)
        .text_size(px(13.5))
        .h(px(38.))
        .px(px(22.))
        .rounded(px(9.))
        .bg(rgb(ACCENT))
        .text_color(rgb(0xffffff))
}

impl ReaderView {
    pub(super) fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.capture_inputs(cx);
        self.capture_position();
        self.flush_session();
        self.queue_margin_note();
        self.appearance_open = false;
        self.reading_order_open = false;
        self.navigation = None;
        self.selection = None;
        self.settings.open = true;
        self.settings.draft = self.settings.committed.clone();
        self.settings.draft.appearance = if self.home.visible {
            self.settings.committed.appearance.clone()
        } else {
            self.appearance.clone()
        };
        self.settings.model_edit = None;
        self.settings.model_menu = None;
        self.settings.recording = None;
        self.settings.log_open = false;
        self.settings.scroll = ScrollHandle::new();
        self.settings.selects.clear();
        gpui_kit::component::Theme::change(gpui_kit::component::ThemeMode::Light, Some(window), cx);
        typography::apply_theme(false, cx);
        self.settings.focus.focus(window, cx);
        self.init_settings_selects(window, cx);
        cx.notify();
    }
    fn settings_select_state(
        &mut self,
        id: &'static str,
        values: Vec<String>,
        current: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let selected = values
            .iter()
            .position(|v| v == &current)
            .map(IndexPath::new);
        let state = cx.new(|cx| SelectState::new(values, selected, window, cx));
        cx.subscribe_in(
            &state,
            window,
            move |this, _, event: &SelectEvent<Vec<String>>, window, cx| {
                if let SelectEvent::Confirm(Some(value)) = event {
                    this.setting_selected(id, value, window, cx);
                }
            },
        )
        .detach();
        self.settings.selects.insert(id, state);
    }
    fn init_settings_selects(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let p = &self.settings.draft;
        let specs = vec![
            (
                "font_size",
                (14..=32).map(|n| format!("{n} px")).collect(),
                format!("{} px", p.appearance.font_size),
            ),
            (
                "width",
                (25..=60).map(|n| format!("{n} chars")).collect(),
                format!("{} chars", p.appearance.line_width),
            ),
            (
                "spacing",
                (12..=26)
                    .map(|n| format!("{:.1}", n as f32 / 10.))
                    .collect(),
                format!("{:.1}", p.appearance.line_spacing),
            ),
            (
                "font",
                ReadingFont::ALL
                    .iter()
                    .map(|f| font_label(*f).into())
                    .collect(),
                font_label(p.appearance.reading_font()).into(),
            ),
            (
                "open",
                vec!["Original".into(), "Bilingual".into()],
                if p.bilingual { "Bilingual" } else { "Original" }.into(),
            ),
            (
                "margin",
                vec!["Show".into(), "Hide".into()],
                if p.appearance.margin_notes {
                    "Show"
                } else {
                    "Hide"
                }
                .into(),
            ),
            (
                "batch",
                (1..=10).map(|n| format!("{n} pages")).collect(),
                format!("{} pages", p.pages_per_batch),
            ),
            (
                "parallel",
                std::iter::once("Auto".into())
                    .chain((1..=8).map(|n| n.to_string()))
                    .collect(),
                if p.parallel == 0 {
                    "Auto".into()
                } else {
                    p.parallel.to_string()
                },
            ),
            (
                "threshold",
                [0, 20, 40, 60, 100, 200, 500]
                    .map(|n| {
                        if n == 0 {
                            "Always ask".into()
                        } else {
                            format!("{n} pages")
                        }
                    })
                    .to_vec(),
                if p.ask_above == 0 {
                    "Always ask".into()
                } else {
                    format!("{} pages", p.ask_above)
                },
            ),
            (
                "effort",
                vec![
                    "Default".into(),
                    "Low".into(),
                    "Medium".into(),
                    "High".into(),
                ],
                p.models
                    .iter()
                    .find(|m| m.id == p.translation_model)
                    .map(|m| effort_label(&m.reasoning).to_string())
                    .unwrap_or("Default".into()),
            ),
            (
                "provider",
                vec![
                    "API (OpenAI compatible)".into(),
                    "Claude Code".into(),
                    "Codex CLI".into(),
                ],
                self.settings
                    .model_edit
                    .as_ref()
                    .map(|m| m.provider.label())
                    .unwrap_or(ai::Provider::Api.label())
                    .into(),
            ),
        ];
        for (id, values, current) in specs {
            self.settings_select_state(id, values, current, window, cx);
        }
    }
    fn setting_selected(&mut self, id: &str, value: &str, _: &mut Window, cx: &mut Context<Self>) {
        let p = &mut self.settings.draft;
        let number = || {
            value
                .split_whitespace()
                .next()
                .unwrap_or("")
                .parse::<f32>()
                .unwrap_or(0.)
        };
        match id {
            "font_size" => p.appearance.font_size = number(),
            "width" => p.appearance.line_width = number(),
            "spacing" => p.appearance.line_spacing = number(),
            "font" => p.appearance.set_font(
                ReadingFont::ALL
                    .into_iter()
                    .find(|f| font_label(*f) == value)
                    .unwrap_or_default(),
            ),
            "open" => p.bilingual = value == "Bilingual",
            "margin" => p.appearance.margin_notes = value == "Show",
            "batch" => p.pages_per_batch = number() as usize,
            "parallel" => p.parallel = number() as usize,
            "threshold" => p.ask_above = number() as usize,
            "effort" => {
                if let Some(m) = p.models.iter_mut().find(|m| m.id == p.translation_model) {
                    m.reasoning = if value == "Default" {
                        String::new()
                    } else {
                        value.to_lowercase()
                    }
                }
            }
            "provider" => {
                if let Some(m) = self.settings.model_edit.as_mut() {
                    m.provider = match value {
                        "Claude Code" => ai::Provider::ClaudeCode,
                        "Codex CLI" => ai::Provider::CodexCli,
                        _ => ai::Provider::Api,
                    }
                }
            }
            _ => {}
        }
        if matches!(id, "batch" | "parallel" | "threshold" | "effort") {
            self.commit_model_settings();
        }
        cx.notify();
    }
    fn settings_select(&self, id: &'static str, label: &'static str) -> AnyElement {
        div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(muted(label))
            .child(
                Select::new(&self.settings.selects[id])
                    .w_full()
                    .h(px(46.))
                    .rounded(px(10.))
                    .bg(rgb(PAPER))
                    .border_color(rgb(BORDER))
                    .text_size(px(14.))
                    .accessibility_label(label),
            )
            .into_any_element()
    }
    fn commit_model_settings(&mut self) -> bool {
        let mut committed = self.settings.committed.clone();
        committed.models = self.settings.draft.models.clone();
        committed.translation_model = self.settings.draft.translation_model.clone();
        committed.ask_model = self.settings.draft.ask_model.clone();
        committed.pages_per_batch = self.settings.draft.pages_per_batch;
        committed.parallel = self.settings.draft.parallel;
        committed.ask_above = self.settings.draft.ask_above;
        match self
            .library
            .as_ref()
            .map(|l| committed.save(&l.root))
            .unwrap_or(Ok(()))
        {
            Ok(()) => {
                self.settings.committed = committed;
                true
            }
            Err(e) => {
                self.settings.notice = format!("Could not save models: {e}");
                false
            }
        }
    }
    fn close_settings(&mut self, save: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.settings.relocating {
            return;
        }
        if save {
            self.settings.draft.appearance.normalize();
            let result = self.settings.draft.validate().and_then(|_| {
                self.library
                    .as_ref()
                    .map(|l| self.settings.draft.save(&l.root))
                    .unwrap_or(Ok(()))
            });
            if let Err(e) = result {
                self.settings.notice = e.to_string();
                cx.notify();
                return;
            }
            self.settings.committed = self.settings.draft.clone();
            self.appearance = self.settings.committed.appearance.clone();
            self.reading_mode = if self.settings.committed.bilingual {
                ReadingMode::Bilingual
            } else {
                ReadingMode::Original
            };
            self.original_blocks.clear();
            self.clear_paragraph_layouts();
            self.enforce_features();
            self.appearance_changed(cx);
            self.flush_session();
        }
        self.settings.open = false;
        self.settings.recording = None;
        if self.home.visible {
            self.home.apply_theme(Some(window), cx);
        }
        window.blur(cx);
        cx.notify();
    }
    pub(super) fn enforce_features(&mut self) {
        let f = self.settings.committed.features.clone();
        if !f.original {
            self.reading_mode = ReadingMode::Translation;
            self.reading_order_open = false;
            self.original_blocks.clear();
            self.clear_paragraph_layouts();
        }
        if !f.page {
            self.show_pdf = false;
        }
        if !f.edit {
            self.inline_edit = None;
        }
        if self.panel_open
            && ((self.panel == Panel::Chat && !f.ask_ai) || (self.panel == Panel::Edit && !f.edit))
        {
            self.panel_open = false;
        }
    }
    fn reading_settings(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = &self.settings.draft;
        let mut body=div().flex().flex_col().child(section("L a y o u t").mt_0().pb(px(12.)).border_b_1().border_color(rgb(BORDER)))
            .child(div().flex().gap(px(18.)).mb(px(24.)).children([self.settings_select("font_size","Font size"),self.settings_select("width","Text width (characters per line)"),self.settings_select("spacing","Line spacing")]))
            .child(div().flex().gap(px(18.)).children([self.settings_select("font","Typeface"),self.settings_select("open","Show on open"),self.settings_select("margin","Margin notes")]))
            .child(div().mt(px(24.)).flex().items_center().gap(px(16.)).child(muted("You can also adjust this from aA at the top right while reading.").flex_1())
                .child(Button::new("reset-layout").ghost().label("Reset layout").on_click(cx.listener(|this,_,window,cx| {this.settings.draft.appearance=Appearance::default();this.settings.draft.bilingual=false;this.init_settings_selects(window,cx);cx.notify();}))))
            .child(section("F e a t u r e s"))
            .child(div().mb(px(18.)).text_color(rgb(0x625e55)).child("Which features the reader shows. Turning one off hides its buttons and shortcuts too."));
        for (id, title, description, checked) in [
            (
                "ask_ai",
                "Ask AI",
                "Chat on the right side of the reader; Ask AI on selected text and note cards",
                p.features.ask_ai,
            ),
            (
                "edit",
                "Edit translation",
                "Edit translation in the paragraph toolbar",
                p.features.edit,
            ),
            (
                "original",
                "Show English original",
                "Original in the paragraph toolbar",
                p.features.original,
            ),
            (
                "page",
                "Original page panel",
                "p.xx beside the text and PDF / EPUB p.xx in the paragraph toolbar",
                p.features.page,
            ),
        ] {
            body = body.child(
                div()
                    .py(px(20.))
                    .border_b_1()
                    .border_color(rgb(BORDER))
                    .flex()
                    .items_center()
                    .gap(px(20.))
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap(px(10.))
                            .child(
                                div()
                                    .text_size(px(14.))
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(title),
                            )
                            .child(muted(description)),
                    )
                    .child(
                        Switch::new(id)
                            .checked(checked)
                            .color(rgb(ACCENT))
                            .accessibility_label(title)
                            .on_click(cx.listener(move |this, checked: &bool, _, cx| {
                                let f = &mut this.settings.draft.features;
                                match id {
                                    "ask_ai" => f.ask_ai = *checked,
                                    "edit" => f.edit = *checked,
                                    "original" => f.original = *checked,
                                    _ => f.page = *checked,
                                };
                                cx.notify();
                            })),
                    ),
            );
        }
        body.into_any_element()
    }
    fn edit_model(
        &mut self,
        model: Option<ai::ModelProfile>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let model = model.unwrap_or_else(|| ai::ModelProfile {
            id: format!("model-{}", Uuid::new_v4()),
            ..Default::default()
        });
        for (id, value) in [
            ("name", &model.name),
            ("model", &model.model),
            ("base", &model.base_url),
            ("key", &model.key_env),
            ("executable", &model.executable),
        ] {
            self.settings.inputs[id]
                .update(cx, |input, cx| input.set_value(value.clone(), window, cx));
        }
        self.settings.model_edit = Some(model);
        self.settings.model_menu = None;
        self.init_settings_selects(window, cx);
        cx.notify();
    }
    fn save_model(&mut self, cx: &mut Context<Self>) {
        let Some(mut model) = self.settings.model_edit.clone() else {
            return;
        };
        for (id, field) in [
            ("name", &mut model.name),
            ("model", &mut model.model),
            ("base", &mut model.base_url),
            ("key", &mut model.key_env),
            ("executable", &mut model.executable),
        ] {
            *field = self.settings.inputs[id].read(cx).value().trim().to_owned();
        }
        if let Err(e) = model.validate() {
            self.settings.notice = e.to_string();
            cx.notify();
            return;
        }
        let p = &mut self.settings.draft;
        if let Some(existing) = p.models.iter_mut().find(|m| m.id == model.id) {
            *existing = model.clone()
        } else {
            p.models.push(model.clone())
        }
        if p.translation_model.is_empty() {
            p.translation_model = model.id.clone()
        }
        if p.ask_model.is_empty() {
            p.ask_model = model.id.clone()
        }
        if !self.commit_model_settings() {
            cx.notify();
            return;
        }
        self.settings.model_edit = None;
        self.settings.notice.clear();
        cx.notify();
    }
    fn model_settings(&self, cx: &mut Context<Self>) -> AnyElement {
        if let Some(model) = &self.settings.model_edit {
            let mut form = div()
                .flex()
                .flex_col()
                .gap(px(22.))
                .child(
                    div()
                        .text_size(px(20.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("Model connection"),
                )
                .child(self.settings_select("provider", "Connection"));
            for (id, label) in [
                ("name", "Display name"),
                ("model", "Model ID / CLI alias"),
                ("base", "API base URL"),
                ("key", "API key environment variable"),
                ("executable", "CLI executable path (optional)"),
            ] {
                if (model.provider == ai::Provider::Api && id == "executable")
                    || (model.provider != ai::Provider::Api && matches!(id, "base" | "key"))
                {
                    continue;
                }
                form = form.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(8.))
                        .child(muted(label))
                        .child(
                            Input::new(&self.settings.inputs[id])
                                .h(px(42.))
                                .bg(rgb(PAPER))
                                .border_color(rgb(BORDER))
                                .aria_label(label),
                        ),
                );
            }
            return form.child(muted(if model.provider==ai::Provider::Api {"The API key stays in the named environment variable."}else{"Uses the CLI’s existing login. Leave the path empty to detect it automatically."}))
                .child(div().flex().gap(px(12.)).child(outline("cancel-model","Cancel").on_click(cx.listener(|this,_,_,cx|{this.settings.model_edit=None;cx.notify()})))
                    .child(primary("save-model","Save model").on_click(cx.listener(|this,_,_,cx|this.save_model(cx))))).into_any_element();
        }
        let p = &self.settings.draft;
        let mut grid = div().grid().grid_cols(3).gap(px(12.));
        for (index, model) in p.models.iter().enumerate() {
            let id = model.id.clone();
            let selected = p.translation_model == id;
            let mut card = div()
                .id(("model-card", index))
                .w_full()
                .min_w_0()
                .min_h(px(132.))
                .p(px(16.))
                .rounded(px(14.))
                .border_1()
                .border_color(rgb(if selected { ACCENT } else { BORDER }))
                .bg(rgb(if selected { 0xecf0ef } else { PAPER }))
                .cursor_pointer()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child(
                            div()
                                .flex_1()
                                .text_size(px(14.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(model.name.clone()),
                        )
                        .child(
                            Button::new(("model-menu", index))
                                .ghost()
                                .xsmall()
                                .label("···")
                                .accessibility_label(format!("Options for {}", model.name))
                                .on_click(cx.listener({
                                    let id = id.clone();
                                    move |this, _, _, cx| {
                                        this.settings.model_menu =
                                            if this.settings.model_menu.as_ref() == Some(&id) {
                                                None
                                            } else {
                                                Some(id.clone())
                                            };
                                        cx.stop_propagation();
                                        cx.notify();
                                    }
                                })),
                        ),
                )
                .child(muted(model.provider.label()).mt(px(10.)));
            let mut tags = div().flex().flex_wrap().gap(px(6.)).mt(px(14.));
            for (active, title) in [
                (selected, "Translation"),
                (p.ask_model == id, "Ask AI default"),
            ] {
                if active {
                    tags = tags.child(
                        div()
                            .rounded(px(18.))
                            .px(px(10.))
                            .py(px(4.))
                            .text_size(px(13.))
                            .bg(rgb(0xdce6de))
                            .text_color(rgb(0x4e8550))
                            .child(title),
                    );
                }
            }
            card = card.child(tags).on_click(cx.listener({
                let id = id.clone();
                move |this, _, window, cx| {
                    this.settings.draft.translation_model = id.clone();
                    this.commit_model_settings();
                    this.init_settings_selects(window, cx);
                    cx.notify();
                }
            }));
            if self.settings.model_menu.as_ref() == Some(&id) {
                card = card.child(
                    div()
                        .mt(px(12.))
                        .flex()
                        .flex_col()
                        .gap(px(6.))
                        .child(outline(("model-edit", index), "Edit connection").on_click(
                            cx.listener({
                                let m = model.clone();
                                move |this, _, window, cx| {
                                    this.edit_model(Some(m.clone()), window, cx);
                                    cx.stop_propagation();
                                }
                            }),
                        ))
                        .child(
                            outline(("model-ask-default", index), "Use for Ask AI").on_click(
                                cx.listener({
                                    let id = id.clone();
                                    move |this, _, _, cx| {
                                        this.settings.draft.ask_model = id.clone();
                                        this.settings.model_menu = None;
                                        this.commit_model_settings();
                                        cx.stop_propagation();
                                        cx.notify();
                                    }
                                }),
                            ),
                        )
                        .child(
                            outline(("model-remove", index), "Remove").on_click(cx.listener({
                                let id = id.clone();
                                move |this, _, _, cx| {
                                    let p = &mut this.settings.draft;
                                    p.models.retain(|m| m.id != id);
                                    let fallback =
                                        p.models.first().map(|m| m.id.clone()).unwrap_or_default();
                                    if p.translation_model == id {
                                        p.translation_model = fallback.clone()
                                    }
                                    if p.ask_model == id {
                                        p.ask_model = fallback
                                    }
                                    this.settings.model_menu = None;
                                    this.commit_model_settings();
                                    cx.stop_propagation();
                                    cx.notify();
                                }
                            })),
                        ),
                );
            }
            grid = grid.child(card);
        }
        grid = grid.child(
            Button::new("add-model")
                .ghost()
                .border_dashed()
                .w_full()
                .min_w_0()
                .h(px(98.))
                .border_1()
                .border_color(rgb(BORDER))
                .rounded(px(14.))
                .child(
                    div()
                        .w_full()
                        .flex()
                        .flex_col()
                        .items_start()
                        .gap(px(10.))
                        .child(div().text_color(rgb(ACCENT)).text_size(px(25.)).child("+"))
                        .child(
                            div()
                                .text_size(px(14.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child("Add model"),
                        ),
                )
                .on_click(cx.listener(|this, _, window, cx| this.edit_model(None, window, cx))),
        );
        let translation = p
            .models
            .iter()
            .find(|m| m.id == p.translation_model)
            .map(|m| m.name.clone())
            .unwrap_or("Not configured".into());
        div()
            .flex()
            .flex_col()
            .child(grid)
            .child(
                div()
                    .mt(px(26.))
                    .mb(px(24.))
                    .border_t_1()
                    .border_color(rgb(BORDER)),
            )
            .child(section(format!("Translation: {translation}")).mt_0())
            .child(
                div()
                    .max_w(px(320.))
                    .child(self.settings_select("effort", "Reasoning effort")),
            )
            .child(div().mt(px(24.)).flex().gap(px(18.)).children([
                self.settings_select("batch", "Pages per batch"),
                self.settings_select("parallel", "Parallel segments"),
                self.settings_select("threshold", "Ask first if a paper has more than"),
            ]))
            .child(
                div().mt(px(26.)).child(
                    outline("test-model", "✳  Test with one sentence")
                        .disabled(self.busy || p.models.is_empty())
                        .on_click(cx.listener(|this, _, _, cx| this.test_model(cx))),
                ),
            )
            .child(
                muted(format!(
                    "Library location: {}",
                    self.library
                        .as_ref()
                        .map(|l| l.root.display().to_string())
                        .unwrap_or("Snapshot mode".into())
                ))
                .mt(px(24.)),
            )
            .into_any_element()
    }
    fn test_model(&mut self, cx: &mut Context<Self>) {
        let connection = match self.settings.committed.connection(true) {
            Ok(c) => c,
            Err(e) => {
                self.settings.notice = e.to_string();
                cx.notify();
                return;
            }
        };
        self.settings.notice = "Testing…".into();
        self.worker(cx,move |cancel,tx| {use ai::Transport;let result=connection.request(json!([{"role":"system","content":"Translate the next sentence into Chinese. Only output the translation."},{"role":"user","content":"Reading makes ideas clearer."}]),&cancel,&mut |_|{});let _=tx.send(Event::SettingsResult(match result {Ok(answer)=>format!("Test succeeded: {answer}"),Err(e)=>format!("Test failed: {e}")}));});
    }
    fn sidebar_settings(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut body=div().flex().flex_col().child(div().text_color(rgb(0x625e55)).line_height(relative(1.8)).child("Choose which folders appear in the library sidebar. The switch shows or hides a folder; the pin keeps it at the top. A paper can be in several folders: right-click it, or drag it onto a folder. Changes apply immediately."))
            .child(section("B u i l t - i n   f o l d e r s"));
        for (id, title, description) in [
            ("all", "All", "Always shown".into()),
            (
                "reading",
                "Reading",
                format!(
                    "{} papers",
                    self.items
                        .iter()
                        .filter(|i| i["status"] == "reading")
                        .count()
                ),
            ),
            (
                "unread",
                "Unread",
                format!(
                    "{} papers",
                    self.items
                        .iter()
                        .filter(|i| i["status"] == "unread")
                        .count()
                ),
            ),
            (
                "done",
                "Done",
                format!(
                    "{} papers",
                    self.items.iter().filter(|i| i["status"] == "done").count()
                ),
            ),
            (
                "starred",
                "Starred",
                format!(
                    "{} papers",
                    self.items
                        .iter()
                        .filter(|i| i["item"]["starred"].as_bool().unwrap_or(false))
                        .count()
                ),
            ),
            (
                "questions",
                "Open questions",
                "Shown only when there are unanswered questions".into(),
            ),
            (
                "translating",
                "Translating",
                "Shown only while a paper is translating".into(),
            ),
        ] {
            body = body.child(self.sidebar_setting_row(id.into(), title.into(), description, cx));
        }
        body = body.child(section("M y   f o l d e r s"));
        let folders = self.home.folders();
        if folders.is_empty() {
            body = body.child(muted("No folders yet.").mb(px(22.)));
        }
        for (id, name) in folders {
            let count = self
                .items
                .iter()
                .filter(|i| self.home.in_folder(i["id"].as_str().unwrap_or(""), &id))
                .count();
            body = body.child(self.sidebar_setting_row(id, name, format!("{count} papers"), cx));
        }
        body.child(
            div()
                .mt(px(16.))
                .flex()
                .items_center()
                .gap(px(20.))
                .child(
                    Input::new(&self.settings.inputs["folder"])
                        .h(px(42.))
                        .w(px(340.))
                        .bg(rgb(PAPER))
                        .border_color(rgb(BORDER))
                        .aria_label("New folder name"),
                )
                .child(div().flex_1())
                .child(
                    outline("settings-new-folder", "+  New folder").on_click(cx.listener(
                        |this, _, window, cx| {
                            let name = this.settings.inputs["folder"].read(cx).value().to_string();
                            this.home
                                .folder_input
                                .update(cx, |i, cx| i.set_value(name, window, cx));
                            this.create_home_folder(window, cx);
                            this.settings.inputs["folder"]
                                .update(cx, |i, cx| i.set_value("", window, cx));
                            this.settings.notice = this.home.notice.clone();
                            cx.notify();
                        },
                    )),
                ),
        )
        .into_any_element()
    }
    fn sidebar_setting_row(
        &self,
        id: String,
        title: String,
        description: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let shown = id == "all"
            || !self.home.prefs["hidden_folders"][&id]
                .as_bool()
                .unwrap_or(false);
        let pinned = self.home.prefs["pinned_folders"][&id]
            .as_bool()
            .unwrap_or(false);
        div()
            .py(px(17.))
            .border_b_1()
            .border_color(rgb(BORDER))
            .flex()
            .items_center()
            .gap(px(14.))
            .child(
                Icon::default()
                    .path("icons/book-open.svg")
                    .size(px(17.))
                    .text_color(rgb(MUTED)),
            )
            .child(
                div()
                    .text_size(px(14.))
                    .font_weight(FontWeight::MEDIUM)
                    .child(title.clone()),
            )
            .child(muted(description).flex_1())
            .when(id != "all", |d| {
                d.child(
                    Button::new(SharedString::from(format!("pin-{id}")))
                        .ghost()
                        .icon(Icon::default().path("icons/pin.svg").size(px(16.)))
                        .tooltip("Pin folder to the top")
                        .text_color(rgb(if pinned { ACCENT } else { MUTED }))
                        .on_click(cx.listener({
                            let id = id.clone();
                            move |this, _, _, cx| {
                                this.home.prefs["pinned_folders"][&id] = json!(!pinned);
                                this.save_home_prefs();
                                cx.notify();
                            }
                        })),
                )
            })
            .child(
                Switch::new(SharedString::from(format!("show-{id}")))
                    .checked(shown)
                    .disabled(id == "all")
                    .color(rgb(ACCENT))
                    .accessibility_label(format!("Show {title}"))
                    .on_click(cx.listener(move |this, checked: &bool, _, cx| {
                        this.home.prefs["hidden_folders"][&id] = json!(!*checked);
                        if !*checked && this.home.filter == id {
                            this.home.filter = "all".into();
                        }
                        this.save_home_prefs();
                        cx.notify();
                    })),
            )
            .into_any_element()
    }
    fn cloud_settings(&self, cx: &mut Context<Self>) -> AnyElement {
        let path = self.library.as_ref().map(|l| l.root.clone());
        let drive = path
            .as_ref()
            .and_then(|p| self.settings.drives.iter().find(|(_, d)| p.starts_with(d)))
            .map(|(n, _)| n.clone());
        let mut body=div().flex().flex_col().child(div().text_color(rgb(0x625e55)).line_height(relative(1.8)).child("A cloud sync folder lives on your computer, and the cloud client uploads what is inside it. Move your library there and your papers, translations and notes follow you to any computer signed in to the same drive."))
            .child(section("T h i s   c o m p u t e r"))
            .child(div().p(px(22.)).rounded(px(16.)).border_1().border_color(rgb(BORDER)).flex().items_center().gap(px(18.))
                .child(div().size(px(54.)).rounded(px(14.)).bg(rgb(0xecf0ef)).flex().items_center().justify_center().child(Icon::default().path("icons/folder.svg").size(px(25.)).text_color(rgb(ACCENT))))
                .child(div().flex_1().min_w_0().flex().flex_col().gap(px(10.)).child(div().text_size(px(14.)).font_weight(FontWeight::MEDIUM).child(drive.map(|d|format!("In {d} sync folder")).unwrap_or("On this computer, not synced".into()))).child(muted(path.as_ref().map(|p|p.display().to_string()).unwrap_or("Snapshot mode".into()))))
                .child(outline("open-library-folder","Open folder").disabled(path.is_none()).on_click(cx.listener(|this,_,_,cx|{if let Some(l)=&this.library{cx.reveal_path(&l.root)}})))
                .child(primary("move-library","Move…").disabled(self.busy||path.is_none()).on_click(cx.listener(|this,_,window,cx|this.choose_library_folder(window,cx)))))
            .child(div().mt(px(24.)).flex().items_center().child(section("C l o u d   d r i v e s").mt_0().flex_1()).child(Button::new("check-drives").ghost().label("Check again").text_color(rgb(ACCENT)).on_click(cx.listener(|this,_,_,cx|{this.settings.drives=preferences::cloud_drives();cx.notify();}))));
        for (i, (name, drive)) in self.settings.drives.iter().enumerate() {
            body = body.child(
                div()
                    .mb(px(12.))
                    .p(px(20.))
                    .rounded(px(14.))
                    .border_1()
                    .border_color(rgb(BORDER))
                    .flex()
                    .items_center()
                    .gap(px(16.))
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap(px(8.))
                            .child(div().font_weight(FontWeight::SEMIBOLD).child(name.clone()))
                            .child(muted(drive.display().to_string())),
                    )
                    .child(
                        outline(("cloud-drive", i), "Use folder…")
                            .disabled(self.busy)
                            .on_click(cx.listener({
                                let drive = drive.clone();
                                move |this, _, _, cx| {
                                    this.capture_inputs(cx);
                                    this.capture_position();
                                    this.flush_session();
                                    this.queue_margin_note();
                                    this.relocate_library(drive.clone(), cx)
                                }
                            })),
                    ),
            );
        }
        body.child(div().p(px(22.)).rounded(px(16.)).border_1().border_color(rgb(BORDER)).flex().items_center().gap(px(20.))
            .child(div().flex_1().flex().flex_col().gap(px(12.)).child(div().text_size(px(14.)).font_weight(FontWeight::MEDIUM).child("Other cloud drives")).child(muted(if self.settings.drives.is_empty(){"No OneDrive, Dropbox or iCloud found. Nutstore, Google Drive and others: choose their sync folder"}else{"Nutstore, Google Drive and others: choose their sync folder"})))
            .child(outline("choose-cloud-folder","Choose folder…").disabled(self.busy||path.is_none()).on_click(cx.listener(|this,_,window,cx|this.choose_library_folder(window,cx)))))
            .child(section("G o o d   t o   k n o w"))
            .children(["Moving copies the library; the original folder is kept.","Avoid opening Vellio on two computers at once: changes to the same paper may overwrite each other.","Space-saving modes in iCloud and Google Drive may leave files online only. Set the Vellio folder to always stay on this device.","Baidu Netdisk and Xunlei do not provide the live synced folders needed for a library."].map(|s|div().mb(px(10.)).flex().gap(px(10.)).child(muted("•")).child(muted(s).flex_1().line_height(relative(1.8)))))
            .into_any_element()
    }
    fn choose_library_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.capture_inputs(cx);
        self.capture_position();
        self.flush_session();
        self.queue_margin_note();
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(
                "Choose a sync folder — creates Vellio/library; keeps the current library".into(),
            ),
        });
        cx.spawn_in(window, async move |this, window| {
            if let Ok(Ok(Some(paths))) = receiver.await
                && let Some(folder) = paths.first()
            {
                let folder = folder.clone();
                let _ = this.update_in(window, |this, _, cx| this.relocate_library(folder, cx));
            }
        })
        .detach();
    }
    fn relocate_library(&mut self, folder: PathBuf, cx: &mut Context<Self>) {
        let Some(library) = self.library.clone() else {
            return;
        };
        if self.busy {
            return;
        }
        let destination = folder.join("Vellio/library");
        let id = self.workspace.as_ref().map(|w| w.id().to_owned());
        self.settings.relocating = true;
        self.settings.notice = "Copying library…".into();
        let writer = std::mem::take(&mut self.margin_writer);
        self.worker(cx, move |cancel, tx| {
            drop(writer);
            let result = (|| -> anyhow::Result<_> {
                let parent = destination.parent().unwrap();
                std::fs::create_dir_all(parent)?;
                preferences::copy_library(&library.root, &destination, &cancel)?;
                let library = Library::open(destination.clone())?;
                let workspace = id.map(|id| library.workspace(&id)).transpose()?;
                let raw = workspace.as_ref().map(Workspace::state).transpose()?;
                preferences::remember_library(&library.root)?;
                Ok((library, workspace, raw))
            })();
            match result {
                Ok((library, workspace, raw)) => {
                    let _ = tx.send(Event::Relocated(library, workspace, raw));
                }
                Err(e) => {
                    let _ = tx.send(Event::SettingsResult(format!(
                        "Move failed; original library retained: {e}"
                    )));
                }
            }
        });
    }
    fn shortcut_settings(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = &self.settings.draft;
        let mut body=div().flex().flex_col().child(div().flex().items_center().gap(px(24.)).mb(px(12.)).child(div().flex_1().flex().flex_col().gap(px(10.)).child(div().text_size(px(14.)).font_weight(FontWeight::MEDIUM).child("Enable keyboard shortcuts")).child(muted("When off, only Esc works. Single-letter shortcuts never fire while you’re typing, but do fire when the page has focus.").line_height(relative(1.7))))
            .child(Switch::new("shortcuts-enabled").checked(p.shortcuts_enabled).color(rgb(ACCENT)).accessibility_label("Enable keyboard shortcuts").on_click(cx.listener(|this,checked:&bool,_,cx|{this.settings.draft.shortcuts_enabled = *checked;cx.notify()}))));
        let mut group = "";
        for (id, label, _, category) in SHORTCUTS {
            if category != group {
                group = category;
                body = body.child(section(category));
            }
            let key = p.shortcuts.get(id).cloned().unwrap_or_default();
            let recording = self.settings.recording.as_deref() == Some(id);
            body = body.child(
                div()
                    .h(px(46.))
                    .flex()
                    .items_center()
                    .gap(px(14.))
                    .child(
                        div()
                            .flex_1()
                            .child(label)
                            .when(!p.action_enabled(id), |d| d.child(muted(" (feature off)"))),
                    )
                    .child(
                        outline(
                            SharedString::from(format!("shortcut-{id}")),
                            if recording {
                                "Press a key…".into()
                            } else if key.is_empty() {
                                "Not set".into()
                            } else {
                                key.to_uppercase()
                            },
                        )
                        .w(px(96.))
                        .bg(rgb(0xf6f3ec))
                        .disabled(id == "retranslate")
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                this.settings.recording = Some(id.into());
                                this.settings.focus.focus(window, cx);
                                cx.notify();
                            },
                        )),
                    )
                    .child(
                        Button::new(SharedString::from(format!("clear-shortcut-{id}")))
                            .ghost()
                            .label("Clear")
                            .text_color(rgb(MUTED))
                            .disabled(id == "retranslate")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.settings
                                    .draft
                                    .shortcuts
                                    .insert(id.into(), String::new());
                                cx.notify()
                            })),
                    ),
            );
        }
        body.child(div().mt(px(22.)).flex().items_center().gap(px(20.)).child(muted("After selecting text: 1–4 highlight in four colors, N adds a note, Q asks a question. Follows the main switch.").line_height(relative(1.7)).flex_1())
            .child(Button::new("reset-shortcuts").ghost().label("Reset shortcuts").on_click(cx.listener(|this,_,_,cx|{this.settings.draft.shortcuts=preferences::default_shortcuts();this.settings.recording=None;cx.notify();})))).into_any_element()
    }
    pub(super) fn settings_modal(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let panel = self.settings_view(window, cx);
        Modal::new(cx)
            .focus_handle(self.settings.focus.clone())
            .close_on_escape(false)
            .close_on_backdrop_press(false)
            .flex()
            .items_center()
            .justify_center()
            .backdrop(
                div()
                    .size_full()
                    .occlude()
                    .bg(rgba(0x00000044))
                    .on_scroll_wheel(|_, _, cx| cx.stop_propagation()),
            )
            .popup(panel)
            .into_any_element()
    }
    pub(super) fn settings_view(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let viewport = window.viewport_size();
        let modal_side = (viewport.width - px(64.))
            .min(viewport.height - px(96.))
            .max(px(0.))
            .min(px(840.));
        let modal_width = modal_side;
        let modal_height = modal_side;
        let compact = modal_width < px(960.);
        let body = if self.settings.log_open {
            div()
                .flex()
                .flex_col()
                .gap(px(12.))
                .child(section("Log"))
                .child(muted(self.status.clone()))
                .children(self.settings.logs.iter().rev().map(|s| muted(s.clone())))
                .into_any_element()
        } else {
            match self.settings.tab {
                0 => self.model_settings(cx),
                1 => self.reading_settings(cx),
                2 => self.sidebar_settings(cx),
                3 => self.cloud_settings(cx),
                _ => self.shortcut_settings(cx),
            }
        };
        div()
            .id("settings-surface")
            .occlude()
            .w(modal_width)
            .h(modal_height)
            .flex_shrink_0()
            .rounded(px(18.))
            .overflow_hidden()
            .shadow(vec![
                BoxShadow::new(px(0.), px(18.), rgba(0x403a2d40).into()).blur_radius(px(44.)),
            ])
            .flex()
            .flex_col()
            .bg(rgb(PAPER))
            .text_color(rgb(INK))
            .text_size(px(typography::UI_SIZE))
            .line_height(relative(typography::UI_LINE_HEIGHT))
            .capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                let key = &event.keystroke.key;
                if let Some(id) = this.settings.recording.take() {
                    if key != "escape" {
                        let normalized = key.to_lowercase();
                        let conflict =
                            this.settings.draft.shortcuts.iter().any(|(other, k)| {
                                other != &id && !k.is_empty() && k == &normalized
                            });
                        if normalized.chars().count() != 1
                            || conflict
                            || matches!(normalized.as_str(), "1" | "2" | "3" | "4")
                        {
                            this.settings.notice =
                                "Choose one unused key; 1–4 are reserved for highlights.".into();
                        } else {
                            this.settings.draft.shortcuts.insert(id, normalized);
                            this.settings.notice.clear();
                        }
                    }
                    cx.stop_propagation();
                    cx.notify();
                } else if key == "escape" {
                    if this.settings.model_edit.is_some() {
                        this.settings.model_edit = None;
                        cx.notify();
                    } else {
                        this.close_settings(false, window, cx)
                    }
                    cx.stop_propagation();
                }
            }))
            .child(
                div()
                    .id("settings-tabs")
                    .overflow_x_scroll()
                    .h(px(72.))
                    .flex_shrink_0()
                    .px(px(28.))
                    .flex()
                    .items_center()
                    .gap(px(if compact { 8. } else { 22. }))
                    .border_b_1()
                    .border_color(rgb(BORDER))
                    .child(
                        div()
                            .font_family(gpui_kit::component::Theme::global(cx).font_family.clone())
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_size(px(20.))
                            .mr(px(6.))
                            .flex_shrink_0()
                            .child("Settings"),
                    )
                    .children(TABS.into_iter().enumerate().map(|(i, title)| {
                        Button::new(("settings-tab", i))
                            .ghost()
                            .h_full()
                            .px(px(if compact { 8. } else { 14. }))
                            .flex_shrink_0()
                            .rounded_none()
                            .text_size(px(14.))
                            .border_b_2()
                            .border_color(if i == self.settings.tab && !self.settings.log_open {
                                rgb(ACCENT)
                            } else {
                                rgba(0x00000000)
                            })
                            .text_color(rgb(if i == self.settings.tab && !self.settings.log_open {
                                INK
                            } else {
                                MUTED
                            }))
                            .label(title)
                            .disabled(self.settings.relocating)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.settings.tab = i;
                                this.settings.scroll = ScrollHandle::new();
                                this.settings.log_open = false;
                                this.settings.model_edit = None;
                                this.settings.recording = None;
                                this.settings.focus.focus(window, cx);
                                cx.notify();
                            }))
                    })),
            )
            .child(
                div()
                    .id("settings-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&self.settings.scroll)
                    .px(px(28.))
                    .py(px(26.))
                    .child(body),
            )
            .when(!self.settings.notice.is_empty(), |d| {
                d.child(
                    div()
                        .flex_shrink_0()
                        .px(px(28.))
                        .py(px(10.))
                        .bg(rgb(0xf0eee6))
                        .text_size(px(13.))
                        .text_color(rgb(0x625e55))
                        .child(self.settings.notice.clone())
                        .when(self.busy, |d| {
                            d.child(
                                Button::new("cancel-settings-task")
                                    .ghost()
                                    .small()
                                    .label("Cancel task")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.cancel.store(true, Ordering::Relaxed);
                                        cx.notify()
                                    })),
                            )
                        }),
                )
            })
            .child(
                div()
                    .h(px(76.))
                    .flex_shrink_0()
                    .px(px(28.))
                    .flex()
                    .items_center()
                    .gap(px(18.))
                    .border_t_1()
                    .border_color(rgb(BORDER))
                    .child(
                        Button::new("settings-log")
                            .ghost()
                            .label("Log")
                            .text_color(rgb(ACCENT))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.settings.log_open = !this.settings.log_open;
                                this.settings.scroll = ScrollHandle::new();
                                cx.notify()
                            })),
                    )
                    .child(div().flex_1())
                    .when(self.settings.tab == 0, |d| {
                        d.child(
                            Button::new("settings-close")
                                .ghost()
                                .label("Close")
                                .disabled(self.settings.relocating)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.close_settings(false, window, cx)
                                })),
                        )
                    })
                    .when(self.settings.tab != 0, |d| {
                        d.child(
                            Button::new("settings-cancel")
                                .ghost()
                                .label("Cancel")
                                .disabled(self.settings.relocating)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.close_settings(false, window, cx)
                                })),
                        )
                        .child(
                            primary("settings-save", "Save")
                                .bg(rgb(INK))
                                .disabled(self.settings.relocating)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.close_settings(true, window, cx)
                                })),
                        )
                    }),
            )
            .into_any_element()
    }
}
fn font_label(font: ReadingFont) -> &'static str {
    match font {
        ReadingFont::NotoSerifSc => "宋体 · Noto Serif SC",
        ReadingFont::SystemSerif => "System serif",
        ReadingFont::SansSerif => "Sans serif",
    }
}
fn effort_label(effort: &str) -> &'static str {
    match effort {
        "low" => "Low",
        "medium" => "Medium",
        "high" => "High",
        _ => "Default",
    }
}

impl ReaderView {
    pub(super) fn reader_shortcut(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let modifiers = event.keystroke.modifiers;
        if modifiers.control
            || modifiers.platform
            || modifiers.alt
            || modifiers.function
            || self.busy
            || self.settings.open
            || self.navigation.is_some()
            || self.appearance_open
            || self.reading_order_open
        {
            return;
        }
        let typing = [
            self.editor.read(cx).focus_handle(cx),
            self.note.read(cx).focus_handle(cx),
            self.quote.read(cx).focus_handle(cx),
            self.question.read(cx).focus_handle(cx),
            self.search.read(cx).focus_handle(cx),
            self.margin_input.read(cx).focus_handle(cx),
            self.paper_note_input.read(cx).focus_handle(cx),
        ]
        .iter()
        .any(|h| h.is_focused(window));
        if typing {
            return;
        }
        let key = event.keystroke.key.to_lowercase();
        if self.settings.committed.shortcuts_enabled
            && self.selection.is_some()
            && let Ok(number) = key.parse::<usize>()
            && (1..=4).contains(&number)
        {
            self.selection_action("highlight", HighlightColor::ALL[number - 1], window, cx);
            cx.stop_propagation();
            return;
        }
        if self.is_epub()
            && self.settings.committed.shortcuts_enabled
            && self.settings.committed.action(&key).is_none()
            && matches!(key.as_str(), "left" | "right" | "pageup" | "pagedown")
        {
            self.turn_epub_page(matches!(key.as_str(), "right" | "pagedown"), cx);
            cx.stop_propagation();
            return;
        }
        let Some(action) = self.settings.committed.action(&key).map(str::to_owned) else {
            return;
        };
        let index = self
            .list
            .logical_scroll_top()
            .item_ix
            .min(self.state.paper.blocks.len().saturating_sub(1));
        if self.state.paper.blocks.is_empty() {
            return;
        }
        match action.as_str() {
            "next" | "previous" => {
                let next = if action == "next" {
                    (index + 1).min(self.state.paper.blocks.len() - 1)
                } else {
                    index.saturating_sub(1)
                };
                self.select(next, window, cx);
                self.list.scroll_to(ListOffset {
                    item_ix: next,
                    offset_in_item: px(0.),
                });
            }
            "contents" => {
                self.navigation = if self.navigation == Some(Navigation::Contents) {
                    None
                } else {
                    Some(Navigation::Contents)
                };
                if self.navigation.is_some() {
                    self.navigation_focus.focus(window, cx);
                }
            }
            "bilingual" => {
                self.toggle_bilingual(cx);
            }
            "larger" | "smaller" | "reset_size" => {
                self.appearance.font_size = if action == "reset_size" {
                    Appearance::default().font_size
                } else {
                    self.appearance.font_size + if action == "larger" { 1. } else { -1. }
                };
                self.appearance.normalize();
                self.appearance_changed(cx);
            }
            "notes" => self.show_panel(Panel::Notes, window, cx),
            "ask" => {
                if self.selection.is_some() {
                    self.selection_action("chat", HighlightColor::Yellow, window, cx)
                } else {
                    self.select(index, window, cx);
                    self.show_panel(Panel::Chat, window, cx);
                    self.question.update(cx, |input, cx| {
                        input.set_value(format!("> {}\n\n", self.text_at(index)), window, cx)
                    });
                    self.question.read(cx).focus_handle(cx).focus(window, cx);
                }
            }
            "edit" => {
                let index = self
                    .active_reading_paragraph
                    .map(|(block, _, _)| block)
                    .unwrap_or(index);
                self.begin_inline_edit(index, window, cx);
            }
            "page" | "locate_page" => {
                if action == "page" && self.show_pdf {
                    self.show_pdf = false;
                    self.reset_rows();
                } else {
                    self.locate_source(index, cx);
                }
            }
            "previous_page" | "next_page" if self.is_epub() => {
                self.turn_epub_page(action == "next_page", cx);
            }
            "previous_page" | "next_page" => {
                self.show_pdf = true;
                self.panel_open = false;
                self.page_block = None;
                let pages = self.state.paper.meta.pages.len().max(1);
                self.page = if action == "previous_page" {
                    self.page.saturating_sub(1).max(1)
                } else {
                    (self.page + 1).min(pages)
                };
                self.load_page();
            }
            "note" | "question" => {
                let kind = if action == "note" { "note" } else { "question" };
                if self.selection.is_some() {
                    self.selection_action(kind, HighlightColor::Yellow, window, cx)
                } else {
                    self.select(index, window, cx);
                    let block = &self.state.paper.blocks[index];
                    let selection = Selection {
                        block: index,
                        key: block.id.clone(),
                        quote: String::new(),
                        prefix: String::new(),
                        suffix: String::new(),
                        position: point(px(0.), px(0.)),
                        underline: false,
                    };
                    self.open_margin_note(selection, kind, HighlightColor::Yellow, window, cx);
                }
            }
            "original" => {
                if self.selection.is_some() {
                    self.selection_action("original", HighlightColor::Yellow, window, cx)
                } else {
                    self.toggle_block_language(index, cx);
                }
            }
            "copy" => {
                if self.selection.is_some() {
                    self.selection_action("copy", HighlightColor::Yellow, window, cx)
                } else {
                    cx.write_to_clipboard(ClipboardItem::new_string(self.text_at(index)));
                }
            }
            _ => {}
        }
        self.capture_position();
        self.flush_session();
        cx.stop_propagation();
        cx.notify();
    }
}
impl ReaderView {
    pub(super) fn translation_confirmation_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let pages = self.state.paper.meta.pages.len().max(
            self.state
                .paper
                .blocks
                .iter()
                .map(|b| b.page)
                .max()
                .unwrap_or(0),
        );
        let model = self
            .settings
            .committed
            .models
            .iter()
            .find(|m| m.id == self.settings.committed.translation_model)
            .map(|m| m.name.as_str())
            .unwrap_or("configured model");
        div().size_full().flex().items_center().justify_center().bg(rgb(PAPER)).text_color(rgb(INK))
            .child(div().w(px(480.)).p(px(28.)).border_1().border_color(rgb(BORDER)).rounded(px(16.)).flex().flex_col().gap(px(20.))
                .child(div().text_size(px(22.)).font_weight(FontWeight::SEMIBOLD).child("Translate this paper?"))
                .child(muted(format!("This paper has {pages} pages. Untranslated text will be sent to {model}. Completed translations, edits and notes are retained.")))
                .child(div().flex().justify_end().gap(px(12.))
                    .child(outline("cancel-translation","Cancel").on_click(cx.listener(|this,_,_,cx|{this.settings.translation_confirm=false;cx.notify()})))
                    .child(primary("confirm-translation","Translate").on_click(cx.listener(|this,_,_,cx|{this.settings.translation_confirm=false;this.start_translate(cx);cx.notify()})))))
            .capture_key_down(cx.listener(|this,event:&KeyDownEvent,_,cx|{if event.keystroke.key=="escape"{this.settings.translation_confirm=false;cx.stop_propagation();cx.notify();}})).into_any_element()
    }
}
