use super::*;

/// Independently discover font runs and hanging terms from PDF objects,
/// then render candidate pages and compare against the persisted book.
#[test]
#[ignore]
fn audit_typography() -> anyhow::Result<()> {
    let path = std::path::PathBuf::from(std::env::var("EASYREAD_QA_PDF")?);
    let paper: serde_json::Value =
        serde_json::from_slice(&std::fs::read(path.parent().unwrap().join("paper.json"))?)?;
    let pdfium = Pdfium::new(Pdfium::bind_to_library("assets/pdfium/libpdfium.dylib")?);
    let doc = pdfium.load_pdf_from_file(&path, None)?;
    let outline = crate::pdf::document_outline(&doc);
    let out = std::path::Path::new("target/pdf-typography-qa");
    std::fs::create_dir_all(out)?;
    let mut records = vec![];
    for (index, page) in doc.pages().iter().enumerate() {
        let number = index + 1;
        let text = page.text()?;
        let mut lines: Vec<Vec<serde_json::Value>> = vec![];
        for object in page.objects().iter() {
            let Some(t) = object.as_text_object() else {
                continue;
            };
            let value = text.for_object(t);
            if value.trim().is_empty() {
                continue;
            }
            let font = t.font();
            let name = font.name().to_lowercase();
            let mono = font.is_fixed_pitch() || name.contains("mono") || name.contains("courier");
            let italic = font.is_italic() || name.contains("italic") || name.contains("oblique");
            let bold = name.contains("bold")
                || font.is_bold_reenforced()
                || matches!(
                    font.weight().ok(),
                    Some(
                        PdfFontWeight::Weight600
                            | PdfFontWeight::Weight700Bold
                            | PdfFontWeight::Weight800
                            | PdfFontWeight::Weight900
                    )
                );
            let (x, y) = t.get_translation();
            let size = t.scaled_font_size().value;
            let r = serde_json::json!({"text":value,"x":x.value,"y":y.value,"size":size,"mono":mono,"italic":italic,"bold":bold});
            if lines.last().and_then(|l| l.last()).is_none_or(|prev| {
                (prev["y"].as_f64().unwrap() - y.value as f64).abs() > size as f64 * 0.2
                    || (x.value as f64) < prev["x"].as_f64().unwrap()
            }) {
                lines.push(vec![]);
            }
            lines.last_mut().unwrap().push(r);
        }
        let mut inline = vec![];
        let mut terms = vec![];
        for (i, line) in lines.iter().enumerate() {
            let ordinary = line
                .iter()
                .any(|r| r["mono"] == false && r["bold"] == false && r["italic"] == false);
            if ordinary {
                for (j, r) in line
                    .iter()
                    .enumerate()
                    .filter(|(_, r)| r["mono"] == true || r["bold"] == true || r["italic"] == true)
                {
                    let mut r = r.clone();
                    r["prefix"] = line[..j]
                        .iter()
                        .map(|v| v["text"].as_str().unwrap())
                        .collect::<String>()
                        .into();
                    r["suffix"] = line[j + 1..]
                        .iter()
                        .map(|v| v["text"].as_str().unwrap())
                        .collect::<String>()
                        .into();
                    inline.push(r);
                }
            }
            let Some(next) = lines.get(i + 1).and_then(|l| l.first()) else {
                continue;
            };
            let first = &line[0];
            let label = line
                .iter()
                .map(|r| r["text"].as_str().unwrap())
                .collect::<String>();
            let size = first["size"].as_f64().unwrap();
            let indent = (next["x"].as_f64().unwrap() - first["x"].as_f64().unwrap()) / size;
            let gap = (first["y"].as_f64().unwrap() - next["y"].as_f64().unwrap()) / size;
            if label.chars().count() < 100
                && line.iter().all(|r| r["mono"] == true || r["bold"] == true)
                && next["mono"] == false
                && (0.5..=6.).contains(&indent)
                && (0.4..=3.5).contains(&gap)
                && (next["size"].as_f64().unwrap() - size).abs() < size * 0.15
            {
                terms.push(serde_json::json!({"text":label,"indent":indent,"x":first["x"],"y":first["y"],"size":size}));
            }
        }
        if !inline.is_empty() || !terms.is_empty() {
            let headings: Vec<_> = outline
                .iter()
                .filter(|h| h.page == number)
                .cloned()
                .collect();
            let reuse = std::env::var_os("EASYREAD_QA_REUSE_HTML").is_some();
            let old_record = std::fs::read(out.join(format!("page-{number}.json")))
                .ok()
                .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok());
            let (raw, html, _, figures, _) =
                if reuse && out.join(format!("page-{number}.html")).exists() {
                    (String::new(), None, vec![], vec![], vec![])
                } else {
                    reading_html(&page, &doc, &headings, number)?
                };
            let stored = paper["blocks"]
                .as_array()
                .unwrap()
                .iter()
                .find(|b| b["page"] == number && b["source_kind"] == "pdf-text")
                .unwrap()["en"]
                .as_str()
                .unwrap();
            if !reuse {
                assert_eq!(raw, stored, "Persisted source page {number}");
            }
            if !reuse {
                std::fs::write(
                    out.join(format!("page-{number}.html")),
                    html.unwrap_or_else(|| {
                        crate::annotations::html(&crate::model::pdf_prose(stored))
                    }),
                )?;
            }
            let config = PdfRenderConfig::new()
                .set_target_width(1200)
                .set_maximum_height(4000);
            let bounds = crate::pdf::reading_visual_bounds(&page)?;
            for r in &mut inline {
                let (x, y) = page.points_to_pixels(
                    PdfPoints::new(r["x"].as_f64().unwrap() as f32),
                    PdfPoints::new(
                        (r["y"].as_f64().unwrap() + r["size"].as_f64().unwrap() * 0.3) as f32,
                    ),
                    &config,
                )?;
                r["visual"] = bounds
                    .iter()
                    .any(|b| {
                        x >= b[0] as i32 && x <= b[2] as i32 && y >= b[1] as i32 && y <= b[3] as i32
                    })
                    .into();
            }
            let ids = if reuse {
                old_record.unwrap()["figures"].clone()
            } else {
                serde_json::json!(figures.iter().map(|f| &f.id).collect::<Vec<_>>())
            };
            let record =
                serde_json::json!({"page":number,"inline":inline,"terms":terms,"figures":ids});
            std::fs::write(
                out.join(format!("page-{number}.json")),
                serde_json::to_vec_pretty(&record)?,
            )?;
            records.push(record);
        }
        if number % 25 == 0 {
            eprintln!("Typography audit: {number} pages");
        }
    }
    std::fs::write(
        out.join("discovery.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"scanned":doc.pages().len(),"pages":records}),
        )?,
    )?;
    Ok(())
}

/// Broader discovery than the renderer: include lowercase and tightly
/// spaced italic blocks, then review them against the rendered audit.
#[test]
#[ignore]
fn audit_italic_blocks() -> anyhow::Result<()> {
    let path = std::path::PathBuf::from(std::env::var("EASYREAD_QA_PDF")?);
    let pdfium = Pdfium::new(Pdfium::bind_to_library("assets/pdfium/libpdfium.dylib")?);
    let doc = pdfium.load_pdf_from_file(&path, None)?;
    let mut records = Vec::new();
    let mut backgrounds = Vec::new();
    let mut changed_paragraph_pages = Vec::new();
    let mut continued_caption_pages = Vec::new();
    for (number, page) in doc.pages().iter().enumerate() {
        let decorations = crate::pdf::page_background_indices(&page);
        if !decorations.is_empty() {
            backgrounds.push(serde_json::json!({"page":number + 1,"objects":decorations}));
        }
        let recovered = recover_fragmented_lines(&glyphs(&page.text()?)).0;
        let lines: Vec<_> = recovered.split(|g| g.ch == '\n').collect();
        if (0..lines.len()).any(|i| leading_caption(&lines, i)) {
            continued_caption_pages.push(number + 1);
        }
        let current = paragraph_breaks(&lines, &Default::default());
        let mut roman_only = recovered.clone();
        for g in &mut roman_only {
            if g.italic {
                g.mono = true;
            }
        }
        let old_lines: Vec<_> = roman_only.split(|g| g.ch == '\n').collect();
        if current != paragraph_breaks(&old_lines, &Default::default()) {
            changed_paragraph_pages.push(number + 1);
        }
        let lines: Vec<_> = recovered.split(|g| g.ch == '\n').collect();
        let mut i = 0;
        while i < lines.len() {
            let start = i;
            let italic = |line: &[Glyph]| {
                line.iter().any(|g| g.ch.is_alphabetic())
                    && line
                        .iter()
                        .filter(|g| !g.ch.is_whitespace() && !g.ch.is_control())
                        .all(|g| g.italic && !g.mono && !g.bold)
            };
            while i < lines.len() && italic(lines[i]) {
                i += 1;
            }
            if start == i {
                i += 1;
                continue;
            }
            let title = lines[start..i]
                .iter()
                .map(|line| line_text(line))
                .collect::<Vec<_>>()
                .join(" ");
            let title = title.split_whitespace().collect::<Vec<_>>().join(" ");
            let first = lines[start].iter().find(|g| !g.ch.is_whitespace()).unwrap();
            let last = lines[i - 1].iter().find(|g| !g.ch.is_whitespace()).unwrap();
            let next = lines
                .get(i)
                .and_then(|line| line.iter().find(|g| !g.ch.is_whitespace()));
            if let Some(next) = next
                && !next.italic
                && !next.mono
                && !next.bold
                && title.chars().count() <= 100
                && title.split_whitespace().count() <= 14
                && !title.ends_with(['.', '?', '!', ';', ':'])
                && !["Figure ", "Table "]
                    .iter()
                    .any(|prefix| title.starts_with(prefix))
                && (next.x - first.x >= first.size * 0.5 || last.y - next.y >= first.size * 1.4)
            {
                records.push(serde_json::json!({"page":number + 1,"text":title,"gap_in_font_sizes":(last.y-next.y)/first.size,"indent_in_font_sizes":(next.x-first.x)/first.size}));
            }
        }
    }
    std::fs::create_dir_all("target/pdf-structure-qa")?;
    std::fs::write(
        "target/pdf-structure-qa/italic-discovery.json",
        serde_json::to_vec_pretty(&records)?,
    )?;
    std::fs::write(
        "target/pdf-structure-qa/background-discovery.json",
        serde_json::to_vec_pretty(&backgrounds)?,
    )?;
    std::fs::write(
        "target/pdf-structure-qa/changed-paragraph-pages.json",
        serde_json::to_vec_pretty(&changed_paragraph_pages)?,
    )?;
    std::fs::write(
        "target/pdf-structure-qa/continued-caption-pages.json",
        serde_json::to_vec_pretty(&continued_caption_pages)?,
    )?;
    eprintln!("Discovered {} independent italic blocks", records.len());
    Ok(())
}
