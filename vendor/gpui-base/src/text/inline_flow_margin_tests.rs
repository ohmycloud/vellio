//! Bold runs must wrap with their painted metrics, not the regular body font.
use super::super::inline::test_fonts::{BODY, WideMonoTextSystem};
use super::*;
use gpui::{AbsoluteLength, Empty, FontWeight, HighlightStyle, SharedUri, TestApp};

#[test]
fn mixed_bold_cjk_and_atomic_math_sized_object_never_clip_line_end_characters() {
    let mut app = TestApp::with_text_system(Arc::new(WideMonoTextSystem));
    let mut window = app.open_window(|_, _| Empty);
    let bold = "Additive attention（加性注意力）用一个小型神经网络";
    let tail = "，给查询与每个键的匹配程度打分。完整的中文与 English tail。";
    let style = TextStyle {
        font_family: BODY.into(),
        font_size: AbsoluteLength::Pixels(px(13.5)),
        ..Default::default()
    };
    for width in [px(150.), px(248.), px(420.)] {
        let items = vec![
            MeasureItem::Text {
                text: bold.into(),
                links: vec![],
                highlights: vec![(
                    0..bold.len(),
                    InlineHighlight {
                        style: HighlightStyle {
                            font_weight: Some(FontWeight::BOLD),
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                )],
            },
            // A fixed-size leaf exercises the same mixed flow as an inline
            // formula; no network image or external math process is needed.
            MeasureItem::Image {
                source: SharedUri::from("https://example.invalid/formula").into(),
                width: None,
                height: None,
            },
            MeasureItem::Text {
                text: tail.into(),
                links: vec![],
                highlights: vec![],
            },
        ];
        let layout = window.update(|_, window, cx| {
            layout_flow(
                &items,
                &[None, Some(size(px(18.), px(14.))), None],
                &style,
                Some(width),
                window,
                cx,
            )
        });
        assert!(
            layout.size.width <= width + px(0.01),
            "painted bold width {:?} exceeds column {width:?}",
            layout.size.width
        );
        let mut restored = String::new();
        for fragment in &layout.fragments {
            if let PositionedFragment::Text {
                text, origin, size, ..
            } = fragment
            {
                assert!(
                    origin.x + size.width <= width + px(0.01),
                    "glyph fragment {text:?} crosses the right clipping edge"
                );
                restored.push_str(text);
            }
        }
        assert_eq!(
            restored,
            format!("{bold}{tail}"),
            "Every UTF-8 source character remains laid out exactly once"
        );
    }
}
