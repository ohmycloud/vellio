//! A book made of many short blocks reproduces first-paint notifications on
//! every turn; a single long paragraph cannot expose that invalidation bug.
use super::ReaderView;
use gpui_kit::{AppContext, TestAppContext, VisualTestContext, component::Root};
use serde_json::json;

fn frame(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.simulate_next_frame(cx);
        window.draw(cx).clear(cx);
    });
    cx.run_until_parked();
}

#[gpui_kit::test]
fn completed_epub_page_map_is_immutable_while_turning_unpainted_blocks(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let blocks: Vec<_> = (0..36).map(|i| {
        let text = format!("Paragraph {i}: {}", "Native pagination must stay stable while turning pages. ".repeat(8));
        json!({"id":format!("p{i}"),"type":"para","source_kind":"epub-xhtml","chapter":1,"en":text,"html":format!("<p>{text}</p>")})
    }).collect();
    let raw = json!({"paper":{"meta":{"format":"epub","title_en":"Stable page map"},"blocks":blocks},"reader":{}});
    let mut reader = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            let mut reader = ReaderView::new(raw.clone(), None, None, window, cx);
            reader.reading_mode = super::ReadingMode::Translation;
            reader
        });
        reader = Some(view.clone());
        Root::new(view, window, cx)
    });
    let reader = reader.unwrap();
    reader.update(cx, |reader, cx| {
        assert!(!reader.epub_reflow.complete);
        assert!(
            reader.epub_reflow.pages.is_empty(),
            "do not publish a partial total"
        );
        assert_eq!(reader.epub_page_label(), "正在排版……");
        let current = reader.epub_reflow.current;
        reader.turn_epub_page(true, cx);
        assert_eq!(
            reader.epub_reflow.current, current,
            "only a completed map is navigable"
        );
    });
    for _ in 0..80 {
        frame(cx);
    }
    let (pages, measurements, builds) = reader.read_with(cx, |reader, _| {
        assert!(reader.epub_reflow.complete);
        assert!(reader.epub_reflow.pages.len() > 8);
        assert_eq!(
            reader.epub_reflow.measured_blocks, 36,
            "each initial block is measured exactly once"
        );
        (
            reader.epub_reflow.pages.clone(),
            reader.epub_reflow.measured_blocks,
            reader.epub_reflow.page_map_builds,
        )
    });
    // Saving a new underline color sends a full reader snapshot. It must not
    // discard the completed page map or briefly leave the current page empty.
    let mut colored = raw.clone();
    colored["reader"]["notes"] = json!({"mark": {
        "id":"mark", "anchor":"p0", "quote":"Native pagination",
        "kind":"highlight", "style":"underline", "color":"blue"
    }});
    reader.update(cx, |reader, _| reader.refresh_state(colored));
    reader.read_with(cx, |reader, cx| {
        assert!(reader.epub_reflow.complete);
        assert_eq!(reader.epub_reflow.pages, pages);
        assert!(
            reader
                .text_views
                .get(&(0, false))
                .is_some_and(|(_, view)| view.read(cx).is_content_ready())
        );
    });
    frame(cx);
    reader.read_with(cx, |reader, cx| {
        assert_eq!(reader.epub_reflow.pages, pages);
        assert_eq!(reader.epub_reflow.measured_blocks, measurements);
        assert_eq!(reader.epub_reflow.page_map_builds, builds);
        let (source, view) = reader.text_views.get(&(0, false)).unwrap();
        assert!(source.contains("text-decoration-color:"), "{source}");
        assert!(view.read(cx).is_content_ready());
    });
    // Navigation markup is a decoration, not a content/layout revision.
    reader.update(cx, |reader, cx| {
        reader.navigation_note = Some(json!({"id":"temporary-navigation","anchor":"p0","quote":"Native pagination must stay stable"}));
        cx.notify();
    });
    frame(cx);
    for target in (1..pages.len()).chain((0..pages.len() - 1).rev()) {
        let forward = reader.read_with(cx, |reader, _| target > reader.epub_reflow.current);
        reader.update(cx, |reader, cx| reader.turn_epub_page(forward, cx));
        for _ in 0..3 {
            frame(cx);
            reader.read_with(cx, |reader, _| {
                assert!(
                    reader.epub_reflow.complete,
                    "turning to page {target} must not return to preparing"
                );
                assert_eq!(
                    reader.epub_reflow.pages, pages,
                    "all page boundaries and total must remain unchanged"
                );
                assert_eq!(reader.epub_reflow.current, target);
                assert_eq!(
                    reader.epub_page_label(),
                    format!("{} / {}", target + 1, pages.len())
                );
                assert_eq!(
                    reader.epub_reflow.measured_blocks, measurements,
                    "a page turn must not remeasure the book"
                );
                assert_eq!(
                    reader.epub_reflow.page_map_builds, builds,
                    "a page turn must not rebuild the page map"
                );
            });
        }
    }
    reader.update(cx, |reader, cx| {
        reader.appearance.font_size += 3.;
        reader.appearance_changed(cx);
    });
    for _ in 0..80 {
        frame(cx);
    }
    reader.read_with(cx, |reader, _| {
        assert!(reader.epub_reflow.complete);
        assert_ne!(
            reader.epub_reflow.pages, pages,
            "a real layout change must still repaginate"
        );
        assert!(reader.epub_reflow.page_map_builds > builds);
    });
    let mut edited = raw;
    edited["reader"]["edits"] = json!({"p0": {"zh": "A genuinely different paragraph."}});
    reader.update(cx, |reader, _| {
        reader.refresh_state(edited);
        assert!(
            !reader.epub_reflow.complete,
            "text edits must still repaginate"
        );
    });
}
