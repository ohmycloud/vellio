//! Publisher-style overlapping callout backgrounds and small-cap labels.
use easyread_gpui::{annotations, pdf, store::Library};
use serde_json::json;

fn publisher_prose_pdf(path: &std::path::Path) {
    let content = r#"0.97 g
72 630 468 35 re f
72 606 468 40 re f
72 587 468 32 re f
72 540 468 60 re f
72 505 468 47 re f
BT 0 g /F2 12 Tf 84 644 Td (Example: Ari, engineer) Tj ET
BT 0 g /F1 12 Tf 84 619 Td (The interviewer asks why you are looking for a new job.) Tj ET
BT 0 g /F2 12 Tf 84 600 Td (Ari's answer) Tj ET
BT 0 g /F1 12 Tf 84 576 Td (I believe that this change aligns with my long-term career goals.) Tj
0 -14 Td (I want to keep learning and contribute to an innovative team.) Tj ET
BT 0 g /F1 12 Tf 84 532 Td (Also, I am looking for opportunities to build useful new products.) Tj
0 -14 Td (Your company is known for its commitment to new technology.) Tj ET
BT 0 g /F2 12 Tf 72 470 Td (T) Tj /F2 9.6 Tf (ranslation) Tj ET
BT 0 g /F1 12 Tf 72 452 Td (Even if you qualify, is this the job that you really want?) Tj ET
BT 0 g /F2 12 Tf 72 420 Td (H) Tj /F2 9.6 Tf (ow to answer) Tj ET
BT 0 g /F1 12 Tf 72 402 Td (Identify a few activities that you enjoy and describe your work.) Tj ET
0 G 1 w 72 100 468 180 re S
72 160 m 540 160 l S
BT 0 g /F1 12 Tf 90 130 Td (Chart A) Tj ET
BT 0 g /F1 12 Tf 90 200 Td (Chart B) Tj ET
BT 0 g /F1 12 Tf 72 60 Td (Body after the genuine chart stays selectable.) Tj ET
"#;
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_owned(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R /F2 5 0 R >> >> /Contents 6 0 R >>".to_owned(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_owned(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold >>".to_owned(),
        format!("<< /Length {} >>\nstream\n{content}endstream", content.len()),
    ];
    let mut data = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (i, object) in objects.iter().enumerate() {
        offsets.push(data.len());
        data.extend_from_slice(format!("{} 0 obj\n{object}\nendobj\n", i + 1).as_bytes());
    }
    let xref = data.len();
    data.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
    );
    for offset in offsets {
        data.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    data.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    std::fs::write(path, data).unwrap();
}

#[test]
fn overlapping_callout_fragments_remain_selectable_and_small_caps_break_before_body() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("publisher-prose.pdf");
    publisher_prose_pdf(&source);
    let library = Library::open(temp.path().join("library")).unwrap();
    let workspace = library.import_pdf(&source, None).unwrap();
    let before: Vec<_> = ["paper", "reader", "layout"]
        .iter()
        .map(|name| std::fs::read(workspace.root.join(format!("{name}.json"))).unwrap())
        .collect();
    let raw = workspace.state().unwrap();
    let blocks = raw["paper"]["blocks"].as_array().unwrap();
    assert_eq!(
        blocks
            .iter()
            .filter(|b| b["source_kind"] == "pdf-image")
            .count(),
        1,
        "Only the genuine chart is imported as an image"
    );
    let text = blocks
        .iter()
        .find(|b| b["source_kind"] == "pdf-text")
        .unwrap()["en"]
        .as_str()
        .unwrap();
    let presentation = pdf::code_presentation(&source, 1, text).unwrap().unwrap();
    let html = &presentation.html;
    assert_eq!(presentation.figures.len(), 1);
    assert_eq!(
        html.matches("<blockquote>").count(),
        1,
        "One connected text panel"
    );
    assert_eq!(
        html.matches("<img ").count(),
        1,
        "Only the chart is rasterized"
    );
    for phrase in [
        "Example: Ari, engineer",
        "The interviewer asks",
        "Ari’s answer",
        "I believe that this change",
        "Also, I am looking",
    ] {
        assert_eq!(
            annotations::plain_text(html).matches(phrase).count(),
            1,
            "Missing or duplicated selectable text: {phrase}"
        );
    }
    for index in 1..=5 {
        assert!(
            presentation
                .inline_figures
                .contains(&format!("pdf-p001-img{index:03}")),
            "Suppress legacy crops of every backdrop fragment"
        );
    }
    assert!(html.contains("<h3>Translation</h3>"), "{html}");
    assert!(html.contains("<h3>How to answer</h3>"), "{html}");
    assert!(html.contains("<p><span>Even if you qualify"), "{html}");
    let marked = annotations::decorate_interactive(
        html,
        &[
            json!({"id":"prose-note", "quote":"I believe that this change", "kind":"highlight", "color":"blue"}),
        ],
    );
    assert!(marked.contains("prose-note"));
    assert_eq!(
        annotations::plain_text(&marked),
        annotations::plain_text(html)
    );
    for (name, bytes) in ["paper", "reader", "layout"].iter().zip(before) {
        assert_eq!(
            std::fs::read(workspace.root.join(format!("{name}.json"))).unwrap(),
            bytes
        );
    }
}
