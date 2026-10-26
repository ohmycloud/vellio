use super::*;

/// Full-source audit, deliberately separate from parallel fixture tests:
/// PDFium's bindings are process-global. Run with EASYREAD_QA_PDF set.
#[test]
#[ignore]
fn audit_book_structure() -> anyhow::Result<()> {
    let path = std::path::PathBuf::from(std::env::var("EASYREAD_QA_PDF")?);
    let pdfium = Pdfium::new(Pdfium::bind_to_library("assets/pdfium/libpdfium.dylib")?);
    let doc = pdfium.load_pdf_from_file(&path, None)?;
    let headings = crate::pdf::document_outline(&doc);
    let paper: serde_json::Value =
        serde_json::from_slice(&std::fs::read(path.parent().unwrap().join("paper.json"))?)?;
    let output = std::path::Path::new("target/pdf-structure-qa");
    std::fs::create_dir_all(output)?;
    let norm = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut records = Vec::new();
    let mut warnings = Vec::new();
    let mut progress = std::fs::File::create(output.join("progress.jsonl"))?;
    let pages: Vec<usize> = std::env::var("EASYREAD_QA_PAGES")
        .ok()
        .map(|s| s.split(',').map(|n| n.parse().unwrap()).collect())
        .unwrap_or_else(|| (1..=doc.pages().len() as usize).collect());
    for number in pages {
        let page = doc.pages().get(number as i32 - 1)?;
        let (source, presentation, images, _, _) = reading_html(&page, &doc, &headings, number)?;
        if let Some(block) = paper["blocks"]
            .as_array()
            .unwrap()
            .iter()
            .find(|b| b["page"] == number && b["source_kind"] == "pdf-text")
        {
            assert_eq!(
                source,
                block["en"].as_str().unwrap(),
                "Stored text changed on page {number}"
            );
        }
        let html = presentation
            .unwrap_or_else(|| crate::annotations::html(&crate::model::pdf_prose(&source)));
        if number == 542 {
            for label in [
                "The data",
                "Tool-use data",
                "Multi-step data",
                "Unit-test data",
                "Software engineering tasks",
                "Reinforcement learning",
            ] {
                assert!(
                    html.contains(&format!("<p><em>{label}</em></p>")),
                    "Page 542 label: {label}"
                );
            }
            assert!(html.contains("what the model will see when it’s in use, which includes:"));
            assert!(html.contains("<p><span>Coding task data:"));
        }
        if number == 83 {
            assert!(html.contains("<em>Figure 2-19. Each token flows through its own track in the stack, with only the final token’s track passed to the LM head to predict the next token</em>"), "Fragmented caption regression");
        }
        if number == 314 {
            assert!(html.contains("especially context engineering, as we discussed in"));
            assert!(html.contains("easyread-pdf:187:792"));
        }
        if number == 321 {
            for title in ["add", "multiply", "subtract"] {
                assert!(html.contains(&format!("<p><em>{title}</em></p>")));
            }
        }
        if number == 429 {
            assert!(html.contains("<p><em>(Optional) Generator</em></p>"));
        }
        if number == 210 {
            for title in [
                "Conversation history",
                "Summaries of past events",
                "User’s portrait",
            ] {
                assert!(html.contains(&format!("<p><em>{title}</em></p>")));
            }
            assert!(images.contains(&0), "Suppress the legacy whole-panel crop");
            assert!(
                html.contains("pdf-p210-img049"),
                "Keep the real retention graph"
            );
            assert!(!html.contains("pdf-p210-img001"));
        }
        if number == 12 {
            let ps: Vec<_> = html
                .split("</p>")
                .map(crate::annotations::plain_text)
                .collect();
            assert!(
                ps.iter().any(|p| p.trim() == "support@oreilly.com"),
                "Separate email paragraph"
            );
            assert!(
                ps.iter()
                    .any(|p| p.trim() == "https://oreilly.com/about/contact.html"),
                "Separate contact URL paragraph"
            );
        }
        for (n, ids) in [
            (58, vec![1565]),
            (61, vec![941]),
            (199, vec![712, 885]),
            (540, vec![964]),
        ] {
            if number == n {
                for id in &ids {
                    assert!(images.contains(id), "Inline image regression on {n}: {id}");
                }
                assert!(html.matches("<img ").count() >= ids.len());
                let clean = |s: &str| {
                    s.chars()
                        .filter(|c| (!c.is_control() || c.is_whitespace()) && *c != '\u{ad}')
                        .collect::<String>()
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" ")
                };
                assert_eq!(
                    clean(&source),
                    clean(&crate::annotations::plain_text(&html)),
                    "Inline page text {number}"
                );
            }
        }
        std::fs::write(output.join(format!("page-{number:03}.html")), &html)?;
        let visible = norm(&crate::annotations::plain_text(&html));
        let paragraphs: Vec<_> = html
            .split("</p>")
            .filter_map(|part| {
                part.rfind("<p")
                    .map(|start| norm(&crate::annotations::plain_text(&part[start..])))
            })
            .collect();
        let recovered = recover_fragmented_lines(&glyphs(&page.text()?)).0;
        let lines: Vec<_> = recovered.split(|g| g.ch == '\n').collect();
        let mut labels = Vec::new();
        let mut absent = Vec::new();
        let mut i = 0;
        while i < lines.len() {
            if let Some((count, title)) = italic_label(&lines, i) {
                if visible.contains(&title) {
                    if !paragraphs.iter().any(|p| p == &title)
                        && reading_heading(&lines, i, &headings).is_none()
                    {
                        warnings
                            .push(serde_json::json!({"page":number,"kind":"label","text":title}));
                    }
                    labels.push(title);
                } else {
                    absent.push(title);
                }
                i += count;
            } else {
                i += 1;
            }
        }
        let boundaries = paragraph_breaks(&lines, &Default::default());
        let mut starts = Vec::new();
        for (i, boundary) in boundaries.iter().enumerate() {
            if !boundary {
                continue;
            }
            let text = norm(&line_text(lines[i]));
            if !text.is_empty() && visible.contains(&text) {
                if !paragraphs.iter().any(|p| p.starts_with(&text))
                    && reading_heading(&lines, i, &headings).is_none()
                {
                    warnings
                        .push(serde_json::json!({"page":number,"kind":"paragraph","text":text}));
                }
                starts.push(text);
            }
        }
        records.push(serde_json::json!({"page":number,"labels":labels,"labels_in_visual_regions":absent,"paragraph_starts":starts,"inline_or_recreated_figures":images}));
        use std::io::Write;
        writeln!(
            progress,
            "{}",
            serde_json::json!({"record":records.last(),"warnings":warnings.iter().filter(|w| w["page"] == number).collect::<Vec<_>>()})
        )?;
        if number % 25 == 0 {
            eprintln!("Audited {number} pages");
        }
    }
    std::fs::write(
        output.join("report.json"),
        serde_json::to_vec_pretty(&serde_json::json!({"pages":records,"warnings":warnings}))?,
    )?;
    eprintln!(
        "Audited {} pages, {} candidates to review",
        records.len(),
        warnings.len()
    );
    Ok(())
}

/// Discover captions and callout text directly in PDF font/line data,
/// independently of image expansion and background classification.
#[test]
#[ignore]
fn audit_callouts_and_captions() -> anyhow::Result<()> {
    let path = std::path::PathBuf::from(std::env::var("EASYREAD_QA_PDF")?);
    let pdfium = Pdfium::new(Pdfium::bind_to_library("assets/pdfium/libpdfium.dylib")?);
    let doc = pdfium.load_pdf_from_file(&path, None)?;
    let headings = crate::pdf::document_outline(&doc);
    let paper: serde_json::Value =
        serde_json::from_slice(&std::fs::read(path.parent().unwrap().join("paper.json"))?)?;
    let out = std::path::Path::new("target/pdf-callout-qa");
    std::fs::create_dir_all(out)?;
    let compact = |s: &str| {
        s.chars()
            .filter(|c| !c.is_whitespace() && !c.is_control())
            .collect::<String>()
    };
    let mut rows = vec![];
    let mut unresolved = vec![];
    let mut captions = 0;
    let mut panels = 0;
    for (index, page) in doc.pages().iter().enumerate() {
        let number = index + 1;
        let text = page.text()?;
        let (gs, _) = recover_fragmented_lines(&glyphs(&text));
        let lines: Vec<_> = gs.split(|g| g.ch == '\n').collect();
        let mut expected_captions = vec![];
        let mut notes = vec![];
        for (i, line) in lines.iter().enumerate() {
            let value = line_text(line);
            let value = value.trim();
            let numbered = ["Figure ", "Table "].iter().any(|prefix| {
                value.strip_prefix(prefix).is_some_and(|tail| {
                    let label = tail.split_whitespace().next().unwrap_or("");
                    label.ends_with('.') && label.chars().next().is_some_and(|c| c.is_ascii_digit())
                })
            });
            if numbered
                && line
                    .iter()
                    .find(|g| !g.ch.is_whitespace())
                    .is_some_and(|g| g.italic && !g.mono)
            {
                let mut caption = value.to_owned();
                let first = line.iter().find(|g| !g.ch.is_whitespace()).unwrap();
                let mut previous_y = first.y;
                for next in lines.iter().skip(i + 1) {
                    let Some(g) = next.iter().find(|g| !g.ch.is_whitespace()) else {
                        break;
                    };
                    let next_text = line_text(next);
                    if !g.italic
                        || g.mono
                        || (g.size - first.size).abs() > first.size * 0.05
                        || previous_y - g.y < first.size * 0.5
                        || previous_y - g.y > first.size * 1.8
                        || next_text.starts_with("Figure ")
                        || next_text.starts_with("Table ")
                    {
                        break;
                    }
                    caption.push(' ');
                    caption.push_str(next_text.trim());
                    previous_y = g.y;
                }
                expected_captions.push(caption);
            }
            if ["NOTE", "TIP", "WARNING", "CAUTION", "IMPORTANT"].contains(&value) {
                notes.push(value.to_owned());
                for following in lines.iter().skip(i + 1).take(4) {
                    let value = line_text(following);
                    if !value.trim().is_empty() {
                        notes.push(value.trim().to_owned());
                    }
                }
            }
        }
        // The reported sample continues without repeating its NOTE label.
        if number == 260 || number == 270 {
            notes.extend(
                lines
                    .iter()
                    .take(4)
                    .map(|l| line_text(l))
                    .filter(|s| !s.trim().is_empty()),
            );
        }
        if expected_captions.is_empty() && notes.is_empty() {
            continue;
        }
        let (source, html, indices, figures, _) = reading_html(&page, &doc, &headings, number)?;
        let stored = paper["blocks"]
            .as_array()
            .unwrap()
            .iter()
            .find(|b| b["page"] == number && b["source_kind"] == "pdf-text")
            .unwrap()["en"]
            .as_str()
            .unwrap();
        assert_eq!(source, stored, "Unchanged source page {number}");
        let html =
            html.unwrap_or_else(|| crate::annotations::html(&crate::model::pdf_prose(&source)));
        let visible = compact(&crate::annotations::plain_text(&html));
        for (kind, values) in [("caption", &expected_captions), ("callout", &notes)] {
            for value in values {
                if !visible.contains(&compact(value)) {
                    unresolved.push(serde_json::json!({"page":number,"kind":kind,"text":value}));
                }
            }
        }
        captions += expected_captions.len();
        panels += usize::from(!notes.is_empty());
        std::fs::write(out.join(format!("page-{number}.html")), &html)?;
        rows.push(serde_json::json!({"page":number,"captions":expected_captions,"callout_lines":notes,"suppressed":indices,"figures":figures.iter().map(|f|&f.id).collect::<Vec<_>>() }));
        if rows.len() % 25 == 0 {
            eprintln!("Caption/callout audit: {} candidate pages", rows.len());
        }
    }
    let report = serde_json::json!({"scanned":doc.pages().len(),"candidate_pages":rows.len(),"captions":captions,"callout_pages":panels,"pages":rows,"unresolved":unresolved});
    std::fs::write(out.join("review.json"), serde_json::to_vec_pretty(&report)?)?;
    assert!(
        unresolved.is_empty(),
        "Unresolved captions/callouts: {unresolved:?}"
    );
    eprintln!("Verified {captions} captions and {panels} callout pages.");
    Ok(())
}

#[test]
#[ignore]
fn audit_unordered_lists() -> anyhow::Result<()> {
    let path = std::path::PathBuf::from(std::env::var("EASYREAD_QA_PDF")?);
    let pdfium = Pdfium::new(Pdfium::bind_to_library("assets/pdfium/libpdfium.dylib")?);
    let doc = pdfium.load_pdf_from_file(&path, None)?;
    let output = std::path::Path::new("target/pdf-list-qa");
    std::fs::create_dir_all(output)?;
    let mut records = vec![];
    for (number, page) in doc.pages().iter().enumerate() {
        let original = glyphs(&page.text()?);
        let recovered = recover_fragmented_lines(&original).0;
        let lines: Vec<_> = recovered.split(|g| g.ch == '\n').collect();
        let mut candidates = vec![];
        for (index, object) in page.objects().iter().enumerate() {
            if object.as_path_object().is_none() {
                continue;
            }
            let b = object.bounds()?;
            let (w, h) = (b.width().value, b.height().value);
            if !(1.5..=10.).contains(&w) || !(1.5..=10.).contains(&h) || w / h < 0.6 || w / h > 1.6
            {
                continue;
            }
            for (line, text) in lines.iter().enumerate() {
                let Some(first) = text
                    .iter()
                    .find(|g| !g.ch.is_whitespace() && !g.ch.is_control())
                else {
                    continue;
                };
                let gap = first.x - b.right().value;
                let rise = (b.top().value + b.bottom().value) / 2. - first.y;
                if (0.0..=24.).contains(&gap) && (-1.0..=10.).contains(&rise) {
                    candidates.push((index, line));
                }
            }
        }
        let mut omitted = std::collections::HashSet::new();
        for table in table::recover(&page, &recovered) {
            omitted.extend(table.lines);
        }
        // Use the same figure boundary to classify diagram dots explicitly.
        let (config, figures) = crate::pdf::reading_figures(&page, &doc, number + 1)?;
        for figure in &figures {
            for (i, line) in lines.iter().enumerate() {
                let positions: Vec<_> = line
                    .iter()
                    .filter(|g| !g.ch.is_whitespace() && !g.ch.is_control())
                    .filter_map(|g| {
                        page.points_to_pixels(
                            PdfPoints::new(g.x),
                            PdfPoints::new(g.y + g.size * 0.3),
                            &config,
                        )
                        .ok()
                    })
                    .collect();
                if !positions.is_empty()
                    && positions.iter().all(|&(x, y)| {
                        x >= figure.bounds[0] as i32
                            && x <= figure.bounds[2] as i32
                            && y >= figure.bounds[1] as i32
                            && y <= figure.bounds[3] as i32
                    })
                {
                    omitted.insert(i);
                }
            }
        }
        let lists = list::recover(&page, &recovered, &omitted);
        let restored: std::collections::HashSet<_> = lists
            .iter()
            .flat_map(|l| l.objects.iter().copied())
            .collect();
        let claimed: std::collections::HashSet<_> =
            lists.iter().flat_map(|l| l.lines.iter().copied()).collect();
        let paths: Vec<_> = candidates.iter().map(|&(object, line)| serde_json::json!({"object":object,"line":line,"text":line_text(lines[line]),"status":if restored.contains(&object) {"restored"} else if omitted.contains(&line) {"diagram_or_table"} else {"review"}})).collect();
        let text_markers: Vec<_> = lines.iter().enumerate().filter_map(|(i,l)| {
            let text = line_text(l);
            let trimmed = text.trim_start();
            let c = trimmed.chars().next()?;
            if !"•‣◦▪▫●○■□".contains(c) && !trimmed.starts_with("¡\t") { return None; }
            Some(serde_json::json!({"line":i,"text":text,"status":if claimed.contains(&i) {"restored"} else if omitted.contains(&i) {"diagram_or_table"} else {"review"}}))
        }).collect();
        if !paths.is_empty() || !text_markers.is_empty() || !lists.is_empty() {
            let (raw, html, _, _, _) = reading_html(&page, &doc, &[], number + 1)?;
            assert_eq!(
                raw,
                original
                    .iter()
                    .map(|g| g.ch)
                    .collect::<String>()
                    .replace("\r\n", "\n")
            );
            std::fs::write(
                output.join(format!("page-{}.html", number + 1)),
                html.unwrap_or_default(),
            )?;
            records.push(serde_json::json!({"page":number+1,"paths":paths,"text_markers":text_markers,"lists":lists.len(),"items":lists.iter().map(|l| l.html.matches("<li>").count()).sum::<usize>()}));
        }
        if (number + 1) % 25 == 0 {
            eprintln!("List audit: {} pages", number + 1);
        }
    }
    std::fs::write(
        output.join("discovery.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"scanned":doc.pages().len(),"pages":records}),
        )?,
    )?;
    Ok(())
}
