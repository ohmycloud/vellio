//! Settings sidebar.
use super::*;

impl ReaderView {
    pub(in crate::desktop) fn sidebar_settings(&self, cx: &mut Context<Self>) -> AnyElement {
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

    pub(in crate::desktop) fn sidebar_setting_row(
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
}
