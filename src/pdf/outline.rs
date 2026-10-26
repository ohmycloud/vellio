//! Local PDF destinations and bookmark hierarchy.
use super::*;

/// Resolve a generated local PDF link to a source line, without mutating the book.
pub fn link_destination(pdf: &Path, url: &str) -> Result<(usize, String)> {
    let target = url
        .strip_prefix("easyread-pdf:")
        .context("Not a local PDF link")?;
    let (page, y) = target.split_once(':').context("Invalid PDF destination")?;
    let page: usize = page.parse()?;
    ensure!(page > 0, "PDF page numbers start at 1");
    let y = if y.is_empty() {
        None
    } else {
        let y: f32 = y.parse()?;
        ensure!(y.is_finite(), "Invalid PDF coordinate");
        Some(y)
    };
    let _guard = PDF_LOCK
        .lock()
        .map_err(|_| anyhow::anyhow!("PDF worker lock poisoned"))?;
    let doc = library(None)?.load_pdf_from_file(pdf, None)?;
    let target_page = doc.pages().get(i32::try_from(page)? - 1)?;
    Ok((
        page,
        crate::pdf::recovery::destination_quote(&target_page, y)?,
    ))
}

/// Read local bookmark destinations without changing the document or reader data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutlineEntry {
    pub page: usize,
    pub title: String,
    pub depth: usize,
}

pub fn outline(pdf: &Path) -> Result<Vec<OutlineEntry>> {
    let _guard = PDF_LOCK
        .lock()
        .map_err(|_| anyhow::anyhow!("PDF worker lock poisoned"))?;
    let doc = library(None)?.load_pdf_from_file(pdf, None)?;
    Ok(document_outline(&doc))
}

pub(crate) fn document_outline(doc: &PdfDocument<'_>) -> Vec<OutlineEntry> {
    let mut entries = Vec::new();
    let mut depths: std::collections::HashMap<PdfBookmark<'_>, usize> =
        std::collections::HashMap::new();
    for bookmark in doc.bookmarks().iter().take(10_000) {
        // The iterator visits parents first. parent() only retains the direct
        // parent handle, so walking parent().parent() loses deeper ancestry.
        let depth = bookmark
            .parent()
            .and_then(|parent| depths.get(&parent).copied())
            .map_or(0usize, |depth| (depth + 1).min(32));
        depths.insert(bookmark.clone(), depth);
        let Some(title) = bookmark.title().filter(|s| !s.trim().is_empty()) else {
            continue;
        };
        let page = bookmark
            .destination()
            .and_then(|d| d.page_index().ok())
            .or_else(|| {
                bookmark.action().and_then(|a| {
                    a.as_local_destination_action()
                        .and_then(|a| a.destination().ok())
                        .and_then(|d| d.page_index().ok())
                })
            });
        if let Some(page) = page.and_then(|page| usize::try_from(page).ok())
            && page < usize::try_from(doc.pages().len()).unwrap_or(0)
        {
            entries.push(OutlineEntry {
                page: page + 1,
                title,
                depth,
            });
        }
    }
    entries
}
