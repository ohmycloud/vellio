use super::*;

#[test]
fn multi_reference_superscript_recovers_the_entire_raised_run_across_links() {
    let mut glyphs = test_lines(&[
        ("There are three connectors:", 12., false, false),
        ("12,13,123", 9., false, false),
        ("Following body.", 12., false, false),
        ("12,13 Normal small footnote text.", 9., false, false),
    ]);
    let marker = glyphs.iter().position(|g| g.ch == '1').unwrap();
    let anchor = glyphs[marker - 2].clone();
    for (offset, g) in glyphs[marker..marker + 9].iter_mut().enumerate() {
        g.x = anchor.x + 3. + offset as f32 * 5.;
        g.y = anchor.y + 4.2;
        g.link = match offset {
            0..=1 => Some("easyread-pdf:4:100".into()),
            3..=4 => Some("easyread-pdf:4:80".into()),
            _ => None,
        };
    }
    assert!(
        superscripts(&glyphs)[marker..marker + 9]
            .iter()
            .all(|script| *script)
    );
    let html = render(&glyphs, &[]).unwrap();
    assert!(
        html.contains("<sup>12</sup></span></a><sup>,</sup><a"),
        "{html}"
    );
    assert!(
        html.contains("<sup>13</sup></span></a><sup>,123</sup>"),
        "{html}"
    );
    assert!(!html.contains("<sup>12,13 Normal"));
    let normalize = |text: &str| text.split_whitespace().collect::<Vec<_>>().join(" ");
    let source: String = glyphs.iter().map(|g| g.ch).collect();
    assert_eq!(
        normalize(&crate::annotations::plain_text(&html)),
        normalize(&source)
    );
}

#[test]
fn wrapped_link_rectangle_excludes_surrounding_body_text() {
    let mut glyphs = test_lines(&[
        ("We truncated the prediction. GitHub", 10., false, false),
        (
            "repository). The pass rate gives a score of 1.",
            10.,
            false,
            false,
        ),
    ]);
    let mut start = 0;
    for label in ["GitHub", "repository"] {
        let source: String = glyphs.iter().map(|g| g.ch).collect();
        let index = source[start..].find(label).unwrap() + start;
        for g in &mut glyphs[index..index + label.len()] {
            g.color = 0x8e0012;
        }
        start = index + label.len();
    }
    let rect = PdfRect::new_from_values(718., 72., 750., 400.);
    let selected = link_glyphs(&glyphs, &[rect], true);
    assert_eq!(
        selected.iter().map(|&i| glyphs[i].ch).collect::<String>(),
        "GitHubrepository"
    );
    for index in selected {
        glyphs[index].link = Some("https://example.org/repo".into());
    }
    let html = render(&glyphs, &[]).unwrap();
    assert!(html.contains(">GitHub</span></a> <a href="));
    assert!(html.contains(">repository</span></a>). The pass rate"));
    assert!(!html.contains(">We truncated</span></a>"));
    assert_eq!(
        crate::annotations::plain_text(&html),
        "We truncated the prediction. GitHub repository). The pass rate gives a score of 1."
    );

    for g in &mut glyphs {
        g.color = 0;
    }
    assert!(link_glyphs(&glyphs, &[rect], true).is_empty());
}

#[test]
fn explicit_line_regions_preserve_uncolored_wrapped_links() {
    let glyphs = test_lines(&[
        ("Prose GitHub", 10., false, false),
        ("repository after", 10., false, false),
    ]);
    let regions = [
        PdfRect::new_from_values(738., 108., 750., 144.),
        PdfRect::new_from_values(718., 72., 730., 132.),
    ];
    assert_eq!(
        link_glyphs(&glyphs, &regions, false)
            .iter()
            .map(|&i| glyphs[i].ch)
            .collect::<String>(),
        "GitHubrepository"
    );
    assert_eq!(link_glyphs(&glyphs, &regions[..1], true).len(), 6);
}

#[test]
fn prose_only_links_render_without_code_headings_or_figures() {
    let mut glyphs = test_lines(&[("See Figure 4-1 for details.", 10., false, false)]);
    for glyph in &mut glyphs[4..14] {
        glyph.link = Some("easyread-pdf:1:680".into());
        glyph.color = 0x8e0012;
    }
    let html = render(&glyphs, &[]).unwrap();
    assert!(html.contains("href=\"easyread-pdf:1:680\""));
    assert!(html.contains("color:#8e0012"));
    assert_eq!(
        crate::annotations::plain_text(&html),
        "See Figure 4-1 for details."
    );
    assert!(!html.contains("<pre>"));
}
