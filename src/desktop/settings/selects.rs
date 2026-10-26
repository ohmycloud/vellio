//! Settings selects.
use super::*;

impl ReaderView {
    pub(in crate::desktop) fn settings_select_state(
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

    pub(in crate::desktop) fn init_settings_selects(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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

    pub(in crate::desktop) fn setting_selected(
        &mut self,
        id: &str,
        value: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
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

    pub(in crate::desktop) fn settings_select(
        &self,
        id: &'static str,
        label: &'static str,
    ) -> AnyElement {
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
}
