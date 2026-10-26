//! Settings reading.
use super::*;

impl ReaderView {
    pub(in crate::desktop) fn reading_settings(&self, cx: &mut Context<Self>) -> AnyElement {
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
}
