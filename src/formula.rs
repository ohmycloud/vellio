//! In-process TeX -> MiTeX -> Typst. Reader formulas use native vector layouts,
//! not PDF crops, raster images or SVG image elements. SVG remains an export API.
use anyhow::{Result, ensure};
use std::sync::LazyLock;
use typst::{
    Library, LibraryExt, World,
    diag::{FileError, FileResult},
    foundations::{Bytes, Datetime, Duration},
    syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot},
    text::{Font, FontBook},
    utils::LazyHash,
};
use typst_layout::PagedDocument;

static FONTS: LazyLock<Vec<Font>> = LazyLock::new(|| {
    let mut fonts: Vec<_> = typst_assets::fonts()
        .flat_map(|data| Font::iter(Bytes::new(data)))
        .collect();
    for data in [
        include_bytes!("../assets/fonts/NotoSerifSC-Regular.ttf").as_slice(),
        include_bytes!("../assets/fonts/NotoSerifSC-SemiBold.ttf").as_slice(),
    ] {
        fonts.extend(Font::iter(Bytes::new(data)));
    }
    fonts
});
struct FormulaWorld {
    library: LazyHash<Library>,
    book: LazyHash<FontBook>,
    source: Source,
}
impl World for FormulaWorld {
    fn library(&self) -> &LazyHash<Library> {
        &self.library
    }
    fn book(&self) -> &LazyHash<FontBook> {
        &self.book
    }
    fn main(&self) -> FileId {
        self.source.id()
    }
    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.main() {
            return Ok(self.source.clone());
        }
        let text = match id.vpath().get_with_slash() {
            "/mitex.typ" => include_str!("../assets/mitex/mitex.typ"),
            "/specs/mod.typ" => include_str!("../assets/mitex/specs/mod.typ"),
            "/specs/prelude.typ" => include_str!("../assets/mitex/specs/prelude.typ"),
            "/specs/latex/standard.typ" => include_str!("../assets/mitex/specs/latex/standard.typ"),
            _ => return Err(FileError::AccessDenied),
        };
        Ok(Source::new(id, text.into()))
    }
    fn file(&self, id: FileId) -> FileResult<Bytes> {
        if id.vpath().get_with_slash() == "/mitex.wasm" {
            return Ok(Bytes::new(
                include_bytes!("../assets/mitex/mitex.wasm").as_slice(),
            ));
        }
        Ok(Bytes::from_string(self.source(id)?.text().to_owned()))
    }
    fn font(&self, index: usize) -> Option<Font> {
        FONTS.get(index).cloned()
    }
    fn today(&self, _: Option<Duration>) -> Option<Datetime> {
        None
    }
}
#[path = "formula_layout.rs"]
mod native;
pub use native::{FormulaLayout, FormulaPath, PathCommand, cached_layout, layout, publish_layout};

fn compile(tex: &str, display: bool, margin: u8) -> Result<PagedDocument> {
    ensure!(
        !tex.trim().is_empty() && tex.len() <= 16384,
        "Formula must be 1..16384 bytes"
    );
    let expression = serde_json::to_string(tex)?;
    let handler = if display { "mitex" } else { "mi" };
    let source = format!(
        "#import \"mitex.typ\": mitex, mi\n#set page(width: auto, height: auto, margin: {margin}pt, fill: none)\n#set text(font: \"New Computer Modern Math\", size: 16pt, fill: rgb(\"23211d\"))\n#{handler}({expression})"
    );
    compile_source(source)
}

pub(crate) fn compile_source(source: String) -> Result<PagedDocument> {
    let world = FormulaWorld {
        library: Library::default().into(),
        book: FontBook::from_fonts(FONTS.iter()).into(),
        source: Source::new(
            FileId::new(RootedPath::new(
                VirtualRoot::Project,
                VirtualPath::new("/main.typ").unwrap(),
            )),
            source,
        ),
    };
    let doc = typst::compile::<PagedDocument>(&world)
        .output
        .map_err(|errors| {
            anyhow::anyhow!(
                "Typst: {}",
                errors
                    .iter()
                    .map(|e| e.message.as_str())
                    .collect::<Vec<_>>()
                    .join("; ")
            )
        })?;
    ensure!(
        doc.pages().len() == 1,
        "Typst object unexpectedly spans pages"
    );
    Ok(doc)
}

pub(crate) fn typeset(source: String) -> Result<FormulaLayout> {
    native::document_layout(&compile_source(source)?)
}

pub fn svg(tex: &str, display: bool) -> Result<String> {
    let doc = compile(tex, display, 4)?;
    Ok(typst_svg::svg(&doc.pages()[0], &Default::default()))
}

/// Native HTML objects preserve TeX for atomic selection/copy and accessibility.
pub fn native_html(tex: &str, display: bool) -> String {
    let escaped = tex
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    let attribute = escaped.replace('"', "&quot;");
    format!("<span data-vellio-math=\"{attribute}\" data-display=\"{display}\">${escaped}$</span>")
}

/// Compile Markdown normally, then insert only application-owned math objects.
/// No arbitrary source HTML is enabled and TeX underscores/backslashes cannot
/// be reinterpreted as Markdown formatting.
pub fn native_markdown_html(source: &str) -> String {
    let hash = crate::model::text_hash(source);
    let mut markdown = String::new();
    let mut cursor = 0;
    let mut objects = vec![];
    for (i, (range, tex)) in inline_math(source).into_iter().enumerate() {
        let mut placeholder = format!("VELLIOMATH{hash}X{i}");
        while source.contains(&placeholder) {
            placeholder.push('X');
        }
        markdown.push_str(&source[cursor..range.start]);
        markdown.push_str(&placeholder);
        objects.push((placeholder, native_html(tex, false)));
        cursor = range.end;
    }
    markdown.push_str(&source[cursor..]);
    let mut html = crate::annotations::html(&markdown);
    for (placeholder, object) in objects {
        html = html.replace(&placeholder, &object);
    }
    html
}

/// Single-dollar math outside escaped dollars and Markdown code spans/fences.
/// Storage is unchanged; these ranges are presentation-only UTF-8 byte offsets.
pub fn inline_math(source: &str) -> Vec<(std::ops::Range<usize>, &str)> {
    let bytes = source.as_bytes();
    let mut result = Vec::new();
    let mut i = 0;
    let mut code = 0;
    while i < bytes.len() {
        if bytes[i] == b'`' {
            let start = i;
            while i < bytes.len() && bytes[i] == b'`' {
                i += 1;
            }
            let run = i - start;
            if code == 0 {
                code = run;
            } else if code == run {
                code = 0;
            }
            continue;
        }
        if bytes[i] != b'$' || code > 0 {
            i += 1;
            continue;
        }
        let escaped = bytes[..i].iter().rev().take_while(|&&b| b == b'\\').count() % 2 == 1;
        if escaped {
            i += 1;
            continue;
        }
        if bytes.get(i + 1) == Some(&b'$') {
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'$' && bytes[i + 1] == b'$') {
                i += 1;
            }
            i = (i + 2).min(bytes.len());
            continue;
        }
        let start = i;
        i += 1;
        while i < bytes.len() {
            if bytes[i] == b'\n' {
                break;
            }
            if bytes[i] == b'$'
                && bytes[..i].iter().rev().take_while(|&&b| b == b'\\').count() % 2 == 0
            {
                if i > start + 1 {
                    result.push((start..i + 1, &source[start + 1..i]));
                }
                i += 1;
                break;
            }
            i += 1;
        }
    }
    result
}
