use super::*;

#[gpui_kit::test]
fn paragraph_annotation_is_continuous_across_bold_inline_code_and_reflow(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let source = "Unmarked lead selected text with **bold words** and *italic words* before `begin_search_query` and `end_search_query` followed by more selected words with a [document link](part.xhtml), then `begin_search_result` and `end_search_result` followed by the final selected words unmarked tail.";
    let plain = easyread_gpui::annotations::plain_text(&easyread_gpui::annotations::html(source));
    let quote = plain
        .strip_prefix("Unmarked lead ")
        .unwrap()
        .strip_suffix(" unmarked tail.")
        .unwrap();
    let raw = json!({"paper":{"meta":{"title_en":"Mixed-font highlight regression"},"blocks":[{"id":"paragraph","type":"para","en":source}]},"reader":{"notes":{"mark":{"id":"mark","anchor":"paragraph","key":"paragraph","kind":"highlight","color":"yellow","quote":quote}}}});
    let original = raw.clone();
    let mut reader = None;
    let (_root, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| translated_reader(raw, None, None, window, cx));
        reader = Some(view.clone());
        Root::new(view, window, cx)
    });
    let reader = reader.unwrap();
    for width in [25., 35.] {
        reader.update(cx, |reader, cx| {
            reader.appearance.line_width = width;
            reader.reset_rows();
            cx.notify();
        });
        draw(cx);
        cx.run_until_parked();
        draw(cx);
        cx.run_until_parked();
        draw(cx);
        let mut rows = cx.update(|window, _| {
            let color: gpui_kit::Background =
                gpui_kit::rgb(easyread_gpui::annotations::HighlightColor::Yellow.rgb()).into();
            let quads = window
                .painted_quads()
                .into_iter()
                .filter(|q| q.background == color)
                .collect::<Vec<_>>();
            assert!(quads.len() > 3);
            assert!(quads.iter().all(|q| q.corner_radii == Default::default()));
            quads.into_iter().map(|q| q.bounds).collect::<Vec<_>>()
        });
        rows.sort_by(|a, b| a.top().partial_cmp(&b.top()).unwrap());
        rows.dedup_by(|a, b| a.top() == b.top());
        for pair in rows.windows(2) {
            assert_eq!(
                pair[0].bottom(),
                pair[1].top(),
                "actual reader highlight must not have mixed-font row gaps: {rows:?}"
            );
        }
        reader.update(cx, |reader, cx| {
            assert_eq!(reader.raw["paper"], original["paper"]);
            assert_eq!(reader.raw["reader"]["notes"], original["reader"]["notes"]);
            let view = reader.text_views[&(0, false)].1.clone();
            assert!(view.read(cx).is_content_ready());
            assert!(view.read(cx).annotation_offset("mark").is_some());
            view.update(cx, |view, cx| view.select_all(cx));
            assert!(view.read(cx).selected_text().contains("begin_search_query"));
        });
    }
}

#[gpui_kit::test]
fn paragraph_selection_is_continuous_across_mixed_fonts_and_reflow(cx: &mut TestAppContext) {
    fn selection_rows(cx: &mut VisualTestContext) -> Vec<gpui_kit::Bounds<gpui_kit::ScaledPixels>> {
        let mut cells = cx.update(|window, cx| {
            let color: gpui_kit::Background = gpui_kit::base::Theme::global(cx)
                .tokens
                .colors
                .selection
                .into();
            window
                .painted_quads()
                .into_iter()
                .filter(|q| {
                    q.background == color && q.bounds.size.width > gpui_kit::ScaledPixels(0.)
                })
                .map(|q| q.bounds)
                .collect::<Vec<_>>()
        });
        assert!(cells.len() > 3);
        cells.sort_by(|a, b| {
            a.top()
                .partial_cmp(&b.top())
                .unwrap()
                .then_with(|| a.left().partial_cmp(&b.left()).unwrap())
        });
        let mut rows: Vec<gpui_kit::Bounds<gpui_kit::ScaledPixels>> = Vec::new();
        for cell in cells {
            if let Some(row) = rows.last_mut().filter(|row| row.top() == cell.top()) {
                assert_eq!(
                    row.bottom(),
                    cell.bottom(),
                    "mixed-font cells must share row height"
                );
                assert!(
                    (row.right() - cell.left()).0.abs() < 0.01,
                    "selection must include inline padding without gaps or overlaps: {row:?}, {cell:?}"
                );
                *row = gpui_kit::Bounds::from_corners(row.origin, cell.bottom_right());
            } else {
                rows.push(cell);
            }
        }
        assert!(rows.len() > 1);
        for pair in rows.windows(2) {
            assert_eq!(
                pair[0].bottom(),
                pair[1].top(),
                "selection must not have mixed-font row gaps: {rows:?}"
            );
        }
        rows
    }

    cx.update(gpui_kit::init);
    let source = "The `SpamDataset` class loads data from the CSV files we created earlier, tokenizes the text using the GPT-2 tokenizer from `tiktoken`, and allows us to *pad* or *truncate* the sequences to a uniform length determined by **the longest sequence**. More **bold words** and ordinary words follow to exercise wrapped selection.";
    let plain = easyread_gpui::annotations::plain_text(&easyread_gpui::annotations::html(source));
    let raw = json!({"paper":{"meta":{"title_en":"Mixed-font selection regression"},"blocks":[{"id":"paragraph","type":"para","en":source}]},"reader":{}});
    let mut reader = None;
    let (_root, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| translated_reader(raw, None, None, window, cx));
        reader = Some(view.clone());
        Root::new(view, window, cx)
    });
    let reader = reader.unwrap();
    for width in [25., 35.] {
        reader.update(cx, |reader, cx| {
            reader.appearance.line_width = width;
            reader.reset_rows();
            cx.notify();
        });
        draw(cx);
        cx.run_until_parked();
        draw(cx);
        reader.update(cx, |reader, cx| {
            let view = reader.text_views[&(0, false)].1.clone();
            view.update(cx, |view, cx| view.select_all(cx));
            assert_eq!(view.read(cx).selected_text().trim_end(), plain);
        });
        draw(cx);
        let rows = selection_rows(cx);
        let first = rows[0];
        let last = *rows.last().unwrap();
        let scale = cx.update(|window, _| window.scale_factor());
        let start = gpui_kit::point(
            px(first.left().0 / scale + 20.),
            px(first.center().y.0 / scale),
        );
        let end = gpui_kit::point(px(last.center().x.0 / scale), px(last.center().y.0 / scale));
        for (anchor, cursor) in [(start, end), (end, start)] {
            cx.simulate_mouse_down(
                anchor,
                gpui_kit::MouseButton::Left,
                gpui_kit::Modifiers::default(),
            );
            draw(cx);
            cx.simulate_mouse_move(
                cursor,
                gpui_kit::MouseButton::Left,
                gpui_kit::Modifiers::default(),
            );
            draw(cx);
            cx.simulate_mouse_up(
                cursor,
                gpui_kit::MouseButton::Left,
                gpui_kit::Modifiers::default(),
            );
            draw(cx);
            let selected = selection_rows(cx);
            assert!(
                selected[0].left() > first.left(),
                "partial selection must not fill the leading unselected text"
            );
            assert!(
                selected.last().unwrap().right() < last.right(),
                "partial selection must not fill the trailing unselected text"
            );
            reader.read_with(cx, |reader, cx| {
                let text = reader.text_views[&(0, false)].1.read(cx).selected_text();
                assert!(!text.trim().is_empty());
                assert!(
                    plain.contains(text.trim_end()),
                    "drag must copy a continuous substring: {text:?}"
                );
            });
        }
    }
}
