//! Read-only presentation DTOs and bounded page batches.
use super::*;

/// Read-only code and heading recovery for existing native PDF page blocks.
/// Require an exact extraction match before applying font/position metadata.
pub fn code_html(pdf: &Path, page: usize, expected: &str) -> Result<Option<String>> {
    Ok(code_presentation(pdf, page, expected)?.map(|p| p.html))
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct CodePresentation {
    pub html: String,
    pub inline_figures: Vec<String>,
    pub figures: Vec<FigurePreview>,
    #[serde(default)]
    pub math: Vec<MathPresentation>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct MathPresentation {
    pub tex: String,
    pub display: bool,
    pub number: Option<String>,
    pub layout: crate::formula::FormulaLayout,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct FigurePreview {
    pub id: String,
    #[serde(with = "png_base64")]
    pub png: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

pub fn code_presentation(
    pdf: &Path,
    page: usize,
    expected: &str,
) -> Result<Option<CodePresentation>> {
    code_presentations(pdf, &[(page, expected.to_owned())])?
        .pop()
        .context("Missing PDF presentation result")?
}

/// Prepare a small batch with one document open and one outline traversal.
/// Keep the batch bounded so a newly requested page can take priority between
/// batches, and do not let a malformed page discard its siblings' results.
pub fn code_presentations(
    pdf: &Path,
    requests: &[(usize, String)],
) -> Result<Vec<Result<Option<CodePresentation>>>> {
    prepare_presentations(pdf, requests, false)
}

/// Translation must distinguish a plain page from stale/mismatched extraction.
/// Never send a flattened fallback when geometry belongs to different text.
pub fn translation_presentations(
    pdf: &Path,
    requests: &[(usize, String)],
) -> Result<Vec<Result<Option<CodePresentation>>>> {
    prepare_presentations(pdf, requests, true)
}

fn prepare_presentations(
    pdf: &Path,
    requests: &[(usize, String)],
    require_match: bool,
) -> Result<Vec<Result<Option<CodePresentation>>>> {
    if requests.is_empty() {
        return Ok(Vec::new());
    }
    let _guard = PDF_LOCK
        .lock()
        .map_err(|_| anyhow::anyhow!("PDF worker lock poisoned"))?;
    let doc = library(None)?.load_pdf_from_file(pdf, None)?;
    let outline = document_outline(&doc);
    Ok(requests
        .iter()
        .map(|(number, expected)| {
            let page = doc.pages().get(
                i32::try_from(*number)?
                    .checked_sub(1)
                    .context("PDF page numbers start at 1")?,
            )?;
            let headings: Vec<_> = outline
                .iter()
                .filter(|entry| entry.page == *number)
                .cloned()
                .collect();
            let (source, html, indices, figures, math) =
                crate::pdf::recovery::reading_html(&page, &doc, &headings, *number)?;
            ensure!(
                !require_match || source == *expected,
                "PDF source text no longer matches translation block on page {number}"
            );
            Ok((source == *expected)
                .then_some(html)
                .flatten()
                .map(|html| CodePresentation {
                    html,
                    figures,
                    math,
                    inline_figures: indices
                        .into_iter()
                        .map(|i| format!("pdf-p{number:03}-img{:03}", i + 1))
                        .collect(),
                }))
        })
        .collect())
}

mod png_base64 {
    use base64::{Engine, engine::general_purpose::STANDARD};
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&STANDARD.encode(bytes))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
        STANDARD
            .decode(String::deserialize(deserializer)?)
            .map_err(serde::de::Error::custom)
    }
}
