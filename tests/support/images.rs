use super::*;

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
