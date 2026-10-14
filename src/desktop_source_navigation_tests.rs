//! Source navigation and compact floating toolbar layout regressions.
use super::{Panel, ReaderView, ReadingMode, Workspace};
use gpui_kit::{
    AppContext, Modifiers, TestAppContext, VisualTestContext, component::Root, px, size,
};
use serde_json::json;

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..24 {
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.simulate_next_frame(cx);
            window.draw(cx).clear(cx);
        });
    }
}

#[gpui_kit::test]
fn pdf_gutter_and_toolbar_locate_layout_page_and_replace_tool_panel(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let temp = tempfile::tempdir().unwrap();
    let workspace = Workspace {
        root: temp.path().to_owned(),
    };
    image::RgbaImage::from_pixel(24, 32, image::Rgba([255, 255, 255, 255]))
        .save(workspace.root.join("page.webp"))
        .unwrap();
    let raw = json!({"paper":{"meta":{"title_en":"Source navigation", "format":"pdf",
        "pages":(0..9).map(|_| json!({"img":"page.webp", "w":612,"h":792})).collect::<Vec<_>>()},
        "blocks":[
            {"id":"first", "type":"para", "page":1, "en":"The first paragraph has selectable text."},
            {"id":"target", "type":"para", "page":2, "en":"The target paragraph belongs to physical PDF page seven.\n\nIts second paragraph also points to the same source page."}
        ]}, "layout":{"target":{"page":7, "box":[0.1,0.2,0.8,0.4]}}, "reader":{}});
    let original = raw["paper"].clone();
    let mut reader = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| ReaderView::new(raw, None, Some(workspace), window, cx));
        reader = Some(view.clone());
        Root::new(view, window, cx)
    });
    let reader = reader.unwrap();
    reader.update(cx, |r, cx| r.set_reading_mode(ReadingMode::Translation, cx));
    settle(cx);
    reader.read_with(cx, |r, _| assert_eq!(r.source_page_number(1), Some(7)));
    let gutter = cx
        .debug_bounds("source-page-gutter-1")
        .expect("right-side p.7 button");
    let paragraph = reader.read_with(cx, |r, cx| {
        r.text_views[&(1, false)]
            .1
            .read(cx)
            .paragraph_bounds(0)
            .unwrap()
    });
    assert!(
        gutter.left() >= paragraph.right(),
        "page link must not overlap prose"
    );
    reader.update(cx, |r, cx| {
        r.panel_open = true;
        r.panel = Panel::Edit;
        cx.notify();
    });
    settle(cx);
    let gutter = cx.debug_bounds("source-page-gutter-1").unwrap();
    cx.simulate_click(gutter.center(), Modifiers::default());
    settle(cx);
    reader.read_with(cx, |r, _| {
        assert!(r.show_pdf);
        assert!(!r.panel_open);
        assert_eq!(r.page, 7);
        assert_eq!(r.page_block, Some(1));
        assert!(r.page_image.is_some());
        assert!(
            r.selection.is_none(),
            "gutter click must not capture body selection"
        );
        assert_eq!(r.raw["paper"], original);
    });
    assert!(cx.debug_bounds("source-panel").is_some());
    reader.update(cx, |r, cx| {
        r.show_pdf = false;
        r.reset_rows();
        cx.notify();
    });
    settle(cx);
    let paragraph = reader.read_with(cx, |r, cx| {
        r.text_views[&(1, false)]
            .1
            .read(cx)
            .paragraph_bounds(1)
            .unwrap()
    });
    cx.simulate_click(paragraph.center(), Modifiers::default());
    settle(cx);
    assert!(cx.debug_bounds("selection-toolbar").is_some());
    let source = cx
        .debug_bounds("selection-source-page")
        .expect("PDF p.7 in toolbar");
    let gutter = cx.debug_bounds("source-page-gutter-1").unwrap();
    assert!(
        (gutter.top() - paragraph.top()).abs() < px(5.),
        "gutter follows the active paragraph"
    );
    cx.simulate_click(source.center(), Modifiers::default());
    settle(cx);
    reader.read_with(cx, |r, _| {
        assert!(r.show_pdf);
        assert_eq!(r.page, 7);
        assert_eq!(r.page_block, Some(1));
        assert!(r.selection.is_none());
    });
}

#[gpui_kit::test]
fn epub_toolbar_opens_original_without_inventing_physical_pages(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let raw = json!({"paper":{"meta":{"title_en":"EPUB source", "format":"epub", "pages":[]},
        "blocks":[{"id":"paragraph", "type":"para", "page":0, "chapter":1,
            "source_kind":"epub-xhtml", "html":"<p>Original EPUB paragraph.</p>",
            "en":"Original EPUB paragraph.", "zh":"对应的中文译文。",
            "source_location":{"href":"chapter.xhtml", "dom":"body/1"}}]}, "reader":{}});
    let mut reader = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| ReaderView::new(raw, None, None, window, cx));
        reader = Some(view.clone());
        Root::new(view, window, cx)
    });
    let reader = reader.unwrap();
    reader.update(cx, |r, cx| r.set_reading_mode(ReadingMode::Translation, cx));
    settle(cx);
    reader.read_with(cx, |r, _| {
        assert!(r.epub_reflow.complete);
        assert_eq!(r.source_page_number(0), Some(1));
        assert!(r.can_locate_source(0));
    });
    let paragraph = reader.read_with(cx, |r, cx| {
        r.text_views[&(0, false)]
            .1
            .read(cx)
            .paragraph_bounds(0)
            .unwrap()
    });
    cx.simulate_click(paragraph.center(), Modifiers::default());
    settle(cx);
    let source = cx.debug_bounds("selection-source-page").unwrap();
    cx.simulate_click(source.center(), Modifiers::default());
    settle(cx);
    assert!(cx.debug_bounds("epub-source-content").is_some());
    reader.read_with(cx, |r, _| {
        assert!(r.show_pdf);
        assert_eq!(r.page_block, Some(0));
        assert_eq!(r.page, 1);
        assert!(
            r.reader_body_html(0, true)
                .contains("Original EPUB paragraph.")
        );
        assert!(r.state.paper.meta.pages.is_empty());
        assert_eq!(r.state.paper.blocks[0].page, 0);
    });
    reader.update(cx, |r, cx| {
        r.settings.committed.features.page = false;
        r.enforce_features();
        cx.notify();
    });
    settle(cx);
    assert!(cx.debug_bounds("source-page-gutter-0").is_none());
    assert!(cx.debug_bounds("source-panel").is_none());
}

#[gpui_kit::test]
fn selection_toolbar_sizes_to_enabled_actions_without_spreading_buttons(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let raw = json!({"paper":{"meta":{"format":"pdf", "pages":[{}]},
        "blocks":[{"id":"paragraph", "type":"para", "page":1,
            "en":"Original paragraph.", "zh":"可修订的译文。"}]},"reader":{}});
    let mut reader = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| ReaderView::new(raw, None, None, window, cx));
        reader = Some(view.clone());
        Root::new(view, window, cx)
    });
    let reader = reader.unwrap();
    cx.simulate_resize(size(px(1200.), px(700.)));
    reader.update(cx, |r, cx| r.set_reading_mode(ReadingMode::Translation, cx));
    settle(cx);
    let paragraph = reader.read_with(cx, |r, cx| {
        r.text_views[&(0, false)]
            .1
            .read(cx)
            .paragraph_bounds(0)
            .unwrap()
    });
    cx.simulate_click(paragraph.center(), Modifiers::default());
    settle(cx);
    let toolbar = cx.debug_bounds("selection-toolbar").unwrap();
    assert_eq!(toolbar.size, size(px(680.), px(42.)));
    let language = cx.debug_bounds("selection-language-toggle").unwrap();
    let edit = cx.debug_bounds("selection-edit-translation").unwrap();
    let source = cx.debug_bounds("selection-source-page").unwrap();
    assert_eq!(edit.left() - language.right(), px(2.));
    assert_eq!(source.left() - edit.right(), px(2.));
    assert_eq!(toolbar.right() - source.right(), px(37.)); // Copy + gap + padding.

    for flags in 0..16 {
        reader.update(cx, |r, cx| {
            let features = &mut r.settings.committed.features;
            features.ask_ai = flags & 1 != 0;
            features.original = flags & 2 != 0;
            features.edit = flags & 4 != 0;
            features.page = flags & 8 != 0;
            cx.notify();
        });
        settle(cx);
        let expected = 356.
            + if flags & 1 != 0 { 72. } else { 0. }
            + if flags & 2 != 0 { 82. } else { 0. }
            + if flags & 4 != 0 { 78. } else { 0. }
            + if flags & 8 != 0 { 92. } else { 0. };
        let toolbar = cx.debug_bounds("selection-toolbar").unwrap();
        assert_eq!(toolbar.size.width, px(expected), "feature flags {flags}");
        for (flag, selector) in [
            (2, "selection-language-toggle"),
            (4, "selection-edit-translation"),
            (8, "selection-source-page"),
        ] {
            let action = cx.debug_bounds(selector);
            assert_eq!(action.is_some(), flags & flag != 0);
            if let Some(action) = action {
                assert!(action.left() >= toolbar.left());
                assert!(action.right() <= toolbar.right());
            }
        }
    }
}

#[test]
fn reader_toolbar_omits_settings_while_library_keeps_its_entry() {
    let reader = include_str!("desktop.rs");
    assert!(!reader.contains("Button::new(\"reader-settings\")"));
    assert!(
        !reader.contains("icons/settings.svg"),
        "no settings gear in the reader toolbar"
    );
    let home = include_str!("desktop_home.rs");
    assert!(home.contains("Button::new(\"home-settings\")"));
    assert!(home.contains("this.open_settings(window, cx)"));
}

#[test]
fn selection_toolbar_does_not_register_hover_tooltips() {
    let source = include_str!("desktop.rs");
    let toolbar = source
        .split("    fn selection_toolbar(")
        .nth(1)
        .unwrap()
        .split("    fn edit_annotation(")
        .next()
        .unwrap();
    assert!(!toolbar.contains(".tooltip("));
    assert!(toolbar.contains(".accessibility_label("));
}

#[gpui_kit::test]
fn compact_toolbar_keeps_source_action_inside_and_edit_translation_accessible(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let raw = json!({"paper":{"meta":{"format":"pdf", "pages":[{}]},
        "blocks":[{"id":"paragraph", "type":"para", "page":1, "en":"Original paragraph.", "zh":"可修订的译文。"}]},"reader":{}});
    let mut reader = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| ReaderView::new(raw, None, None, window, cx));
        reader = Some(view.clone());
        Root::new(view, window, cx)
    });
    let reader = reader.unwrap();
    reader.update(cx, |r, cx| r.set_reading_mode(ReadingMode::Translation, cx));
    cx.simulate_resize(size(px(640.), px(700.)));
    settle(cx);
    let paragraph = reader.read_with(cx, |r, cx| {
        r.text_views[&(0, false)]
            .1
            .read(cx)
            .paragraph_bounds(0)
            .unwrap()
    });
    cx.simulate_click(paragraph.center(), Modifiers::default());
    settle(cx);
    let toolbar = cx.debug_bounds("selection-toolbar").unwrap();
    let source = cx.debug_bounds("selection-source-page").unwrap();
    assert_eq!(toolbar.size, size(px(332.), px(40.)));
    assert!(toolbar.contains(&source.center()));
    assert!(source.right() <= toolbar.right());
    let edit = cx.debug_bounds("selection-edit-translation").unwrap();
    cx.simulate_click(edit.center(), Modifiers::default());
    settle(cx);
    reader.read_with(cx, |r, cx| {
        assert!(!r.panel_open);
        assert_eq!(r.inline_edit.as_ref().unwrap().block, 0);
        assert_eq!(
            r.inline_edit
                .as_ref()
                .unwrap()
                .input
                .read(cx)
                .value()
                .as_str(),
            "可修订的译文。"
        );
    });
}
