//! Settings models.
use super::*;

impl ReaderView {
    pub(in crate::desktop) fn edit_model(
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

    pub(in crate::desktop) fn save_model(&mut self, cx: &mut Context<Self>) {
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

    pub(in crate::desktop) fn model_settings(&self, cx: &mut Context<Self>) -> AnyElement {
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

    pub(in crate::desktop) fn test_model(&mut self, cx: &mut Context<Self>) {
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
}
