//! Shared reading engine used by the CLI, native desktop and Web experiment.
//! Implementation directories follow domain boundaries; the root re-exports
//! preserve the existing public module paths for callers.
pub mod ai;
pub mod annotations;
pub mod formats;
pub mod pdf;

mod app;
mod library;
mod reader;
mod rendering;

pub use app::{session, settings, updates};
pub use library::{library_home, store};
pub use pdf::layout_cache as pdf_layout_cache;
pub use reader::{
    annotation_panel, epub_pagination, layout, margin_note, model, navigation, paragraph_edit,
    paragraph_translation, reading_footnotes, reading_motion, translation_layout,
};
#[cfg(feature = "syntax-highlighting")]
pub use rendering::code_syntax;
pub use rendering::{figure_image, fonts, formula, typst_table};
