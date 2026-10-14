//! Original synthetic math PDFs; no external paper or TeX tool is required.
use std::{fs, path::Path};

pub fn fixture(path: &Path, mode: &str) {
    let y = if mode == "adjacent" { 710 } else { 600 };
    let x = if mode == "inline" { 72 } else { 170 };
    let font = if mode == "code" { "F4" } else { "F1" };
    let mut content = String::new();
    if mode != "only" {
        content.push_str("BT /F1 12 Tf 72 740 Td (Body before the formula.) Tj ET\n");
    }
    if mode == "reference" {
        content.push_str("BT /F1 12 Tf 100 600 Td (As shown in this paragraph x = y (1).) Tj ET\n");
    } else if mode == "inline-scripts" {
        content.push_str(
            "BT /F1 12 Tf 72 600 Td (Inline ) Tj ET\n\
BT /F2 12 Tf 115 600 Td (Q) Tj ET\n\
BT /F2 8 Tf 125 607 Td (T) Tj ET\n\
BT /F1 12 Tf 136 600 Td ( and ordinary d and k, then ) Tj ET\n\
BT /F2 12 Tf 280 600 Td (d) Tj ET\n\
BT /F2 8 Tf 287 598 Td (k) Tj ET\n\
BT /F1 12 Tf 298 600 Td ( remain inline.) Tj ET\n",
        );
    } else {
        content.push_str(&format!(
            "BT /{font} 12 Tf {x} {y} Td (f(x) = ) Tj ET\n\
BT /F2 12 Tf {} {} Td (QK) Tj ET\n\
BT /F2 8 Tf {} {} Td (T) Tj ET\n\
0 0 0 RG 0.6 w {} {} m {} {} l S\n\
BT /F3 12 Tf {} {} Td (\\326) Tj ET\n\
0.6 w {} {} m {} {} l S\n\
BT /F2 12 Tf {} {} Td (d) Tj ET\n\
BT /F2 8 Tf {} {} Td (k) Tj ET\n\
BT /F1 12 Tf {} {y} Td (V) Tj ET\n",
            x + 65,
            y + 8,
            x + 82,
            y + 14,
            x + 62,
            y + 3,
            x + 95,
            y + 3,
            x + 65,
            y - 11,
            x + 74,
            y - 1,
            x + 95,
            y - 1,
            x + 78,
            y - 9,
            x + 85,
            y - 13,
            x + 105,
        ));
        if mode != "unnumbered" {
            content.push_str(&format!("BT /F1 12 Tf 500 {y} Td ((1)) Tj ET\n"));
        }
        if mode == "inline" {
            content.push_str("BT /F1 12 Tf 390 600 Td (within ordinary prose.) Tj ET\n");
        }
        if mode == "adjacent" {
            // Close body baselines must not be swept into the recovered math.
            content.push_str("BT /F1 12 Tf 72 700 Td (Nearby body stays text.) Tj ET\n");
        }
    }
    if mode != "only" {
        content.push_str("BT /F1 12 Tf 72 540 Td (Body after the formula.) Tj ET\n");
    }
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".into(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R /F2 5 0 R /F3 6 0 R /F4 7 0 R >> >> /Contents 8 0 R >>".into(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Times-Roman >>".into(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Times-Italic >>".into(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Symbol >>".into(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Courier >>".into(),
        format!("<< /Length {} >>\nstream\n{content}endstream", content.len()),
    ];
    write_pdf(path, &objects);
}

/// Split CM roman/italic objects, epsilon's alternate Unicode form, and a
/// section title immediately followed by its smaller subsection title.
pub fn numbers_fixture(path: &Path) {
    let content = "BT /F4 10 Tf 72 710 Td (Label Smoothing ) Tj /F1 10 Tf (During training we smooth labels of value ) Tj ET\n\
BT /F2 10 Tf 340 710 Td (e) Tj /F2 7 Tf 5 -1.5 Td (ls) Tj ET\n\
BT /F3 10 Tf 354 710 Td (= 0) Tj /F2 10 Tf (.) Tj /F3 10 Tf (1) Tj /F1 10 Tf ( [36]. This) Tj ET\n\
BT /F1 10 Tf 72 697 Td (hurts perplexity but improves accuracy and BLEU score.) Tj ET\n\
BT /F4 12 Tf 72 669 Td (6 Results) Tj ET\n\
BT /F4 10 Tf 72 643 Td (6.1 Machine Translation) Tj ET\n\
BT /F1 10 Tf 72 620 Td (We obtain a score of ) Tj /F3 10 Tf (28) Tj /F2 10 Tf (.) Tj /F3 10 Tf (4) Tj /F1 10 Tf ( BLEU in ) Tj /F3 10 Tf (3) Tj /F2 10 Tf (.) Tj /F3 10 Tf (5) Tj /F1 10 Tf ( days, using ) Tj /F3 10 Tf (1) Tj /F2 10 Tf (/) Tj /F3 10 Tf (4) Tj /F1 10 Tf ( the cost.) Tj ET\n\
BT /F1 10 Tf 72 603 Td (The training cost is ) Tj /F3 10 Tf (1) Tj /F2 10 Tf (.) Tj /F3 10 Tf (0) Tj /F2 10 Tf (m) Tj /F3 10 Tf (10) Tj /F3 7 Tf 4 Ts (20) Tj 0 Ts /F1 10 Tf ( operations.) Tj ET\n\
BT /F1 10 Tf 72 580 Td (Ordinary prose numbers 0.1 and 28.4 remain ordinary text.) Tj ET\n";
    let cmap = "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n\
/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n\
/CMapName /Numbers def\n/CMapType 2 def\n1 begincodespacerange\n<00> <FF>\nendcodespacerange\n\
2 beginbfchar\n<65> <03F5>\n<6D> <00B7>\nendbfchar\nendcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n";
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".into(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R /F2 5 0 R /F3 6 0 R /F4 7 0 R >> >> /Contents 8 0 R >>".into(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Times-Roman >>".into(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /CMMI10 /ToUnicode 9 0 R >>".into(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /CMR10 >>".into(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Times-Bold >>".into(),
        format!("<< /Length {} >>\nstream\n{content}endstream", content.len()),
        format!("<< /Length {} >>\nstream\n{cmap}endstream", cmap.len()),
    ];
    write_pdf(path, &objects);
}

fn write_pdf(path: &Path, objects: &[String]) {
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
