//! PDF import, visual-asset completion and source crops.
use super::*;

/// Extract only visual assets for previously imported text-only workspaces.
/// Never replace their paper text or reader-owned annotations/edits.
pub fn media(
    pdf: &Path,
    root: &Path,
    cancel: &std::sync::atomic::AtomicBool,
    mut progress: impl FnMut(usize, usize),
) -> Result<Media> {
    let _guard = PDF_LOCK
        .lock()
        .map_err(|_| anyhow::anyhow!("PDF worker lock poisoned"))?;
    let doc = library(None)?.load_pdf_from_file(pdf, None)?;
    let mut result = Media {
        blocks: vec![],
        layout: json!({}),
    };
    for (index, page) in doc.pages().iter().enumerate() {
        ensure!(
            !cancel.load(std::sync::atomic::Ordering::Relaxed),
            "Image extraction cancelled"
        );
        let config = PdfRenderConfig::new()
            .set_target_width(1200)
            .set_maximum_height(4000)
            .render_form_data(false);
        let image = page.render_with_config(&config)?.as_image()?;
        let extracted = page_media(&page, index + 1, &image, &config, root)?;
        result.blocks.extend(extracted.blocks);
        result
            .layout
            .as_object_mut()
            .unwrap()
            .extend(extracted.layout.as_object().unwrap().clone());
        progress(index + 1, doc.pages().len() as usize);
    }
    Ok(result)
}
pub fn prepare(
    pdf: &Path,
    root: &Path,
    explicit: Option<&Path>,
    cancel: &std::sync::atomic::AtomicBool,
    mut progress: impl FnMut(usize, usize),
) -> Result<Extracted> {
    let _guard = PDF_LOCK
        .lock()
        .map_err(|_| anyhow::anyhow!("PDF worker lock poisoned"))?;
    let doc = library(explicit)?
        .load_pdf_from_file(pdf, None)
        .context("PDF cannot be opened (invalid or password protected)")?;
    ensure!(!doc.pages().is_empty(), "PDF has no pages");
    fs::create_dir_all(root.join("pages"))?;
    fs::create_dir_all(root.join("extract"))?;
    let mut result = Extracted {
        title: doc
            .metadata()
            .get(PdfDocumentMetadataTagType::Title)
            .map(|t| t.value().into())
            .unwrap_or_default(),
        author: doc
            .metadata()
            .get(PdfDocumentMetadataTagType::Author)
            .map(|t| t.value().into())
            .unwrap_or_default(),
        pages: vec![],
        blocks: vec![],
        layout: json!({}),
    };
    for (index, page) in doc.pages().iter().enumerate() {
        ensure!(
            !cancel.load(std::sync::atomic::Ordering::Relaxed),
            "PDF import cancelled"
        );
        let n = index + 1;
        let config = PdfRenderConfig::new()
            .set_target_width(1200)
            .set_maximum_height(4000)
            .render_form_data(false);
        let bitmap = page.render_with_config(&config)?;
        let image = bitmap.as_image()?;
        let (w, h) = (image.width() as f64, image.height() as f64);
        let name = format!("pages/page-{n:03}.webp");
        image.save(root.join(&name))?;
        let textpage = page.text()?;
        let text = textpage
            .chars()
            .iter()
            .filter_map(|ch| ch.unicode_char())
            .collect::<String>()
            .replace("\r\n", "\n");
        fs::write(root.join(format!("extract/page-{n:03}.txt")), &text)?;
        let mut chars = Vec::new();
        for ch in textpage.chars().iter() {
            let Some(c) = ch.unicode_char() else {
                continue;
            };
            if c.is_control() {
                continue;
            }
            if let Ok(bounds) = ch.tight_bounds() {
                let corners = [
                    (bounds.left(), bounds.top()),
                    (bounds.right(), bounds.top()),
                    (bounds.left(), bounds.bottom()),
                    (bounds.right(), bounds.bottom()),
                ];
                let pixels = corners
                    .into_iter()
                    .map(|(x, y)| page.points_to_pixels(x, y, &config))
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                let x0 = pixels.iter().map(|p| p.0).min().unwrap() as f64 / w;
                let x1 = pixels.iter().map(|p| p.0).max().unwrap() as f64 / w;
                let y0 = pixels.iter().map(|p| p.1).min().unwrap() as f64 / h;
                let y1 = pixels.iter().map(|p| p.1).max().unwrap() as f64 / h;
                chars.push(json!([
                    c.to_string(),
                    x0.clamp(0., 1.),
                    y0.clamp(0., 1.),
                    x1.clamp(0., 1.),
                    y1.clamp(0., 1.)
                ]));
            }
        }
        crate::store::write_json(
            &root.join(format!("extract/page-{n:03}.chars.json")),
            &json!(chars),
        )?;
        result
            .pages
            .push(json!({"n":n,"w":page.width().value,"h":page.height().value,"img":name}));
        // Conservative source view: do not pretend extraction identifies tables/math/reading order.
        if !text.trim().is_empty() {
            let id = format!("pdf-p{n:03}");
            result.blocks.push(
                json!({"id":id,"type":"para","page":n,"en":text,"zh":"","source_kind":"pdf-text"}),
            );
            result.layout[&id] = json!({"page":n,"src":"rust-pdfium","box":[0.,0.,1.,1.]});
            let media = page_media(&page, n, &image, &config, root)?;
            result.blocks.extend(media.blocks);
            result
                .layout
                .as_object_mut()
                .unwrap()
                .extend(media.layout.as_object().unwrap().clone());
        } else {
            // Keep image-only/scanned pages visible without inventing a caption.
            // A figure's actual caption may start on the following source page;
            // no extracted text here does not establish an OCR failure.
            let id = format!("pdf-p{n:03}-page");
            result.blocks.push(json!({"id":id,"type":"figure","page":n,"src":name,"width":image.width(),"height":image.height(),"caption_zh":"","caption_en":"","source_kind":"pdf-page"}));
            result.layout[&id] = json!({"page":n,"src":"rust-pdfium","box":[0.,0.,1.,1.]});
        }
        progress(n, doc.pages().len() as usize);
    }
    Ok(result)
}
pub fn crop(ws: &crate::store::Workspace, page: usize, box_: [f64; 4], out: &Path) -> Result<()> {
    ensure!(
        page > 0
            && box_
                .iter()
                .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
            && box_[0] < box_[2]
            && box_[1] < box_[3],
        "Invalid crop box"
    );
    let pages = ws.load("paper")?;
    let rel = pages["meta"]["pages"][page - 1]["img"]
        .as_str()
        .context("Page not found")?;
    let mut image = image::open(ws.asset(rel)?)?;
    let (w, h) = (image.width(), image.height());
    let x0 = (box_[0] * w as f64).floor() as u32;
    let y0 = (box_[1] * h as f64).floor() as u32;
    let x1 = (box_[2] * w as f64).ceil().min(w as f64) as u32;
    let y1 = (box_[3] * h as f64).ceil().min(h as f64) as u32;
    image.crop(x0, y0, x1 - x0, y1 - y0).save(out)?;
    Ok(())
}
