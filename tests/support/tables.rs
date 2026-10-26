use super::*;

/// Ruled three-column table, real PDF font runs, a wrapped first cell and an
/// empty data cell. Borders are emitted as individual cell segments.
#[allow(dead_code)]
pub fn table_pdf_fixture(path: &std::path::Path) {
    let mut content =
        String::from("BT /F2 12 Tf 72 770 Td (Table 9-1. Connector comparison) Tj ET\n");
    for y in [750, 720, 660, 610] {
        for (left, width) in [(72, 108), (180, 180), (360, 180)] {
            content.push_str(&format!("0.6 g {left} {y} {width} 0.6 re f\n"));
        }
    }
    for (font, text, x, y) in [
        ("F3", "Connectors", 78, 732),
        ("F3", "Advantages", 186, 732),
        ("F3", "Disadvantages", 366, 732),
        ("F1", "Projection-", 78, 700),
        ("F1", "based", 78, 686),
        ("F2", "Simple:", 186, 700),
        ("F1", " Lightweight & fast.", 219, 700),
        ("F1", "Low latency", 186, 686),
        ("F1", "and few parameters.", 186, 672),
        ("F2", "Scalability:", 366, 700),
        ("F1", " Many tokens.", 421, 700),
        ("F1", "Query-based", 78, 640),
        ("F1", "More training.", 366, 640),
        ("F1", "Following body stays outside the table.", 72, 580),
    ] {
        content.push_str(&format!("BT 0 g /{font} 10 Tf {x} {y} Td ({text}) Tj ET\n"));
    }
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".into(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R /F2 5 0 R /F3 6 0 R >> >> /Contents 7 0 R /Annots [8 0 R] >>".into(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Times-Roman >>".into(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Times-Italic >>".into(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold >>".into(),
        format!("<< /Length {} >>\nstream\n{content}endstream", content.len()),
        "<< /Type /Annot /Subtype /Link /Rect [186 685 235 696] /A << /S /URI /URI (https://example.org/latency) >> >>".into(),
    ];
    let mut data = b"%PDF-1.4\n".to_vec();
    let mut offsets = vec![];
    for (i, body) in objects.iter().enumerate() {
        offsets.push(data.len());
        data.extend_from_slice(format!("{} 0 obj\n{body}\nendobj\n", i + 1).as_bytes());
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
    fs::write(path, data).unwrap();
}

/// A five-column table with only a header divider, and per-row vertical rules.
#[allow(dead_code)]
pub fn vertical_table_pdf_fixture(path: &std::path::Path) {
    let mut content = String::from("BT /F1 10 Tf 72 775 Td (CHAPTER 1. INTRODUCTION) Tj ET\n");
    content.push_str("0 g 72 719.7 468 0.6 re f\n");
    for x in [180., 270., 340., 460.] {
        for (bottom, height) in [(720., 28.), (690., 30.), (660., 30.), (610., 50.)] {
            content.push_str(&format!("0 g {} {bottom} 0.6 {height} re f\n", x - 0.3));
        }
    }
    for (y, cells) in [
        (
            730,
            ["Course Goal", "Parts", "Case Study", "Chapters", "Optional"],
        ),
        (
            700,
            [
                "Comprehensive",
                "I, II, III, IV, V",
                "1, 2, 3, 4",
                "1-18",
                "19-22",
            ],
        ),
        (
            670,
            ["Core Assistants", "II, III, IV", "2, 3", "6-14", "2-5, 19"],
        ),
        (
            640,
            [
                "Core Social Simulation",
                "II, III, V",
                "2, 4",
                "6-10, 11, 15-18",
                "2-5, 22",
            ],
        ),
    ] {
        for (text, x) in cells.into_iter().zip([76, 186, 276, 346, 466]) {
            content.push_str(&format!("BT /F1 10 Tf {x} {y} Td ({text}) Tj ET\n"));
        }
    }
    content
        .push_str("BT /F1 10 Tf 72 590 Td (Table 1.1: Mapping course goals to chapters) Tj ET\n");
    content.push_str("BT /F1 10 Tf 72 555 Td (Following prose stays outside the table.) Tj ET\n");
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".into(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>".into(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Times-Roman >>".into(),
        format!("<< /Length {} >>\nstream\n{content}endstream", content.len()),
    ];
    let mut data = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (i, body) in objects.iter().enumerate() {
        offsets.push(data.len());
        data.extend_from_slice(format!("{} 0 obj\n{body}\nendobj\n", i + 1).as_bytes());
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
    fs::write(path, data).unwrap();
}

/// A source-fallback regression: grouped two-level headers, missing values,
/// partial header rules, and a training-cost cell spanning two subcolumns.
#[allow(dead_code)]
pub fn complex_table_pdf_fixture(path: &std::path::Path) {
    let mut content = String::new();
    for (left, y, width) in [
        (72, 748, 468),
        (220, 730, 110),
        (350, 730, 190),
        (72, 710, 468),
        (72, 670, 468),
        (72, 620, 468),
    ] {
        content.push_str(&format!("0 g {left} {y} {width} 0.7 re f\n"));
    }
    for (text, x, y) in [
        ("Table 2: Comparison of models and training costs", 72, 770),
        ("Model", 78, 722),
        ("BLEU", 250, 738),
        ("Training Cost", 395, 738),
        ("EN-DE", 225, 718),
        ("EN-FR", 290, 718),
        ("EN-DE", 365, 718),
        ("EN-FR", 465, 718),
        ("ByteNet", 78, 695),
        ("23.75", 230, 695),
        ("Recurrent", 78, 680),
        ("39.92", 290, 680),
        ("1.4 x 10", 465, 680),
        ("Transformer base", 78, 651),
        ("27.3", 230, 651),
        ("38.1", 290, 651),
        ("3.3 x 10", 411, 651),
        ("Transformer big", 78, 635),
        ("28.4", 230, 635),
        ("41.8", 290, 635),
        ("2.3 x 10", 411, 635),
        (
            "Following prose remains selectable outside the table.",
            72,
            590,
        ),
    ] {
        content.push_str(&format!("BT /F1 10 Tf {x} {y} Td ({text}) Tj ET\n"));
    }
    content.push_str("BT /F1 7 Tf 505 684 Td (20) Tj ET\nBT /F1 7 Tf 451 655 Td (18) Tj ET\nBT /F1 7 Tf 451 639 Td (19) Tj ET\n");
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".into(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>".into(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Times-Roman >>".into(),
        format!("<< /Length {} >>\nstream\n{content}endstream", content.len()),
    ];
    let mut data = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (i, body) in objects.iter().enumerate() {
        offsets.push(data.len());
        data.extend_from_slice(format!("{} 0 obj\n{body}\nendobj\n", i + 1).as_bytes());
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
    fs::write(path, data).unwrap();
}

/// A comparison table split over two pages, with an empty corner header and
/// a headerless shaded continuation whose first cell is intentionally empty.
#[allow(dead_code)]
pub fn split_comparison_pdf_fixture(path: &std::path::Path) {
    let mut first = String::from("BT /F2 12 Tf 72 265 Td (Table 4-1. Comparison) Tj ET\n");
    let mut second = String::new();
    for (content, bands) in [
        (&mut first, vec![(240, 210), (210, 150), (150, 20)]),
        (&mut second, vec![(768, 656)]),
    ] {
        for (top, bottom) in bands {
            for (left, width) in [(72, 84), (156, 215), (371, 169)] {
                if bottom == 20 || top == 768 {
                    content.push_str(&format!(
                        "0.94 0.96 0.99 rg {left} {bottom} {width} {} re f\n",
                        top - bottom
                    ));
                }
                for y in [top, bottom] {
                    content.push_str(&format!("0.6 g {left} {y} {width} 0.6 re f\n"));
                }
            }
        }
    }
    for (font, text, x, y) in [
        ("F3", "Single-agent RAG", 162, 222),
        ("F3", "Multi-agent RAG", 377, 222),
        ("F1", "Advantages", 78, 192),
        ("F2", "Cost effective", 162, 192),
        ("F1", "Fewer API calls are needed.", 162, 178),
        ("F2", "Modularity", 377, 192),
        ("F1", "Specialized agents.", 377, 178),
        ("F1", "Disadvantages", 78, 132),
        ("F2", "Single point of failure", 162, 132),
        ("F1", "Errors may compound.", 162, 118),
        ("F2", "Higher costs", 377, 132),
        ("F1", "Parallel execution", 377, 118),
    ] {
        first.push_str(&format!("BT 0 g /{font} 10 Tf {x} {y} Td ({text}) Tj ET\n"));
    }
    for (font, text, x, y) in [
        ("F2", "Lower accuracy ceiling", 162, 746),
        ("F1", "Too many sources.", 162, 732),
        ("F1", "drives costs.", 377, 746),
        ("F2", "Complexity", 377, 732),
        ("F1", "Harder to debug.", 377, 718),
        ("F1", "Following body stays outside the table.", 72, 625),
    ] {
        second.push_str(&format!("BT 0 g /{font} 10 Tf {x} {y} Td ({text}) Tj ET\n"));
    }
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".into(),
        "<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >>".into(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 5 0 R /F2 6 0 R /F3 7 0 R >> >> /Contents 8 0 R >>".into(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 5 0 R /F2 6 0 R /F3 7 0 R >> >> /Contents 9 0 R /Annots [10 0 R] >>".into(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Times-Roman >>".into(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Times-Italic >>".into(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold >>".into(),
        format!("<< /Length {} >>\nstream\n{first}endstream", first.len()),
        format!("<< /Length {} >>\nstream\n{second}endstream", second.len()),
        "<< /Type /Annot /Subtype /Link /Rect [162 730 250 741] /A << /S /URI /URI (https://example.org/sources) >> >>".into(),
    ];
    let mut data = b"%PDF-1.4\n".to_vec();
    let mut offsets = vec![];
    for (i, body) in objects.iter().enumerate() {
        offsets.push(data.len());
        data.extend_from_slice(format!("{} 0 obj\n{body}\nendobj\n", i + 1).as_bytes());
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
    fs::write(path, data).unwrap();
}
