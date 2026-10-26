//! Read-only acceptance for prose-heavy PDFs; optional local source, never modifies the library.
use super::*;

#[test]
fn small_caps_labels_are_separate_headings_but_inline_emphasis_is_not() {
    let mut gs = super::tests::test_lines(&[
        ("Translation", 9.5, true, false),
        (
            "Even if you qualify, I need to know that this is a job that you want.",
            10.,
            false,
            false,
        ),
        ("How to answer", 9.5, true, false),
        (
            "Identify a few work activities and describe your experience.",
            10.,
            false,
            false,
        ),
    ]);
    for line in gs.split_mut(|g| g.ch == '\n') {
        if line.first().is_some_and(|g| g.bold) {
            for g in line.iter_mut().filter(|g| g.ch.is_lowercase()) {
                g.size *= 0.8;
            }
        }
    }
    let html = render(&gs, &[]).unwrap();
    assert!(html.contains("<h3>Translation</h3>"), "{html}");
    assert!(html.contains("<h3>How to answer</h3>"), "{html}");
    assert!(html.contains("<p><span>Even if you qualify"), "{html}");
    let mut gs = super::tests::test_lines(&[
        (
            "Important inline emphasis continues as normal body text.",
            10.,
            false,
            false,
        ),
        (
            "The next physical line belongs to the same paragraph.",
            10.,
            false,
            false,
        ),
    ]);
    for g in gs.iter_mut().take("Important".len()) {
        g.bold = true;
    }
    let html = render(&gs, &[]).unwrap();
    assert!(!html.contains("<h3>"), "{html}");
    assert!(html.contains("<strong>Important</strong>"), "{html}");
}

#[test]
fn diagram_callout_columns_stay_inside_the_diagram_not_bold_body() {
    let mut gs = super::tests::test_lines(&[
        ("Left explanation is a label", 9., true, false),
        ("and wraps to another row.", 9., true, false),
        ("Right explanation is a label", 9., true, false),
        ("with its own second row.", 9., true, false),
        ("Following body must not be captured.", 10., false, false),
    ]);
    for (i, line) in gs.split_mut(|g| g.ch == '\n').enumerate() {
        for g in line {
            g.y = [383., 373., 383., 373., 313.][i];
            if i == 2 || i == 3 {
                g.x += 250.;
                g.right += 250.;
            }
        }
    }
    let bounds = [168., 392., 430., 432.];
    let expanded = diagram_callouts(&gs, bounds, 612.).unwrap();
    assert!(expanded[1] < 373. && expanded[1] > 360.);
    assert_eq!(expanded[0], 72.);
    assert!(expanded[2] > 430.);
    let left_only: Vec<_> = gs
        .split(|g| g.ch == '\n')
        .take(2)
        .flat_map(|line| {
            line.iter().cloned().chain(std::iter::once(Glyph {
                ch: '\n',
                ..line[0].clone()
            }))
        })
        .collect();
    assert!(
        diagram_callouts(&left_only, bounds, 612.).is_none(),
        "A lone bold heading must not expand a diagram"
    );
    for g in &mut gs {
        g.bold = false;
    }
    assert!(
        diagram_callouts(&gs, bounds, 612.).is_none(),
        "Never consume ordinary body paragraphs"
    );
}

#[test]
#[ignore = "requires EASYREAD_QA_PDF pointing to Interview Speak"]
fn interview_prose_acceptance() -> anyhow::Result<()> {
    let path = std::env::var("EASYREAD_QA_PDF")?;
    let pdfium = Pdfium::new(Pdfium::bind_to_library("assets/pdfium/libpdfium.dylib")?);
    let doc = pdfium.load_pdf_from_file(&path, None)?;
    let headings = crate::pdf::document_outline(&doc);
    let output = std::path::Path::new("target/interview-prose-qa");
    std::fs::create_dir_all(output)?;
    let paper_path = std::path::Path::new(&path)
        .parent()
        .unwrap()
        .join("paper.json");
    let paper_bytes = std::fs::read(&paper_path)?;
    let paper: serde_json::Value = serde_json::from_slice(&paper_bytes)?;
    let pages: Vec<usize> = if std::env::var_os("EASYREAD_QA_ALL_PAGES").is_some() {
        (1..=doc.pages().len() as usize).collect()
    } else {
        vec![50, 51, 52, 54, 66]
    };
    let mut records = Vec::new();
    for number in pages {
        let page = doc.pages().get(number as i32 - 1)?;
        let (source, html, indices, figures, _) = reading_html(&page, &doc, &headings, number)?;
        let stored = paper["blocks"]
            .as_array()
            .unwrap()
            .iter()
            .find(|b| b["page"] == number && b["source_kind"] == "pdf-text");
        if let Some(block) = stored {
            assert_eq!(
                source,
                block["en"].as_str().unwrap(),
                "Source/anchors on page {number}"
            );
        }
        let html =
            html.unwrap_or_else(|| crate::annotations::html(&crate::model::pdf_prose(&source)));
        if [50, 51, 52, 54].contains(&number) {
            assert!(
                figures.is_empty(),
                "Page {number}: a prose backdrop became a figure"
            );
            assert!(!html.contains("<img "));
        }
        if number == 51 {
            assert!(html.contains("<h3>Example: Ari, engineer</h3>"));
            assert!(html.contains("<h3>Ari’s answer</h3>"));
            assert!(html.contains("Also, I’m looking to work in an innovative environment."));
        }
        if number == 52 {
            assert!(html.contains("Lastly, through my research"));
        }
        if number == 54 {
            assert!(html.contains("<h3>Translation</h3>"));
            assert!(html.contains("<h3>How to answer</h3>"));
            assert!(html.contains("<p><span>Even if you qualify"));
            assert!(html.contains("<p><span>Identify a few work activities"));
        }
        if number == 66 {
            assert_eq!(figures.len(), 1, "One complete explanatory diagram");
            assert!(figures[0].height >= 170 && figures[0].height <= 260);
            assert!(
                !html.contains("“Know our industry” means"),
                "Label must stay in the diagram, not become large bold body"
            );
            assert!(html.contains("<h3>Translation</h3>"));
            assert!(html.contains("What can you say to convince me"));
            std::fs::write(output.join("page-66-diagram.png"), &figures[0].png)?;
        }
        records.push(serde_json::json!({"page":number,"figures":figures.len(),"suppressed_legacy":indices.len(),"callouts":html.matches("<blockquote>").count()}));
        // Keep a compact review copy without embedded image payloads.
        let mut review = html.clone();
        while let Some(start) = review.find("data:image/png;base64,") {
            let end = start + review[start..].find('"').unwrap();
            review.replace_range(start..end, "[image]");
        }
        std::fs::write(output.join(format!("page-{number}.html")), html)?;
        std::fs::write(output.join(format!("page-{number}-review.html")), review)?;
    }
    std::fs::write(
        output.join("report.json"),
        serde_json::to_vec_pretty(&records)?,
    )?;
    assert_eq!(
        std::fs::read(paper_path)?,
        paper_bytes,
        "Never rewrite imported text or anchors"
    );
    Ok(())
}
