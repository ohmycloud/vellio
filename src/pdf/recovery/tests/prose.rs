use super::*;

#[test]
fn ordered_items_keep_literal_numbers_and_annotation_text_on_separate_rows() {
    let gs = test_lines(&[
        ("The following steps:", 12., false, false),
        (
            "1. Embed all external documents the agent has no direct access to.",
            12.,
            false,
            false,
        ),
        ("2. Embed the user’s query.", 12., false, false),
        ("3. Compare the embeddings and create", 12., false, false),
        ("a similarity matrix.", 12., false, false),
        (
            "4. Return the documents with the highest similarity.",
            12.,
            false,
            false,
        ),
        ("5. Add those documents to the prompt.", 12., false, false),
        ("This gives us the following class:", 12., false, false),
    ]);
    let html = render(&gs, &[]).unwrap();
    let paragraphs: Vec<_> = html
        .split("</p>")
        .map(crate::annotations::plain_text)
        .filter(|p| !p.trim().is_empty())
        .collect();
    assert_eq!(
        paragraphs.iter().map(|p| p.trim()).collect::<Vec<_>>(),
        [
            "The following steps:",
            "1. Embed all external documents the agent has no direct access to.",
            "2. Embed the user’s query.",
            "3. Compare the embeddings and create a similarity matrix.",
            "4. Return the documents with the highest similarity.",
            "5. Add those documents to the prompt.",
            "This gives us the following class:",
        ],
        "{html}"
    );
    let marked = crate::annotations::decorate_interactive(
        &html,
        &[
            serde_json::json!({"id":"list-note", "quote":"3. Compare the embeddings and create a similarity matrix.", "kind":"highlight", "color":"blue"}),
        ],
    );
    assert!(marked.contains("list-note"), "{marked}");
    assert_eq!(
        crate::annotations::plain_text(&marked),
        crate::annotations::plain_text(&html)
    );
}

#[test]
fn unnumbered_labels_and_physical_paragraphs_keep_wrapped_body_together() {
    for size in [10., 12., 16.] {
        let mut gs = test_lines(&[
            ("The data", size, false, false),
            ("First paragraph begins", size, false, false),
            ("and wraps without punctuation", size, false, false),
            ("Second paragraph begins", size, false, false),
            ("and also wraps", size, false, false),
            ("Tool-use data", size, false, false),
            ("A tool explanation begins", size, false, false),
            ("and continues on this line", size, false, false),
            ("Software engineering", size, false, false),
            ("tasks", size, false, false),
            ("A repository explanation follows", size, false, false),
        ]);
        let ys = [0., 1.75, 3., 4.75, 6., 8.25, 10., 11.25, 13.5, 14.75, 16.5];
        for (i, line) in gs.split_mut(|g| g.ch == '\n').enumerate() {
            for g in line {
                g.y = 740. - ys[i] * size;
                g.x += if i == 0 {
                    0.
                } else if [6, 7, 10].contains(&i) {
                    3. * size
                } else {
                    1.5 * size
                };
                g.italic = [0, 5, 8, 9].contains(&i);
            }
        }
        let html = render(&gs, &[]).unwrap();
        let ps: Vec<_> = html
            .split("</p>")
            .map(crate::annotations::plain_text)
            .filter(|p| !p.trim().is_empty())
            .collect();
        assert_eq!(
            ps.iter().map(|p| p.trim()).collect::<Vec<_>>(),
            [
                "The data",
                "First paragraph begins and wraps without punctuation",
                "Second paragraph begins and also wraps",
                "Tool-use data",
                "A tool explanation begins and continues on this line",
                "Software engineering tasks",
                "A repository explanation follows"
            ],
            "{html}"
        );
        assert_eq!(html.matches("<p><em>").count(), 3);
        let marked = crate::annotations::decorate_interactive(
            &html,
            &[
                serde_json::json!({"id":"body-note", "quote":"begins and continues", "kind":"highlight", "color":"pink"}),
            ],
        );
        assert!(marked.contains("body-note"));
        assert_eq!(
            crate::annotations::plain_text(&html),
            crate::annotations::plain_text(&marked)
        );
    }
}

#[test]
fn bottom_page_numbers_are_hidden_without_dropping_body_numbers_or_footnotes() {
    for (text, x, y, page, hidden) in [
        ("5", 303., 40., 5, true),
        ("6", 303., 40., 5, false),
        ("(5)", 303., 40., 5, false),
        ("5 experiments", 303., 40., 5, false),
        ("5", 72., 40., 5, false),
        ("5", 303., 500., 5, false),
    ] {
        let mut gs = test_lines(&[
            (
                "The model uses 5 layers and keeps all these body numbers",
                12.,
                false,
                false,
            ),
            (text, 12., false, false),
        ]);
        for row in gs.split_mut(|g| g.ch == '\n').skip(1) {
            for g in row {
                let dx = x - 72.;
                g.x += dx;
                g.right += dx;
                g.advance_right += dx;
                g.y = y;
            }
        }
        let omitted = page_number_lines(&gs, 612., 792., page);
        assert_eq!(omitted.contains(&1), hidden, "{text} at {x},{y}");
        if hidden {
            let html = render_page(&gs, &[], &[], &omitted).unwrap();
            assert_eq!(
                crate::annotations::plain_text(&html).trim(),
                "The model uses 5 layers and keeps all these body numbers"
            );
            let mut nearby = test_lines(&[("A nearby footnote", 12., false, false)]);
            for g in &mut nearby {
                g.y = 55.;
            }
            gs.extend(nearby);
            assert!(page_number_lines(&gs, 612., 792., page).is_empty());
        }
    }
    let mut lone_number = test_lines(&[("5", 12., false, false)]);
    for g in &mut lone_number {
        g.x = 303.;
        g.y = 40.;
    }
    assert!(page_number_lines(&lone_number, 612., 792., 5).is_empty());
}

#[test]
fn merged_physical_rows_use_first_and_last_baselines_for_paragraph_gaps() {
    for continuation in [
        "learned linear transformation and softmax function to convert the decoder output",
        "and continue",
    ] {
        let mut gs = test_lines(&[
            ("Similarly we convert the input", 12., false, false),
            (
                "tokens and output tokens to vectors of dimension dmodel",
                12.,
                false,
                false,
            ),
            (continuation, 12., false, false),
            ("This is still the same paragraph", 12., false, false),
            ("A new indented paragraph begins", 12., false, false),
            ("and wraps to the margin", 12., false, false),
            ("A paragraph after extra space", 12., false, false),
            ("also wraps to the margin", 12., false, false),
        ]);
        for (i, row) in gs.split_mut(|g| g.ch == '\n').enumerate() {
            for g in row.iter_mut() {
                if i == 4 {
                    g.x += 18.;
                    g.right += 18.;
                    g.advance_right += 18.;
                }
                if i >= 6 {
                    g.y -= 10.;
                }
            }
            // Inline subscripts must not become a physical row boundary.
            if i == 1 {
                for g in row.iter_mut().rev().take(5) {
                    g.size = 8.;
                    g.y -= 2.;
                }
            }
        }
        // Simulate PDFium omitting a newline between the two physical rows.
        let newline = gs
            .iter()
            .enumerate()
            .filter(|(_, g)| g.ch == '\n')
            .nth(1)
            .unwrap()
            .0;
        gs[newline].ch = ' ';
        let lines: Vec<_> = gs.split(|g| g.ch == '\n').collect();
        assert_eq!(
            paragraph_breaks(&lines, &Default::default()),
            [false, false, false, true, false, true, false, false],
        );
        let html = render(&gs, &[]).unwrap();
        let paragraphs: Vec<_> = html
            .split("</p>")
            .map(crate::annotations::plain_text)
            .filter(|p| !p.trim().is_empty())
            .collect();
        assert_eq!(paragraphs.len(), 3, "{html}");
        assert_eq!(
            paragraphs[0].trim(),
            format!(
                "Similarly we convert the input tokens and output tokens to vectors of dimension dmodel {continuation} This is still the same paragraph"
            ),
        );
        assert_eq!(
            paragraphs[1].trim(),
            "A new indented paragraph begins and wraps to the margin"
        );
        assert_eq!(
            paragraphs[2].trim(),
            "A paragraph after extra space also wraps to the margin"
        );
    }
}

#[test]
fn physical_paragraph_boundaries_work_without_italic_or_punctuation() {
    let mut gs = test_lines(&[
        ("An ordinary paragraph begins", 12., false, false),
        ("and continues", 12., false, false),
        ("A new indented paragraph", 12., false, false),
        ("returns to the margin", 12., false, false),
        ("A paragraph after extra space", 12., false, false),
    ]);
    for (i, line) in gs.split_mut(|g| g.ch == '\n').enumerate() {
        for g in line {
            g.y = [740., 725., 710., 695., 674.][i];
            if i == 2 {
                g.x += 18.;
            }
        }
    }
    let html = render(&gs, &[]).unwrap();
    assert!(
        html.contains("begins and continues</span></p><p><span>A new"),
        "{html}"
    );
    assert!(
        html.contains("paragraph returns to the margin</span></p><p><span>A paragraph after"),
        "{html}"
    );
}

#[test]
fn lower_case_and_parenthesized_labels_require_an_indented_explanation() {
    for title in ["add", "multiply", "subtract", "(Optional) Generator"] {
        let mut gs = test_lines(&[
            (title, 12., false, false),
            ("An indented explanation follows", 12., false, false),
        ]);
        for (i, line) in gs.split_mut(|g| g.ch == '\n').enumerate() {
            for g in line {
                g.italic = i == 0;
                g.y = 740. - i as f32 * 21.;
                if i == 1 {
                    g.x += 18.;
                }
            }
        }
        assert!(
            render(&gs, &[])
                .unwrap()
                .contains(&format!("<p><em>{title}</em></p>\n<p>"))
        );
        if title.chars().next().unwrap().is_lowercase() {
            for g in gs.split_mut(|g| g.ch == '\n').nth(1).unwrap() {
                g.x -= 18.;
            }
            let lines: Vec<_> = gs.split(|g| g.ch == '\n').collect();
            assert!(italic_label(&lines, 0).is_none());
        }
    }
}

#[test]
fn cross_page_caption_tail_keeps_emphasis_alignment_and_body_boundary() {
    let mut gs = test_lines(&[
        (
            "then pick the best using generated unit tests",
            9.,
            false,
            false,
        ),
        (
            "In a way, the approach works like retrieval",
            12.,
            false,
            false,
        ),
        ("and continues at normal body spacing", 12., false, false),
    ]);
    for (i, line) in gs.split_mut(|g| g.ch == '\n').enumerate() {
        for g in line {
            g.y = [702., 679., 664.][i];
            g.italic = i == 0;
            if i == 0 {
                g.x += 100.;
            }
        }
    }
    let html = render(&gs, &[]).unwrap();
    assert!(html.starts_with("<p style=\"text-align:center\"><em>then pick the best using generated unit tests</em></p><p><span>In a way"), "{html}");
    assert!(html.contains("retrieval and continues"));
    for g in gs.iter_mut().take_while(|g| g.ch != '\n') {
        g.size = 12.;
    }
    let lines: Vec<_> = gs.split(|g| g.ch == '\n').collect();
    assert!(
        !leading_caption(&lines, 0),
        "Ordinary italic body paragraph"
    );
}

#[test]
#[ignore = "requires EASYREAD_QA_PDF pointing to Attention Is All You Need"]
fn embeddings_softmax_stays_one_paragraph_in_real_pdf() -> anyhow::Result<()> {
    let path = std::env::var("EASYREAD_QA_PDF")?;
    let pdfium = Pdfium::new(Pdfium::bind_to_library("assets/pdfium/libpdfium.dylib")?);
    let doc = pdfium.load_pdf_from_file(&path, None)?;
    let page = doc.pages().get(4)?;
    let gs = recover_fragmented_lines(&glyphs(&page.text()?)).0;
    let lines: Vec<_> = gs.split(|g| g.ch == '\n').collect();
    let breaks = paragraph_breaks(&lines, &Default::default());
    let tokens = lines
        .iter()
        .position(|line| line_text(line).starts_with("tokens and output tokens"))
        .unwrap();
    assert!(!breaks[tokens]);
    let headings: Vec<_> = crate::pdf::document_outline(&doc)
        .into_iter()
        .filter(|heading| heading.page == 5)
        .collect();
    let (_, html, _, _, _) = reading_html(&page, &doc, &headings, 5)?;
    let html = html.expect("Expected rich reading presentation");
    let start = html.find("Similarly to other sequence").unwrap();
    let end = html[start..].find("</p>").unwrap() + start;
    assert!(
        html[start..end].contains("tokens and output tokens"),
        "{}",
        &html[start..]
    );
    assert!(
        !crate::annotations::plain_text(&html)
            .trim_end()
            .ends_with("\n5")
    );
    assert!(html.contains(&crate::formula::native_html(
        "\\sqrt{d_{\\mathrm{model}}}",
        false
    )));
    Ok(())
}

#[test]
fn fragmented_baselines_recover_words_without_merging_real_lines_or_code() {
    let caption = "Figure 1. A caption";
    let mut original = test_lines(&[(caption, 12., false, false)]);
    for g in &mut original {
        g.italic = true;
    }
    let mut broken = Vec::new();
    for g in original.iter().filter(|g| !g.ch.is_whitespace()) {
        broken.push(g.clone());
        let mut newline = g.clone();
        newline.ch = '\n';
        broken.push(newline);
    }
    let (recovered, changed) = recover_fragmented_lines(&broken);
    assert!(changed);
    assert_eq!(line_text(&recovered), caption);
    assert!(
        render(&recovered, &[])
            .unwrap()
            .contains(&format!("<em>{caption}</em>"))
    );
    let mut code = broken.clone();
    for g in &mut code {
        g.mono = true;
    }
    assert!(!recover_fragmented_lines(&code).1);
    let separate = test_lines(&[
        ("A", 12., false, false),
        ("B", 12., false, false),
        ("C", 12., false, false),
        ("D", 12., false, false),
    ]);
    assert!(!recover_fragmented_lines(&separate).1);
    let mut columns = broken.clone();
    for (i, g) in columns.iter_mut().enumerate() {
        g.x += (i / 2) as f32 * 100.;
        g.advance_right += (i / 2) as f32 * 100.;
    }
    assert!(!recover_fragmented_lines(&columns).1);
}

#[test]
fn raster_insertion_requires_glyph_size_baseline_and_an_empty_gap() {
    let mut glyphs = test_lines(&[("AB", 12., false, false)]);
    glyphs[1].x = 96.;
    glyphs[1].right = 102.;
    assert_eq!(insertion(&glyphs, [82., 738., 94., 750.]), Some((1, false)));
    assert_eq!(
        insertion(&glyphs, [72., 738., 84., 750.]),
        None,
        "overlaps text"
    );
    assert_eq!(
        insertion(&glyphs, [82., 720., 94., 732.]),
        None,
        "different baseline"
    );
    assert_eq!(insertion(&glyphs, [82., 738., 94., 768.]), None, "too tall");
    assert_eq!(
        insertion(&glyphs, [130., 738., 142., 750.]),
        None,
        "isolated icon"
    );
    assert_eq!(
        insertion(&glyphs, [110., 738., 122., 750.]),
        None,
        "detached margin icon"
    );
}

#[test]
fn pdf_prose_keeps_literal_markdown_punctuation_and_source_fonts() {
    let source = "Use __init__ and *args with [name](value), `literal` and <tag>.";
    let mut gs = test_lines(&[(source, 12., false, false)]);
    for g in gs.iter_mut().take(3) {
        g.bold = true;
    }
    let html = render(&gs, &[]).unwrap();
    assert_eq!(crate::annotations::plain_text(&html).trim(), source);
    assert!(html.contains("<strong>Use</strong>"));
    assert!(!html.contains("<a "));
}

#[test]
fn italic_labels_preserve_nested_boundaries_wraps_and_annotations() {
    for marker in ["1.", "2)", "(3)", "A)", "b.", "(c)", "IV.", "viii)"] {
        let mut glyphs = test_lines(&[
            ("The following behaviors:", 12., false, false),
            (
                &format!("{marker} You hand the agent the file"),
                12.,
                false,
                false,
            ),
            ("(for example, a spreadsheet)", 12., false, false),
            ("A simple tool retrieves the exact file.", 12., false, false),
            (
                "The body continues on the next physical line.",
                12.,
                false,
                false,
            ),
        ]);
        for (i, line) in glyphs.split_mut(|g| g.ch == '\n').enumerate() {
            for g in line {
                g.italic = i == 1 || i == 2;
                g.y = 740. - i as f32 * 15.;
            }
        }
        let html = render(&glyphs, &[]).unwrap();
        assert!(html.contains(&format!("</p><p><em>{marker} You hand the agent the file (for example, a spreadsheet)</em></p>\n<p><span>A simple tool")), "{html}");
        assert!(html.contains("exact file. The body continues"));
        let marked = crate::annotations::decorate_interactive(
            &html,
            &[
                serde_json::json!({"id":"label-note", "quote":"file (for example", "kind":"highlight", "color":"pink"}),
            ],
        );
        assert!(marked.contains("label-note"));
        let source: String = glyphs.iter().map(|g| g.ch).collect();
        let normalize = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
        assert_eq!(
            normalize(&crate::annotations::plain_text(&marked)),
            normalize(&source)
        );
    }
}

#[test]
fn italic_label_detection_requires_a_whole_label_and_physical_boundary() {
    for case in [
        "roman",
        "inline",
        "sentence",
        "version",
        "mixed",
        "code",
        "size",
        "gap",
        "blank",
        "lowercase",
        "unnumbered",
    ] {
        let label = match case {
            "sentence" => "1. This is a full sentence.",
            "version" => "1.2 Release notes",
            "lowercase" => "1. retrieve the data",
            "unnumbered" => "An italic quotation",
            _ => "1. Data retrieval",
        };
        let mut glyphs = test_lines(&[
            ("Prior body line", 12., false, false),
            (label, 12., false, false),
            ("continued label", 12., false, false),
            ("Ordinary body text.", 12., false, false),
        ]);
        for (i, line) in glyphs.split_mut(|g| g.ch == '\n').enumerate() {
            for (j, g) in line.iter_mut().enumerate() {
                g.italic = (i == 1 || i == 2) && case != "roman";
                g.y = 740. - i as f32 * 15.;
                if i == 1 {
                    if case == "inline" {
                        g.y = 740.;
                    }
                    if case == "mixed" && j > 7 {
                        g.italic = false;
                    }
                    if case == "code" {
                        g.mono = true;
                    }
                }
                if i == 2 {
                    if case == "size" {
                        g.size = 10.;
                    }
                    if case == "gap" {
                        g.y -= 40.;
                    }
                    if case == "blank" {
                        g.ch = ' ';
                    }
                }
            }
        }
        let lines: Vec<_> = glyphs.split(|g| g.ch == '\n').collect();
        assert!(italic_label(&lines, 1).is_none(), "{case}");
    }
}

#[test]
fn wrapped_caption_stays_one_paragraph_before_body_without_other_page_features() {
    let mut glyphs = test_lines(&[
        ("Figure 10-2. Coding agents solve complex", 9., false, false),
        ("problems", 9., false, false),
        (
            "The first shows a common pattern of asking LLMs.",
            12.,
            false,
            false,
        ),
    ]);
    for (i, line) in glyphs.split_mut(|g| g.ch == '\n').enumerate() {
        for g in line {
            g.y = 178.2 - i as f32 * 10.8;
            g.italic = i < 2;
        }
    }
    let source: String = glyphs.iter().map(|g| g.ch).collect();
    let html = render(&glyphs, &[]).unwrap();
    assert!(
        html.starts_with(
            "<p style=\"text-align:center\"><em>Figure 10-2. Coding agents solve complex problems</em></p><p>"
        ),
        "{html}"
    );
    let marked = crate::annotations::decorate_interactive(
        &html,
        &[
            serde_json::json!({"id":"caption-note", "quote":"complex problems", "kind":"highlight", "style":"underline", "color":"pink"}),
        ],
    );
    assert!(marked.contains("caption-note"));
    assert!(marked.contains("text-align:center"));
    assert!(marked.contains("text-decoration-color:#cc413a"));
    let normalize = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
    assert_eq!(
        normalize(&crate::annotations::plain_text(&marked)),
        normalize(&source)
    );
}

#[test]
fn caption_continuations_stop_at_style_spacing_and_block_boundaries() {
    for case in [
        "roman", "size", "gap", "blank", "caption", "code", "image", "omitted",
    ] {
        let next = if case == "caption" {
            "Table 2. Another caption"
        } else {
            "separate text"
        };
        let mut glyphs = test_lines(&[
            ("Figure 1. A caption", 10., false, false),
            (next, 10., false, false),
        ]);
        for (i, line) in glyphs.split_mut(|g| g.ch == '\n').enumerate() {
            for g in line {
                g.italic = !(i == 1 && case == "roman");
                g.y = 740. - i as f32 * if case == "gap" { 30. } else { 12. };
                if i == 1 {
                    if case == "size" {
                        g.size = 12.;
                    }
                    if case == "code" {
                        g.mono = true;
                    }
                    if case == "blank" {
                        g.ch = ' ';
                    }
                }
            }
        }
        let figures = if case == "image" {
            vec![(1, "<p><img alt=\"\"></p>".into())]
        } else {
            vec![]
        };
        let omitted = if case == "omitted" {
            [1].into_iter().collect()
        } else {
            Default::default()
        };
        let html = render_page(&glyphs, &[], &figures, &omitted).unwrap();
        assert!(
            html.starts_with("<p style=\"text-align:center\"><em>Figure 1. A caption</em></p>"),
            "{case}: {html}"
        );
    }
}

#[test]
fn full_size_raised_reference_labels_preserve_items_and_wrapped_body() {
    let mut glyphs = test_lines(&[
        (
            "20 Zhang, Susan et al. 2022. OPT: Open Pre-trained Transformer",
            9.,
            false,
            false,
        ),
        ("Language Models, arXiv, 2205.01068.", 9., false, false),
        (
            "21 Bai, Jinze et al. 2023. Qwen-VL: A Frontier Vision-Language Model.",
            9.,
            false,
            false,
        ),
        (
            "22 Alayrac, Jean-Baptiste et al. 2022. Flamingo: A Visual Language Model",
            9.,
            false,
            false,
        ),
        (
            "for Few-Shot Learning, Advances in Neural Information Processing Systems.",
            9.,
            false,
            false,
        ),
    ]);
    for line in glyphs.split_mut(|g| g.ch == '\n') {
        if line.first().is_some_and(|g| g.ch == '2') {
            for g in &mut line[..2] {
                g.y += 3.6;
                g.link = Some("easyread-pdf:1:100".into());
            }
        }
    }
    let source: String = glyphs.iter().map(|g| g.ch).collect();
    let html = render(&glyphs, &[]).unwrap();
    assert_eq!(html.matches("<p>").count(), 3, "{html}");
    for number in [20, 21, 22] {
        assert!(html.contains(&format!("<sup>{number}</sup>")), "{html}");
    }
    assert!(!html.contains("20<sup>22"), "{html}");
    assert!(html.contains("Transformer Language Models"), "{html}");
    assert!(html.contains("Model for Few-Shot"), "{html}");
    let normalize = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
    let marked = crate::annotations::decorate_interactive(
        &html,
        &[serde_json::json!({
            "id":"reference-note", "quote":"22 Alayrac", "prefix":"Model.\n", "color":"green"
        })],
    );
    assert_eq!(
        normalize(&crate::annotations::plain_text(&marked)),
        normalize(&source)
    );
    assert!(marked.contains("<sup>"));
    assert!(marked.contains("data-easyread-note=\"reference-note\""));
}

#[test]
fn same_size_inline_marker_does_not_start_a_paragraph_or_use_other_line_baselines() {
    let mut glyphs = test_lines(&[
        ("Body with reference 12 continues.", 9., false, false),
        ("20 Ordinary label and year 2022.", 9., false, false),
    ]);
    let marker = glyphs.iter().position(|g| g.ch == '1').unwrap();
    for g in &mut glyphs[marker..marker + 2] {
        g.y += 3.6;
    }
    let html = render(&glyphs, &[]).unwrap();
    assert!(html.contains("<sup>12</sup> continues."), "{html}");
    assert!(!html.contains("<sup>20"), "{html}");
    assert!(!html.contains("<sup>2022"), "{html}");
    let mut plain = test_lines(&[
        ("20 A normal label.", 9., false, false),
        ("A neighbouring line.", 9., false, false),
    ]);
    for g in plain.iter_mut().skip(19) {
        g.y = 736.4;
    }
    assert!(render(&plain, &[]).is_none());
}
