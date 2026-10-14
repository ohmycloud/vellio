use std::fs;

#[allow(dead_code)]
pub fn definitions_pdf_fixture(path: &std::path::Path) {
    let mut content = String::new();
    for (font, size, x, y, text) in [
        ("F1", 12, 72, 750, "Use "),
        ("F2", 12, 94, 750, "Tools"),
        ("F1", 12, 132, 750, " "),
        ("F4", 12, 136, 750, "carefully"),
        ("F1", 12, 187, 750, " with "),
        ("F5", 12, 215, 750, "important"),
        ("F1", 12, 265, 750, " inputs."),
        ("F2", 12, 72, 720, "description"),
        ("F1", 12, 90, 698, "A description of all registered tools"),
        ("F4", 12, 90, 684, "and their parameters."),
        ("F2", 12, 72, 660, "prompt"),
        ("F1", 12, 90, 638, "Instructions for "),
        ("F3", 12, 170, 638, "Tools"),
        ("F1", 12, 90, 624, "that wrap to the hanging margin."),
        ("F1", 12, 72, 598, "Following ordinary prose."),
        ("F2", 9, 72, 570, "x = 1"),
        ("F1", 12, 90, 548, "Body after a small code block."),
        ("F1", 12, 72, 520, "Use "),
        ("F2", 12, 94, 520, "Tools"),
        ("F1", 12, 132, 520, " again."),
    ] {
        content.push_str(&format!("BT /{font} {size} Tf {x} {y} Td ({text}) Tj ET\n"));
    }
    let objects=[
        "<< /Type /Catalog /Pages 2 0 R >>".into(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R /F2 5 0 R /F3 6 0 R /F4 7 0 R /F5 8 0 R >> >> /Contents 9 0 R /Annots [10 0 R] >>".into(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Times-Roman >>".into(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Courier >>".into(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Courier-Bold >>".into(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Times-Italic >>".into(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Times-Bold >>".into(),
        format!("<< /Length {} >>\nstream\n{content}endstream",content.len()),
        "<< /Type /Annot /Subtype /Link /Rect [90 697 268 710] /A << /S /URI /URI (https://example.org/tools) >> >>".into(),
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

/// Border-only note split over two pages, narrow external caption and real art.
#[allow(dead_code)]
pub fn border_callouts_pdf_fixture(path: &std::path::Path) {
    let text = |font: &str, size: u32, x: u32, y: u32, value: &str| {
        let value = value
            .replace('\\', "\\\\")
            .replace('(', "\\(")
            .replace(')', "\\)");
        format!("BT 0 g /{font} {size} Tf {x} {y} Td ({value}) Tj ET\n")
    };
    let mut first = String::from("0.2 0.5 0.7 rg 90 600 432 120 re f\n");
    first += &text("F2", 10, 205, 585, "Figure 1. A narrow caption");
    first += "0.74 g 90 400 432 0.6 re f\n90 280 432 0.6 re f\n90 280 0.6 120 re f\n521.4 280 0.6 120 re f\n";
    first += &text("F4", 14, 285, 380, "NOTE");
    for (y, value) in [
        (
            355,
            "Quality data for tool calling can be a challenge for many prompts and tools.",
        ),
        (
            340,
            "Multiple tools and argument formats may produce equivalent valid outcomes.",
        ),
        (
            325,
            "This explanation is flowing prose that continues on the next source page.",
        ),
    ] {
        first += &text("F1", 12, 95, y, value);
    }
    // Short callout and a title-only fragment at the page bottom.
    first += "0.74 g 90 250 432 0.6 re f\n90 180 432 0.6 re f\n90 180 0.6 70 re f\n521.4 180 0.6 70 re f\n";
    first += &text("F4", 14, 292, 225, "TIP");
    first += &text("F1", 12, 95, 200, "A short reminder remains readable text.");
    first += "0.74 g 90 80 432 0.6 re f\n90 -20 432 0.6 re f\n90 -20 0.6 100 re f\n521.4 -20 0.6 100 re f\n";
    first += &text("F4", 14, 270, 55, "IMPORTANT");
    let mut second = String::from(
        "0.5 G 0.6 w 90 650 m 90 780 l S\n522 650 m 522 780 l S\n90 650 m 522 650 l S\n",
    );
    second += "1 1 0 rg 95 754 86.4 12 re f\n";
    second += &text("F3", 12, 95, 755, "get_readme()");
    second += &text(
        "F1",
        12,
        182,
        755,
        " returns the file; tools can also fail or return partial errors.",
    );
    for (y, value) in [
        (
            735,
            "Schemas can change and tools may behave inconsistently in real applications.",
        ),
        (
            715,
            "These problems are less pronounced when using reinforcement learning.",
        ),
        (
            695,
            "The next paragraph stays outside this continued note and remains selectable.",
        ),
    ] {
        second += &text("F1", 12, 95, y, value);
    }
    second += &text(
        "F1",
        12,
        72,
        615,
        "Following ordinary body outside the frame.",
    );
    // A labeled chart is not a text panel; its frame and short labels survive.
    second += "0 G 1 w 90 250 432 220 re S\n90 300 m 522 300 l S\n";
    second += &text("F1", 12, 100, 330, "Chart A");
    second += &text("F1", 12, 100, 360, "Chart B");
    second += &text("F1", 12, 100, 390, "Chart C");
    let objects: Vec<String> = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".into(),
        "<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >>".into(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 5 0 R /F2 6 0 R /F3 7 0 R /F4 8 0 R >> >> /Contents 9 0 R /Annots [11 0 R] >>".into(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 5 0 R /F2 6 0 R /F3 7 0 R /F4 8 0 R >> >> /Contents 10 0 R >>".into(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Times-Roman >>".into(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Times-Italic >>".into(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Courier >>".into(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold >>".into(),
        format!("<< /Length {} >>\nstream\n{first}endstream",first.len()),
        format!("<< /Length {} >>\nstream\n{second}endstream",second.len()),
        "<< /Type /Annot /Subtype /Link /Rect [95 353 150 366] /A << /S /URI /URI (https://example.org/quality) >> >>".into(),
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

/// Real painted circle/square markers (not PDF text), hanging wraps, nested
/// items, two distinct list groups and a URI annotation on an item.
#[allow(dead_code)]
pub fn vector_lists_pdf_fixture(path: &std::path::Path) {
    let mut content = String::new();
    for (x, y) in [(102., 723.6), (126., 687.6), (102., 663.6), (102., 573.6)] {
        let r = 2.4;
        let k = r * 0.55228475;
        content.push_str(&format!("0 g {} {y} m {} {} {} {} {x} {} c {} {} {} {} {} {y} c {} {} {} {} {x} {} c {} {} {} {} {} {y} c f\n",x+r,x+r,y+k,x+k,y+r,y+r,x-k,y+r,x-r,y+k,x-r,x-r,y-k,x-k,y-r,y-r,x+k,y-r,x+r,y-k,x+r));
    }
    content.push_str("0 g 102 611.2 4.8 4.8 re f\n");
    for (font, x, y, text) in [
        ("F1", 72, 750, "Introduction to tools:"),
        ("F1", 114, 720, "First item continues"),
        ("F2", 114, 706, "on the next line."),
        ("F1", 138, 684, "Nested item"),
        ("F1", 114, 660, "Last circle item"),
        ("F1", 72, 636, "Following body without a full stop"),
        ("F1", 114, 610, "Square item"),
        ("F1", 114, 570, "Another circle item"),
        ("F1", 72, 540, "Closing prose."),
        ("F3", 114, 510, "* code expression"),
    ] {
        content.push_str(&format!("BT 0 g /{font} 12 Tf {x} {y} Td ({text}) Tj ET\n"));
    }
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".into(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R /F2 5 0 R /F3 6 0 R >> >> /Contents 7 0 R /Annots [8 0 R] >>".into(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Times-Roman >>".into(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Times-Italic >>".into(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Courier >>".into(),
        format!("<< /Length {} >>\nstream\n{content}endstream", content.len()),
        "<< /Type /Annot /Subtype /Link /Rect [138 684 198 696] /A << /S /URI /URI (https://example.org/nested) >> >>".into(),
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

/// In-page code panel, a callout continuing below the page, and a real diagram.
#[allow(dead_code)]
pub fn text_panels_pdf_fixture(path: &std::path::Path) {
    let mut content = String::from(
        "0.97 g 72 540 468 200 re f\n0.86 g 72 739.4 468 0.6 re f\n72 540 0.6 200 re f\n539.4 540 0.6 200 re f\n72 540 468 0.6 re f\n0.97 g 72 -1800 468 2320 re f\n0.86 g 72 519.4 468 0.6 re f\n72 -1800 0.6 2320 re f\n539.4 -1800 0.6 2320 re f\n",
    );
    for (font, size, x, y, text) in [
        ("F2", 14, 230, 713, "WHAT WE BUILT"),
        ("F3", 9, 84, 685, "TinyAgent/"),
        (
            "F3",
            9,
            84,
            670,
            "|-- agent.py - Updated integration of Memory into your TinyAgent",
        ),
        (
            "F3",
            9,
            84,
            655,
            "|-- llm.py - Added EmbeddingModel to create document embeddings",
        ),
        (
            "F3",
            9,
            84,
            640,
            "|-- memory.py - Added both short-term and long-term agent memory",
        ),
        ("F3", 9, 84, 625, "|-- trajectory.py"),
        ("F2", 14, 230, 493, "MEMORYBANK"),
        (
            "F1",
            12,
            84,
            465,
            "An interesting take on RAG is a memory mechanism for chatbots.",
        ),
        (
            "F1",
            12,
            84,
            449,
            "Recall relevant memories through a separate external database.",
        ),
        (
            "F1",
            12,
            84,
            433,
            "The memory is continuously updated to preserve useful context.",
        ),
        ("F1", 12, 100, 180, "Diagram A"),
        ("F1", 12, 370, 180, "Diagram B"),
    ] {
        content.push_str(&format!(
            "BT 0 g /{font} {size} Tf {x} {y} Td ({text}) Tj ET\n"
        ));
    }
    // Colored paths inside the callout remain a genuine illustration.
    content.push_str("0.2 0.5 0.7 rg 90 100 150 70 re f\n0.7 0.4 0.2 rg 340 100 150 70 re f\n0 G 2 w 240 135 m 340 135 l S\n");
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".into(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R /F2 5 0 R /F3 6 0 R >> >> /Contents 7 0 R >>".into(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Times-Roman >>".into(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold >>".into(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Courier >>".into(),
        format!("<< /Length {} >>\nstream\n{content}endstream", content.len()),
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

/// Nested italic labels without bookmarks, with a wrapped lettered label.
#[allow(dead_code)]
pub fn italic_labels_pdf_fixture(path: &std::path::Path) {
    let content = "0.95 g 72 -100 468 1000 re f 0 g\n\
BT /F1 12 Tf 72 740 Td (The behaviors we expect are the following:) Tj ET\n\
BT /F2 12 Tf 72 719 Td (1. Data retrieval) Tj ET\n\
BT /F1 12 Tf 90 698 Td (The agent retrieves the data.) Tj 0 -15 Td (This body wraps normally:) Tj ET\n\
BT /F2 12 Tf 90 656 Td (A\\) You hand the agent the file) Tj 0 -15 Td (\\(for example, a spreadsheet\\)) Tj ET\n\
BT /F1 12 Tf 108 620 Td (A simple tool is a Python sandbox environment.) Tj ET\n\
BT /F1 12 Tf 72 580 Td (2. Ordinary numbered body remains prose.) Tj ET\n\
BT /F2 12 Tf 72 550 Td (The data) Tj ET\n\
BT /F1 12 Tf 90 529 Td (First paragraph begins) Tj 0 -15 Td (and wraps without punctuation) Tj ET\n\
BT /F1 12 Tf 90 493 Td (Second paragraph begins) Tj 0 -15 Td (and also wraps) Tj ET\n\
BT /F2 12 Tf 90 451 Td (Tool-use data) Tj ET\n\
BT /F1 12 Tf 108 430 Td (A tool explanation begins) Tj 0 -15 Td (and continues on this line) Tj ET\n\
BT /F2 12 Tf 90 388 Td (Software engineering) Tj 0 -15 Td (tasks) Tj ET\n\
BT /F1 12 Tf 108 352 Td (A repository explanation follows) Tj ET\n\
BT /F2 12 Tf 90 331 Td (support@example.org) Tj 0 -21 Td (https://example.org/contact) Tj ET\n\
BT /F1 12 Tf 72 283 Td (A normal paragraph with several lines) Tj 0 -15 Td (continues at the usual spacing) Tj 0 -15 Td (and keeps its final physical wrap) Tj ET";
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".into(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R /F2 5 0 R >> >> /Contents 6 0 R /Annots [7 0 R] >>".into(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Times-Roman >>".into(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Times-Italic >>".into(),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
        "<< /Type /Annot /Subtype /Link /Rect [85 716 156 732] /A << /S /URI /URI (https://example.org/data) >> >>".into(),
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

/// Reference entries with full-size or smaller raised labels and physical wraps.
#[allow(dead_code)]
pub fn references_pdf_fixture(path: &std::path::Path, label_size: u8) {
    let content = format!(
        "BT /F1 {label_size} Tf 72 744 Td (20) Tj ET\n\
BT /F1 10 Tf 92 740 Td (Zhang, Susan et al. 2022. OPT: Open Pre-trained Transformer) Tj ET\n\
BT /F1 10 Tf 92 726 Td (Language Models, arXiv, 2205.01068.) Tj ET\n\
BT /F1 {label_size} Tf 72 704 Td (21) Tj ET\n\
BT /F1 10 Tf 92 700 Td (Bai, Jinze et al. 2023. Qwen-VL.) Tj ET\n\
BT /F1 {label_size} Tf 72 674 Td (22) Tj ET\n\
BT /F1 10 Tf 92 670 Td (Alayrac, Jean-Baptiste et al. 2022. Flamingo: A Visual Language Model) Tj ET\n\
BT /F1 10 Tf 92 656 Td (for Few-Shot Learning, Advances in Neural Information Processing Systems.) Tj ET"
    );
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".into(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R /Annots [6 0 R] >>".into(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into(),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
        "<< /Type /Annot /Subtype /Link /Rect [72 703 84 714] /A << /S /URI /URI (https://example.org/qwen) >> >>".into(),
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

/// Wrapped links with either one broad rectangle or precise per-line quads.
#[allow(dead_code)]
pub fn wrapped_link_fixture(path: &std::path::Path, quads: bool) {
    let ink = if quads { "0 0 0" } else { "0.55686 0 0.07059" };
    let content = format!(
        "BT /F1 10 Tf 72 740 Td (The full output is in the associated) Tj ET\n\
BT /F1 10 Tf {ink} rg 450 740 Td (GitHub) Tj ET\n\
BT /F1 10 Tf {ink} rg 72 725 Td (repository) Tj 0 0 0 rg (\\). The pass rate gives a score of 1.) Tj ET\n\
BT /F1 10 Tf 0 0 0 rg 72 690 Td (Key from the ) Tj (website) Tj (. We then create the judge.) Tj ET"
    );
    let points = if quads {
        "/QuadPoints [450 750 487.4 750 450 738 487.4 738 72 735 115.8 735 72 723 115.8 723]"
    } else {
        ""
    };
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".into(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R /Annots [6 0 R 7 0 R] >>".into(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into(),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
        format!("<< /Type /Annot /Subtype /Link /Rect [72 723 487.4 750] {points} /A << /S /URI /URI (https://example.org/repo) >> >>"),
        "<< /Type /Annot /Subtype /Link /Rect [130.36 688 165.37 700] /A << /S /URI /URI (https://example.org/site) >> >>".into(),
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

/// Prose-only PDF with nested, unnumbered and wrapped bookmarked headings.
#[allow(dead_code)]
pub fn heading_pdf_fixture(path: &std::path::Path) {
    let content = "BT /F1 10 Tf 72 740 Td (Previous paragraph.) Tj ET\n\
BT /F2 16 Tf 72 710 Td (Building Code Agents) Tj ET\n\
BT /F1 10 Tf 72 685 Td (The first thing that comes to readers' minds is code agents.) Tj\n\
0 -14 Td (We discuss Building Code Agents in this paragraph.) Tj ET\n\
BT /F2 14 Tf 72 640 Td (Code Agents to Serve) Tj 0 -18 Td (Non-Coders) Tj ET\n\
BT /F1 10 Tf 72 595 Td (Code agents open the door for non-coders.) Tj ET";
    let other_page = "BT /F2 16 Tf 72 710 Td (Building Code Agents) Tj ET\n\
BT /F1 10 Tf 72 685 Td (This page has no matching bookmark destination.) Tj ET";
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R /Outlines 9 0 R >>".into(),
        "<< /Type /Pages /Kids [3 0 R 7 0 R] /Count 2 >>".into(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R /F2 5 0 R >> >> /Contents 6 0 R >>".into(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold >>".into(),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R /F2 5 0 R >> >> /Contents 8 0 R >>".into(),
        format!("<< /Length {} >>\nstream\n{other_page}\nendstream", other_page.len()),
        "<< /Type /Outlines /First 10 0 R /Last 10 0 R /Count 3 >>".into(),
        "<< /Title (Code Agents and Code LLMs) /Parent 9 0 R /First 11 0 R /Last 11 0 R /Count 2 /Dest [3 0 R /Fit] >>".into(),
        "<< /Title (Building Code Agents) /Parent 10 0 R /First 12 0 R /Last 12 0 R /Count 1 /Dest [3 0 R /Fit] >>".into(),
        "<< /Title (Code Agents to Serve Non-Coders) /Parent 11 0 R /A << /S /GoTo /D [3 0 R /Fit] >> >>".into(),
    ];
    let mut data = b"%PDF-1.4\n".to_vec();
    let mut offsets = vec![0];
    for (i, body) in objects.iter().enumerate() {
        offsets.push(data.len());
        data.extend_from_slice(format!("{} 0 obj\n{body}\nendobj\n", i + 1).as_bytes());
    }
    let xref = data.len();
    data.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
    );
    for offset in offsets.iter().skip(1) {
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

pub fn pdf_fixture(path: &std::path::Path, rotation: u16, scanned: bool) {
    pdf_fixture_with_outline(path, rotation, scanned, false);
}
pub fn pdf_fixture_with_outline(
    path: &std::path::Path,
    rotation: u16,
    scanned: bool,
    outline: bool,
) {
    let content = if scanned {
        "0 0 0 rg 72 700 200 30 re f"
    } else {
        "BT /F1 18 Tf 72 720 Td (Rust PDF integration) Tj 0 -28 Td (Second text line) Tj ET"
    };
    let mut objects = vec![
        if outline {
            "<< /Type /Catalog /Pages 2 0 R /Outlines 6 0 R >>"
        } else {
            "<< /Type /Catalog /Pages 2 0 R >>"
        }
        .into(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),
        format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Rotate {rotation} /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>"
        ),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into(),
        format!(
            "<< /Length {} >>\nstream\n{content}\nendstream",
            content.len()
        ),
    ];
    if outline {
        objects.extend([
            "<< /Type /Outlines /First 7 0 R /Last 9 0 R /Count 3 >>".into(),
            "<< /Title (First chapter) /Parent 6 0 R /Next 9 0 R /First 8 0 R /Last 8 0 R /Count 1 /Dest [3 0 R /Fit] >>".into(),
            "<< /Title (Second chapter) /Parent 7 0 R /A << /S /GoTo /D [3 0 R /Fit] >> >>".into(),
            "<< /Title (External link) /Parent 6 0 R /Prev 7 0 R /A << /S /URI /URI (https://example.invalid) >> >>".into(),
        ]);
    }
    let mut data = b"%PDF-1.4\n".to_vec();
    let mut offsets = vec![0];
    for (i, body) in objects.iter().enumerate() {
        offsets.push(data.len());
        data.extend_from_slice(format!("{} 0 obj\n{body}\nendobj\n", i + 1).as_bytes());
    }
    let xref = data.len();
    data.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
    );
    for offset in offsets.iter().skip(1) {
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

/// Oversized nested form bounds cross the external caption, but its clip does
/// not. Invisible text also overlaps visible body text in the scaled variant.
#[allow(dead_code)]
pub fn clipped_caption_form_fixture(path: &std::path::Path, rotated: bool) {
    let matrix = if rotated {
        "0 1.5 -1.5 0 400 400"
    } else {
        "1.5 0 0 1.5 100 400"
    };
    let body_y = if rotated { 740 } else { 685 };
    let content = format!(
        "BT /F1 12 Tf 100 {body_y} Td (Visible body before the diagram.) Tj ET\n/Fm1 Do\nBT /F1 12 Tf 72 375 Td (Figure 4: A selectable caption outside the clipped diagram.) Tj 0 -14 Td (The final caption line remains text.) Tj ET\nBT /F1 12 Tf 72 320 Td (Body after the caption.) Tj ET"
    );
    let clip = "q 0 0 200 160 re W n /Fm2 Do Q";
    let drawing = "0.2 0.4 0.6 rg -40 -100 280 400 re f\nBT 0 g /F1 12 Tf 20 120 Td (Visible diagram label) Tj ET\nBT /F1 12 Tf 0 190 Td (Hidden diagram title) Tj ET";
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".into(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> /XObject << /Fm1 6 0 R >> >> /Contents 5 0 R >>".into(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into(),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
        format!("<< /Type /XObject /Subtype /Form /BBox [-100 -100 400 400] /Matrix [{matrix}] /Resources << /XObject << /Fm2 7 0 R >> >> /Length {} >>\nstream\n{clip}\nendstream", clip.len()),
        format!("<< /Type /XObject /Subtype /Form /BBox [-100 -100 400 400] /Resources << /Font << /F1 4 0 R >> >> /Length {} >>\nstream\n{drawing}\nendstream", drawing.len()),
    ];
    let mut data = b"%PDF-1.4\n".to_vec();
    let mut offsets = vec![0];
    for (i, body) in objects.iter().enumerate() {
        offsets.push(data.len());
        data.extend_from_slice(format!("{} 0 obj\n{body}\nendobj\n", i + 1).as_bytes());
    }
    let xref = data.len();
    data.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
    );
    for offset in offsets.iter().skip(1) {
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

/// Tiny raster with six distinct colors, optionally nested in a translated form.
#[allow(dead_code)] // Also included by the independent rust_smoke example.
pub fn image_pdf_fixture(path: &std::path::Path, rotation: u16, nested: bool) {
    image_fixture(path, rotation, nested, false);
}

#[allow(dead_code)]
pub fn inline_code_image_fixture(path: &std::path::Path) {
    image_fixture(path, 0, false, true);
}

fn image_fixture(path: &std::path::Path, rotation: u16, nested: bool, inline: bool) {
    let content = if inline {
        "BT /F1 10 Tf 72 700 Td (a ) Tj ET\nq 10 0 0 10 84 698 cm /Im1 Do Q\nBT /F1 10 Tf 94 700 Td (\") Tj ET\nq 10 0 0 10 84 630 cm /Im1 Do Q"
    } else if nested {
        "BT /F1 18 Tf 72 720 Td (Illustrated book text) Tj ET\nq 1 0 0 1 100 300 cm /Fm1 Do Q"
    } else {
        "BT /F1 18 Tf 72 720 Td (Illustrated book text) Tj ET\nq 120 0 0 80 100 300 cm /Im1 Do Q"
    };
    image_content_fixture(
        path,
        rotation,
        if inline { "Courier" } else { "Helvetica" },
        content,
    );
}

/// Proportional prose with a raster in the middle, at each line edge and isolated.
#[allow(dead_code)]
pub fn inline_prose_image_fixture(path: &std::path::Path) {
    image_content_fixture(
        path,
        0,
        "Helvetica",
        "\
BT /F1 12 Tf 72 700 Td (A ) Tj ET\nq 12 0 0 12 85 698 cm /Im1 Do Q\nBT /F1 12 Tf 100 700 Td (B celebrates.) Tj ET\n\
q 12 0 0 12 60 668 cm /Im1 Do Q\nBT /F1 12 Tf 76 670 Td (Start of a line.) Tj ET\n\
BT /F1 12 Tf 72 640 Td (End!) Tj ET\nq 12 0 0 12 100 638 cm /Im1 Do Q\n\
q 12 0 0 12 300 600 cm /Im1 Do Q\n",
    );
}

#[allow(dead_code)]
pub fn annotated_figure_fixture(path: &std::path::Path) {
    image_content_fixture(
        path,
        0,
        "Helvetica",
        "BT /F1 12 Tf 72 740 Td (Body before the annotated diagram.) Tj ET\n\
0.2 0.4 0.6 RG 2 w 100 450 250 200 re S\n\
1 1 1 rg 140 500 50 50 re f\n\
BT 0 0 0 rg /F1 12 Tf 120 620 Td (Label inside diagram) Tj ET\n\
BT /F1 12 Tf 72 410 Td (Body after the annotated diagram.) Tj ET\n\
q 120 0 0 80 72 250 cm /Im1 Do Q",
    );
}

#[allow(dead_code)]
pub fn prose_between_figures_fixture(path: &std::path::Path) {
    let prose = "Let's go over some examples of how these agentic RAG systems work through impactful papers";
    let fragments: String = prose
        .chars()
        .map(|c| {
            let s = c.to_string().replace('(', "\\(").replace(')', "\\)");
            format!("({s}) Tj\n")
        })
        .collect();
    let content = format!(
        "0.95 g 72 660 468 60 re f\n\
BT 0 g /F1 10 Tf 160 700 Td (Table cell text.) Tj ET\n\
BT /F1 10 Tf 72 635 Td {fragments} ET\n\
BT /F1 10 Tf 72 615 Td (and implementations in the field.) Tj ET\n\
0.7 G 0.6 w 90 490 432 100 re S\n\
BT 0 g /F1 12 Tf 280 570 Td (NOTE) Tj ET\n\
BT /F1 10 Tf 100 550 Td (Text inside the note panel.) Tj ET\n\
BT /F1 12 Tf 72 450 Td (Following body paragraph.) Tj ET"
    );
    image_content_fixture(path, 0, "Helvetica", &content);
}

/// Stretch a 3:2 bitmap into a square; optionally add content that requires
/// preserving page geometry rather than extracting the bitmap alone.
#[allow(dead_code)]
pub fn stretched_bitmap_fixture(path: &std::path::Path, mode: &str) {
    let (rotation, drawing) = match mode {
        "overlay" => (
            0,
            "q 120 0 0 120 100 300 cm /Im1 Do Q\nBT /F1 12 Tf 110 350 Td (Overlay label) Tj ET",
        ),
        "vector" => (
            0,
            "q 120 0 0 120 100 300 cm /Im1 Do Q\n0 0 0 RG 2 w 100 300 120 120 re S",
        ),
        "clip" => (
            0,
            "q 100 300 60 120 re W n 120 0 0 120 100 300 cm /Im1 Do Q",
        ),
        "reflect" => (0, "q -120 0 0 120 220 300 cm /Im1 Do Q"),
        "rotate" => (90, "q 120 0 0 120 100 300 cm /Im1 Do Q"),
        "nested" => (0, "q 1 0 0 1 100 300 cm /Fm1 Do Q"),
        "normal" => (0, "q 120 0 0 80 100 300 cm /Im1 Do Q"),
        _ => (0, "q 120 0 0 120 100 300 cm /Im1 Do Q"),
    };
    let content = format!(
        "BT /F1 12 Tf 72 720 Td (Body before figure.) Tj ET\n{drawing}\nBT /F1 12 Tf 72 260 Td (Figure 1. Bitmap caption.) Tj ET\nBT /F1 12 Tf 72 220 Td (Body after figure.) Tj ET"
    );
    image_content_fixture(path, rotation, "Helvetica", &content);
}

/// Disconnected panels, external titles and a shared roman/italic Figure label.
/// The bitmaps have deliberately different placement sizes: grouping must use
/// a page crop, not independently repair/resize their intrinsic proportions.
#[allow(dead_code)]
pub fn shared_caption_panels_fixture(path: &std::path::Path, mode: &str) {
    let left = if mode == "vector" {
        "0.2 0.4 0.6 RG 2 w 100 480 120 180 re S"
    } else {
        "q 120 0 0 180 100 480 cm /Im1 Do Q"
    };
    let right = if mode == "vector" {
        "0.6 0.4 0.2 RG 2 w 320 450 160 210 re S"
    } else {
        "q 160 0 0 210 320 450 cm /Im1 Do Q"
    };
    let captions = match mode {
        "separate" => {
            "BT /F1 10 Tf 100 425 Td (Figure 1: Left panel.) Tj ET\nBT /F1 10 Tf 320 425 Td (Figure 2: Right panel.) Tj ET"
        }
        "none" => "",
        "reference" => {
            "BT /F1 12 Tf 72 425 Td (Figure 2 shows the two panels discussed in the text.) Tj ET"
        }
        _ => {
            "BT /F1 12 Tf 72 425 Td (Figure 2: Left and right attention panels are shown side by side.) Tj 0 -14 Td (Both panels retain their source scale.) Tj ET"
        }
    };
    let content = format!(
        "BT /F1 12 Tf 72 740 Td (Body before the panels.) Tj ET\n\
BT /F1 12 Tf 75 680 Td (Scaled Dot-Product Attention) Tj ET\n{left}\n\
BT /F1 12 Tf 360 680 Td (Right attention) Tj ET\n{right}\n{captions}\n\
BT /F1 12 Tf 72 370 Td (Body after the panels.) Tj ET"
    );
    image_content_fixture(
        path,
        0,
        if mode == "italic" {
            "Helvetica-Oblique"
        } else {
            "Helvetica"
        },
        &content,
    );
}

fn image_content_fixture(path: &std::path::Path, rotation: u16, font: &str, content: &str) {
    let image = "FF000000FF000000FFFFFF0000FFFFFF00FF>";
    let form = "q 120 0 0 80 0 0 cm /Im1 Do Q";
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".into(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),
        format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Rotate {rotation} /Resources << /Font << /F1 4 0 R >> /XObject << /Im1 6 0 R /Fm1 7 0 R >> >> /Contents 5 0 R >>"
        ),
        format!("<< /Type /Font /Subtype /Type1 /BaseFont /{font} >>"),
        format!(
            "<< /Length {} >>\nstream\n{content}\nendstream",
            content.len()
        ),
        format!(
            "<< /Type /XObject /Subtype /Image /Width 3 /Height 2 /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /ASCIIHexDecode /Length {} >>\nstream\n{image}\nendstream",
            image.len()
        ),
        format!(
            "<< /Type /XObject /Subtype /Form /BBox [0 0 120 80] /Resources << /XObject << /Im1 6 0 R >> >> /Length {} >>\nstream\n{form}\nendstream",
            form.len()
        ),
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

/// Two illustrations with body text, captions and real PDF link annotations.
#[allow(dead_code)]
pub fn linked_figures_fixture(path: &std::path::Path) {
    caption_figures_fixture(path, false);
}

#[allow(dead_code)]
pub fn wrapped_captions_fixture(path: &std::path::Path) {
    caption_figures_fixture(path, true);
}

fn caption_figures_fixture(path: &std::path::Path, wrapped: bool) {
    let content = "BT /F1 12 Tf 72 740 Td (See ) Tj 0.6 0 0 rg (Figure 4-1) Tj 0 0 0 rg ( for this example.) Tj ET\n\
BT /F1 12 Tf 72 720 Td (As such, information is not persisted across calls.) Tj ET\n\
q 240 0 0 120 72 560 cm /Im1 Do Q\n\
BT /F2 10 Tf 72 540 Td (Figure 4-1. Without memory.) Tj ET\n\
BT /F1 12 Tf 72 510 Td (Over the years, agents have improved.) Tj ET\n\
BT /F1 12 Tf 72 490 Td (The next figure follows this paragraph.) Tj ET\n\
q 240 0 0 120 72 330 cm /Im1 Do Q\n\
BT /F2 10 Tf 72 310 Td (Figure 4-2. With memory.) Tj ET\n\
BT /F1 12 Tf 72 280 Td (Read the documentation.) Tj ET";
    let content = if wrapped {
        content
            .replace(
                "(Figure 4-1. Without memory.) Tj ET",
                "(Figure 4-1. Coding agents solve complex) Tj 0 -12 Td (problems) Tj ET",
            )
            .replace(
                "(Figure 4-2. With memory.) Tj ET",
                "(Table 4-2. Small & large) Tj 0 -12 Td (examples.) Tj ET",
            )
    } else {
        content.into()
    };
    let image = "FF000000FF000000FFFFFF0000FFFFFF00FF>";
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".into(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R /F2 5 0 R >> /XObject << /Im1 7 0 R >> >> /Contents 6 0 R /Annots [8 0 R 9 0 R 10 0 R] >>".into(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Oblique >>".into(),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
        format!("<< /Type /XObject /Subtype /Image /Width 3 /Height 2 /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /ASCIIHexDecode /Length {} >>\nstream\n{image}\nendstream", image.len()),
        "<< /Type /Annot /Subtype /Link /Rect [96 738 152 752] /Dest [3 0 R /XYZ 72 680 null] >>".into(),
        "<< /Type /Annot /Subtype /Link /Rect [72 488 286 502] /A << /S /GoTo /D [3 0 R /XYZ 72 450 null] >> >>".into(),
        "<< /Type /Annot /Subtype /Link /Rect [72 278 199 292] /A << /S /URI /URI (https://example.invalid/docs?a=1&b=2) >> >>".into(),
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
