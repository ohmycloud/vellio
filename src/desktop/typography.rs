//! Typography from EasyRead's web/css/{base,library,reader,settings}.css.
//! Resolve CSS font stacks against installed faces instead of using a CJK face
//! for English prose. GPUI handles missing glyph fallback within each face.
use super::*;

pub(super) const READER_INK: u32 = 0x23211d;
pub(super) const READER_MUTED_INK: u32 = 0x5a554c;
pub(super) const UI_SIZE: f32 = 14.;
pub(super) const UI_LINE_HEIGHT: f32 = 1.6;
pub(super) const SECONDARY_SCALE: f32 = 0.84;
pub(super) const CAPTION_SCALE: f32 = 0.86;
pub(super) const SECONDARY_LINE_HEIGHT: f32 = 1.66;
pub(super) const CAPTION_LINE_HEIGHT: f32 = 1.75;
pub(super) const TABLE_LINE_HEIGHT: f32 = 1.5;
pub(super) const CODE_SCALE: f32 = 0.86;

pub(super) struct Fonts {
    pub serif_zh: SharedString,
    pub serif_en: SharedString,
    pub system_serif_zh: SharedString,
    pub sans_zh: SharedString,
    ui_zh: SharedString,
    ui_en: SharedString,
    mono: SharedString,
}

fn choose(installed: &[String], stack: &[&str], fallback: &str) -> SharedString {
    stack
        .iter()
        .copied()
        .find(|name| installed.iter().any(|font| font == name))
        .unwrap_or(fallback)
        .to_owned()
        .into()
}

impl Fonts {
    pub fn resolve(cx: &App) -> Self {
        let installed = cx.text_system().all_font_names();
        let choose = |stack: &[&str], fallback| choose(&installed, stack, fallback);
        Self {
            serif_zh: choose(
                &[
                    "Noto Serif SC",
                    "Source Han Serif SC",
                    "Source Han Serif CN",
                    "Songti SC",
                    "STSong",
                ],
                "Noto Serif SC",
            ),
            serif_en: choose(
                &[
                    "Source Serif 4",
                    "Source Serif Pro",
                    "Charter",
                    "Iowan Old Style",
                    "Georgia",
                    "Noto Serif SC",
                ],
                "Times New Roman",
            ),
            system_serif_zh: choose(
                &["Songti SC", "STSong", "Source Han Serif SC"],
                "Noto Serif SC",
            ),
            sans_zh: choose(
                &[
                    "Noto Sans SC",
                    "Source Han Sans SC",
                    "PingFang SC",
                    "Microsoft YaHei UI",
                    "Microsoft YaHei",
                ],
                ".SystemUIFont",
            ),
            ui_zh: choose(
                &[
                    "Noto Sans SC",
                    "Segoe UI",
                    "PingFang SC",
                    "Microsoft YaHei UI",
                ],
                ".SystemUIFont",
            ),
            // CSS system-ui precedes Helvetica/Arial in the English UI stack.
            ui_en: choose(&["Segoe UI"], ".SystemUIFont"),
            mono: choose(
                &[
                    "JetBrains Mono",
                    "Cascadia Mono",
                    "Consolas",
                    "Menlo",
                    "DejaVu Sans Mono",
                    "Liberation Mono",
                ],
                ".SystemUIFont",
            ),
        }
    }

    pub fn reading(&self, font: ReadingFont, english: bool) -> SharedString {
        match font {
            ReadingFont::SansSerif => self.sans_zh.clone(),
            _ if english => self.serif_en.clone(),
            ReadingFont::SystemSerif => self.system_serif_zh.clone(),
            ReadingFont::NotoSerifSc => self.serif_zh.clone(),
        }
    }

    pub fn title(&self, text: &str) -> SharedString {
        if text
            .chars()
            .any(|ch| matches!(ch as u32, 0x3400..=0x9fff | 0xf900..=0xfaff | 0x20000..=0x3134f))
        {
            self.serif_zh.clone()
        } else {
            self.serif_en.clone()
        }
    }
}

/// Theme changes reinstall GPUI's defaults; call this afterwards so menus,
/// input controls and rich-text code all retain EasyRead's typography.
pub(super) fn apply_theme(zh: bool, cx: &mut App) {
    let fonts = Fonts::resolve(cx);
    gpui_kit::component::Theme::update(cx, |theme| {
        theme.font_family = if zh { fonts.ui_zh } else { fonts.ui_en };
        theme.font_size = px(UI_SIZE);
        theme.mono_font_family = fonts.mono;
    });
}

/// TextView applies its own foreground rather than inheriting the reader's
/// color. Override theme defaults here, including after theme/menu changes.
pub(super) fn reading_style(style: gpui_kit::base::TextViewStyle) -> gpui_kit::base::TextViewStyle {
    style
        .with_foreground(rgb(READER_INK).into())
        .with_muted_foreground(rgb(READER_MUTED_INK).into())
}

/// One native rich-text style for normal prose and the unchanged fragments
/// surrounding an inline editor. Editing must not restyle the rest of a page.
pub(super) fn reader_rich_style(
    reader: &ReaderView,
    index: usize,
    original: bool,
    family: SharedString,
    text_size: f32,
    cx: &App,
) -> gpui_kit::base::TextViewStyle {
    let pdf = reader
        .state
        .paper
        .blocks
        .get(index)
        .is_some_and(|block| block.source_kind == "pdf-text");
    let latin = reader.appearance.reading_font() == ReadingFont::NotoSerifSc;
    reading_style(
        gpui_kit::base::TextViewDefaults::global(cx)
            .style()
            .cloned()
            .unwrap_or_default(),
    )
    .with_code_block(
        StyleRefinement::default()
            .bg(rgba(0x00000000))
            .text_size(px(text_size * CODE_SCALE)),
    )
    .with_inline_code(HighlightStyle {
        background_color: Some(rgba(0x00000000).into()),
        font_weight: pdf.then_some(FontWeight::BOLD),
        ..Default::default()
    })
    .with_inline_code_font_size_scale(if pdf { 1.0 } else { CODE_SCALE })
    .with_paragraph_indent_font_size(px(text_size))
    .with_unordered_list_marker_color(rgb(0x8c8579).into())
    .with_link_underline(false)
    .with_heading(move |level| {
        StyleRefinement::default()
            .font_weight(FontWeight::SEMIBOLD)
            .line_height(relative(1.5))
            .text_size(px(text_size * heading_scale(level)))
    })
    .with_companion(
        StyleRefinement::default()
            .border_l_0()
            .px_0()
            .font_family(
                reader
                    .fonts
                    .reading(reader.appearance.reading_font(), !original),
            )
            .text_size(px(text_size * SECONDARY_SCALE))
            .line_height(relative(SECONDARY_LINE_HEIGHT)),
    )
    .with_latin_apostrophe_font(latin.then(|| reader.fonts.serif_en.clone()))
    .with_latin_quote_font(latin.then(|| reader.fonts.serif_en.clone()))
    .with_latin_italic_font(latin.then(|| reader.fonts.serif_en.clone()))
    .with_paragraph_gap(rems(0.9))
    .with_table_cell({
        let mut style = StyleRefinement::default();
        style.text.font_size = Some(px(text_size * CAPTION_SCALE).into());
        style.text.line_height = Some(relative(TABLE_LINE_HEIGHT));
        style.padding.left = Some(px(4.).into());
        style.padding.right = Some(px(4.).into());
        style
    })
    .with_table_head({
        let mut style = StyleRefinement::default();
        style.text.font_family = Some(family);
        style.text.font_weight = Some(FontWeight::SEMIBOLD);
        style
    })
}

pub(super) fn text_scale(secondary: bool, caption: bool) -> f32 {
    // A bilingual caption's secondary text is .84em of the .86em caption.
    (if secondary { SECONDARY_SCALE } else { 1. }) * (if caption { CAPTION_SCALE } else { 1. })
}

pub(super) fn heading_scale(level: u8) -> f32 {
    match level {
        1 => 1.28,
        2 => 1.08,
        _ => 1.,
    }
}

#[cfg(test)]
mod tests {
    use super::{Fonts, ReadingFont, choose, heading_scale, text_scale};

    #[test]
    fn font_stacks_use_css_order_not_installation_order() {
        let installed = vec!["Songti SC".into(), "Noto Serif SC".into(), "Charter".into()];
        assert_eq!(
            choose(&installed, &["Noto Serif SC", "Songti SC"], "fallback"),
            "Noto Serif SC"
        );
        assert_eq!(
            choose(&installed, &["Source Serif 4", "Charter"], "fallback"),
            "Charter"
        );
        assert_eq!(choose(&installed, &["missing"], "fallback"), "fallback");
    }

    #[test]
    fn english_prose_uses_latin_serif_without_overriding_user_sans_choice() {
        let fonts = Fonts {
            serif_zh: "Noto Serif SC".into(),
            serif_en: "Charter".into(),
            system_serif_zh: "Songti SC".into(),
            sans_zh: "PingFang SC".into(),
            ui_zh: "PingFang SC".into(),
            ui_en: ".SystemUIFont".into(),
            mono: "Menlo".into(),
        };
        assert_eq!(
            fonts.reading(ReadingFont::NotoSerifSc, false),
            "Noto Serif SC"
        );
        assert_eq!(fonts.reading(ReadingFont::NotoSerifSc, true), "Charter");
        assert_eq!(fonts.reading(ReadingFont::SystemSerif, false), "Songti SC");
        assert_eq!(fonts.reading(ReadingFont::SystemSerif, true), "Charter");
        for english in [false, true] {
            assert_eq!(
                fonts.reading(ReadingFont::SansSerif, english),
                "PingFang SC"
            );
        }
        assert_eq!(fonts.title("全部文献"), "Noto Serif SC");
        assert_eq!(fonts.title("English title"), "Charter");
    }

    #[cfg(feature = "desktop-ui-tests")]
    #[gpui_kit::test]
    fn theme_switches_keep_control_and_code_fonts(cx: &mut gpui_kit::TestAppContext) {
        cx.update(gpui_kit::init);
        cx.update(|cx| {
            for zh in [false, true] {
                gpui_kit::component::Theme::change(gpui_kit::component::ThemeMode::Dark, None, cx);
                super::apply_theme(zh, cx);
                let fonts = Fonts::resolve(cx);
                let theme = gpui_kit::component::Theme::global(cx);
                assert_eq!(theme.font_size, gpui_kit::px(super::UI_SIZE));
                assert_eq!(
                    theme.font_family,
                    if zh { fonts.ui_zh } else { fonts.ui_en }
                );
                assert_eq!(theme.mono_font_family, fonts.mono);
                let base = gpui_kit::base::Theme::global(cx);
                assert_eq!(base.tokens.typography.sans, theme.font_family);
                assert_eq!(base.tokens.typography.mono, theme.mono_font_family);
                let style = super::reading_style(gpui_kit::base::TextViewStyle::from_theme(&base));
                assert_eq!(style.foreground(), gpui_kit::rgb(super::READER_INK).into());
                assert_eq!(
                    style.muted_foreground(),
                    gpui_kit::rgb(super::READER_MUTED_INK).into()
                );
            }
        });
    }

    #[test]
    fn reading_sizes_match_easyread_and_scale_with_user_size() {
        for size in [14., 21., 32.] {
            assert_eq!(size * text_scale(false, false), size);
            assert!((size * text_scale(true, false) - size * 0.84).abs() < 0.001);
            assert!((size * text_scale(false, true) - size * 0.86).abs() < 0.001);
            assert!((size * text_scale(true, true) - size * 0.86 * 0.84).abs() < 0.001);
            assert_eq!(size * heading_scale(1), size * 1.28);
            assert_eq!(size * heading_scale(2), size * 1.08);
        }
    }
}
