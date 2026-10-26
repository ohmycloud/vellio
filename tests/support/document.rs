use super::*;

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
