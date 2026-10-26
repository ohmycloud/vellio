use super::*;

#[test]
fn prose_heading_and_caption_images_survive_reflow_and_repeated_letters() {
    let mut glyphs = test_lines(&[
        ("You read the introduction.", 12., false, false),
        ("Celebration", 12., true, false),
        ("Congratulations! You did it.", 12., false, false),
        ("Figure 1. You did it.", 10., false, false),
    ]);
    let index = |text: &str| {
        glyphs
            .iter()
            .map(|g| g.ch)
            .collect::<String>()
            .find(text)
            .unwrap()
    };
    let heading = index("Celebration");
    let prose = index("You did it.");
    let caption = index("Figure 1.") + "Figure 1. ".len();
    for g in &mut glyphs[caption - "Figure 1. ".len()..] {
        g.italic = true;
    }
    let image = "<img src=\"data:image/png;base64,AA==\" alt=\"\">";
    for i in [heading, prose, caption] {
        glyphs[i].image_before = Some(image.into());
    }
    let html = render(&glyphs, &[]).unwrap();
    assert_eq!(html.matches("<img ").count(), 3);
    assert!(html.contains("<h3><span><img"), "{html}");
    assert!(html.contains("Congratulations! <span><img"), "{html}");
    assert!(html.contains("Figure 1. <span><img"), "{html}");
    assert!(html.contains("You read the introduction."));
}

#[test]
fn italic_labels_support_roman_markers_indentation_and_escaped_text() {
    let mut glyphs = test_lines(&[
        ("  A) Tools & <Agents>", 12., false, false),
        ("The ordinary body follows.", 12., false, false),
    ]);
    for g in glyphs.iter_mut().take_while(|g| g.ch != '\n').skip(5) {
        g.italic = true;
    }
    let html = render(&glyphs, &[]).unwrap();
    assert!(html.starts_with("<p><em>A) Tools &amp; &lt;Agents&gt;</em></p>\n<p>"));
}

#[test]
fn section_heading_before_smaller_subheading_stays_out_of_the_previous_paragraph() {
    let glyphs = test_lines(&[
        (
            "Label Smoothing During training we smooth labels.",
            10.,
            false,
            false,
        ),
        ("This improves accuracy and BLEU score.", 10., false, false),
        ("6 Results", 12., true, false),
        ("6.1 Machine Translation", 10., true, false),
        (
            "On the translation task the model improves.",
            10.,
            false,
            false,
        ),
    ]);
    let source: String = glyphs.iter().map(|g| g.ch).collect();
    let html = render(&glyphs, &[]).unwrap();
    let heading = html.find("<h3>6 Results</h3>").expect(&html);
    assert!(html[..heading].ends_with("</p>"), "{html}");
    assert!(
        html[heading..].starts_with("<h3>6 Results</h3>\n<h3>6.1 Machine Translation</h3>\n<p>"),
        "{html}"
    );
    assert_eq!(html.matches("<p>").count(), 2, "{html}");
    assert_eq!(glyphs.iter().map(|g| g.ch).collect::<String>(), source);
    // A smaller bold prefix followed by normal prose is not a subheading.
    let mut mixed = glyphs.clone();
    let line = mixed.split_mut(|g| g.ch == '\n').nth(3).unwrap();
    for g in &mut line[4..] {
        g.bold = false;
    }
    assert!(typographic_heading(&mixed.split(|g| g.ch == '\n').collect::<Vec<_>>(), 2).is_none());
}

#[test]
fn bold_unnumbered_title_and_raised_marker_keep_prose_boundaries() {
    let mut glyphs = test_lines(&[
        ("Let’s find out!", 12., false, false),
        ("ViViT", 12., true, false),
        ("An early example is called (ViViT).", 12., false, false),
        ("8 This method uses Transformer models.", 12., false, false),
        (
            "8 Footnote text stays on its own baseline.",
            9.,
            false,
            false,
        ),
    ]);
    let marker = glyphs.iter().position(|g| g.ch == '8').unwrap();
    let previous = glyphs[marker - 2].clone();
    glyphs[marker].size = 9.;
    glyphs[marker].y = previous.y + 4.2;
    glyphs[marker].x = previous.x + 6.;
    glyphs[marker].link = Some("https://example.org/reference".into());
    let html = render(&glyphs, &[]).unwrap();
    assert!(
        html.contains("</p><h3>ViViT</h3>\n<p><span>An early"),
        "{html}"
    );
    assert!(html.contains("(ViViT). <a href="), "{html}");
    assert!(
        html.contains("<sup>8</sup></span></a> This method"),
        "{html}"
    );
    assert_eq!(html.matches("<sup>").count(), 1);
    let marked = crate::annotations::decorate_interactive(
        &html,
        &[serde_json::json!({
            "id":"sup-note", "quote":"8 This method", "kind":"underline", "color":"blue"
        })],
    );
    assert!(marked.contains("<sup>"));
    assert_eq!(
        crate::annotations::plain_text(&marked),
        crate::annotations::plain_text(&html)
    );
}

#[test]
fn ordinary_small_numbers_and_inline_bold_are_not_titles_or_superscripts() {
    let mut glyphs = test_lines(&[("There are 8 examples.", 12., false, false)]);
    for g in &mut glyphs[0..5] {
        g.bold = true;
    }
    glyphs[10].size = 9.;
    let html = render(&glyphs, &[]).unwrap();
    assert!(html.contains("<strong>There</strong>"));
    assert!(!html.contains("<h") && !html.contains("<sup>"));
    let html = render(
        &test_lines(&[
            ("Bold sentence.", 12., true, false),
            ("Body", 12., false, false),
        ]),
        &[],
    )
    .unwrap();
    assert!(html.contains("<strong>Bold sentence.</strong>"));
    assert!(!html.contains("<h") && !html.contains("<sup>"));
}

#[test]
fn heading_lookahead_past_end_of_page_returns_none() {
    let gs = test_lines(&[("Last line", 12., false, false)]);
    let lines: Vec<_> = gs.split(|g| g.ch == '\n').collect();
    let headings = [crate::pdf::OutlineEntry {
        title: "Last line".into(),
        page: 1,
        depth: 0,
    }];
    for start in [lines.len(), lines.len() + 1] {
        assert!(bookmarked_heading(&lines, start, &headings).is_none());
        assert!(typographic_heading(&lines, start).is_none());
        assert!(reading_heading(&lines, start, &headings).is_none());
    }
    assert!(reading_heading(&[], 0, &headings).is_none());
}

#[test]
fn bookmarked_headings_preserve_code_and_escape_title_markup() {
    let glyphs = test_lines(&[
        ("Prior prose.", 10., false, false),
        ("Tools & <Agents>", 16., true, false),
        ("print(\"hello\")", 10., false, true),
        ("Following prose.", 10., false, false),
    ]);
    let headings = [crate::pdf::OutlineEntry {
        page: 1,
        title: "Tools & <Agents>".into(),
        depth: 1,
    }];
    let html = render(&glyphs, &headings).unwrap();
    assert!(html.contains("</p><h2>Tools &amp; &lt;Agents&gt;</h2>\n<pre><code>"));
    assert!(html.contains("</code></pre><p><span>Following prose."));
    assert_eq!(html.matches("<pre>").count(), 1);
}

#[test]
fn title_matching_does_not_consume_body_text_or_monospace_code() {
    let headings = [crate::pdf::OutlineEntry {
        page: 1,
        title: "Building Code Agents".into(),
        depth: 1,
    }];
    for lines in [
        vec![("We discuss Building Code Agents here.", 10., false, false)],
        vec![
            ("Building Code", 16., true, false),
            ("Agents", 10., false, false),
        ],
        vec![
            ("Building Code", 16., true, false),
            ("", 16., true, false),
            ("Agents", 16., true, false),
        ],
    ] {
        let html = render(&test_lines(&lines), &headings).unwrap_or_default();
        assert!(!html.contains("<h"), "{html}");
    }
    let html = render(
        &test_lines(&[("Building Code Agents", 10., false, true)]),
        &headings,
    )
    .unwrap();
    assert!(html.contains("<pre><code>"));
    assert!(!html.contains("<h2>"));
}

#[test]
fn reconstructs_indentation_blank_lines_source_colors_and_prose_boundaries() {
    let mut glyphs = vec![];
    for (text, mono, x, y) in [
        ("Chapter 4. Memory", false, 72., 754.),
        ("Explanation", false, 72., 730.),
        ("messages = [", true, 84., 700.),
        ("{\"role\": \"user\"},", true, 108., 688.),
        ("]", true, 84., 676.),
        ("print(messages)", true, 84., 652.),
        ("This gives us:", false, 72., 628.),
    ] {
        for (i, ch) in text.chars().chain(std::iter::once('\n')).enumerate() {
            glyphs.push(Glyph {
                ch,
                mono,
                x: x + i as f32 * 6.,
                right: x + (i + 1) as f32 * 6.,
                advance_right: x + (i + 1) as f32 * 6.,
                y,
                size: 10.,
                color: if ch == '"' { 0xff3300 } else { 0x0000aa },
                italic: false,
                bold: false,
                image_before: None,
                image_after: None,
                link: None,
            });
        }
    }
    let html = render(&glyphs, &[]).unwrap();
    assert!(html.contains("<h1>Chapter 4. Memory</h1>\n<p><span>Explanation</span></p>"));
    assert_eq!(html.matches("<pre>").count(), 1);
    assert!(html.contains("\n    <span"));
    assert!(html.contains("\n\n<span"));
    assert!(html.contains("color:#ff3300"));
    let plain = crate::annotations::plain_text(&html);
    assert!(plain.contains("messages = [\n    {\"role\": \"user\"},\n]\n\nprint(messages)"));
    assert!(plain.contains("This gives us:"));
    assert_eq!(
        render(
            &[Glyph {
                ch: 'x',
                mono: false,
                x: 0.,
                right: 6.,
                advance_right: 6.,
                y: 0.,
                size: 10.,
                color: 0,
                italic: false,
                bold: false,
                image_before: None,
                image_after: None,
                link: None
            }],
            &[]
        ),
        None
    );
}
