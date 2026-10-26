//! Serialized PDFium access, import and read-only presentation recovery.
use anyhow::{Context, Result, ensure};
use pdfium_render::prelude::*;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

mod figures;
mod geometry;
mod import;
pub mod layout_cache;
mod outline;
mod presentation;
pub(crate) mod recovery;
mod runtime;

pub(crate) use figures::reading_figures;
#[cfg(test)]
pub(crate) use figures::reading_visual_bounds;
use figures::*;
use geometry::*;
pub(crate) use geometry::{
    hidden_text_indices, page_background_indices, rectangle_bounds, text_panel_bounds,
};
pub use import::{crop, media, prepare};
pub(crate) use outline::document_outline;
pub use outline::{OutlineEntry, link_destination, outline};
pub use presentation::{
    CodePresentation, FigurePreview, MathPresentation, code_html, code_presentation,
    code_presentations, translation_presentations,
};
use runtime::{PDF_LOCK, library};

pub struct Extracted {
    pub title: String,
    pub author: String,
    pub pages: Vec<Value>,
    pub blocks: Vec<Value>,
    pub layout: Value,
}
pub struct Media {
    pub blocks: Vec<Value>,
    pub layout: Value,
}

#[cfg(test)]
mod tests;
