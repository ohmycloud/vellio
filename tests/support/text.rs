use super::*;

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
