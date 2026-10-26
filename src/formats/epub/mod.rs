//! Bounded, offline EPUB 2/3 reflow import. Publisher CSS/scripts are not executed.
//! Source XHTML stays in source.epub; the reader receives allowlisted HTML and
//! Markdown source fields for the existing translation/edit/annotation pipeline.
use anyhow::{Context, Result, bail, ensure};
use percent_encoding::percent_decode_str;
use roxmltree::{Document, Node};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fs::{self, File},
    io::{Read, Seek},
    path::Path,
    sync::{
        OnceLock,
        atomic::{AtomicBool, Ordering},
    },
};
use zip::ZipArchive;

const MAX_ENTRY: u64 = 64 * 1024 * 1024;
const MAX_TOTAL: u64 = 512 * 1024 * 1024;
const MAX_ENTRIES: usize = 10_000;

mod archive;
mod content;
mod markup;
use archive::*;
use content::*;
pub use markup::is_figure_caption;
use markup::*;
#[cfg(test)]
mod tests;

fn cancelled(cancel: &AtomicBool) -> Result<()> {
    ensure!(!cancel.load(Ordering::Relaxed), "EPUB import cancelled");
    Ok(())
}

#[derive(Clone)]
struct Resource {
    path: String,
    media: String,
    properties: String,
}
fn collect_nav(
    node: Node<'_, '_>,
    base: &str,
    depth: usize,
    entries: &mut Vec<(String, String, usize)>,
) -> Result<()> {
    if node.has_tag_name("li") {
        if let Some(a) = node.children().find(|n| n.has_tag_name("a"))
            && let Some(href) = a.attribute("href")
        {
            let (p, f) = resolve(base, href)?;
            entries.push((compact(&text(a)), target_key(&p, f.as_deref()), depth));
        }
        for ol in node.children().filter(|n| n.has_tag_name("ol")) {
            for li in ol.children().filter(|n| n.has_tag_name("li")) {
                collect_nav(li, base, depth + 1, entries)?;
            }
        }
    } else if node.has_tag_name("navPoint") {
        let label = node
            .children()
            .find(|n| n.has_tag_name("navLabel"))
            .map(|n| compact(&text(n)))
            .unwrap_or_default();
        if let Some(src) = node
            .children()
            .find(|n| n.has_tag_name("content"))
            .and_then(|n| n.attribute("src"))
        {
            let (p, f) = resolve(base, src)?;
            entries.push((label, target_key(&p, f.as_deref()), depth));
        }
        for child in node.children().filter(|n| n.has_tag_name("navPoint")) {
            collect_nav(child, base, depth + 1, entries)?;
        }
    }
    Ok(())
}

/// Parse into schema-2 without inventing physical pages. All resource writes
/// go to the caller's staging directory; publication remains a library transaction.
pub fn prepare(
    path: &Path,
    root: &Path,
    cancel: &AtomicBool,
    progress: impl FnMut(usize, usize),
) -> Result<Value> {
    prepare_archive(
        File::open(path)?,
        root,
        MAX_ENTRY,
        MAX_TOTAL,
        cancel,
        progress,
    )
}

fn prepare_archive<R: Read + Seek>(
    reader: R,
    root: &Path,
    max_entry: u64,
    max_total: u64,
    cancel: &AtomicBool,
    mut progress: impl FnMut(usize, usize),
) -> Result<Value> {
    cancelled(cancel)?;
    let mut archive = Archive::open(reader, max_entry, max_total)?;
    let container = archive.text("META-INF/container.xml")?;
    let container = parse(&container)?;
    let opf = container
        .descendants()
        .find(|n| n.has_tag_name("rootfile"))
        .and_then(|n| n.attribute("full-path"))
        .context("Missing EPUB rootfile")?;
    ensure!(safe_archive_path(opf), "Unsafe EPUB rootfile path");
    let package = archive.text(opf)?;
    let package = parse(&package)?;
    let package_node = package.root_element();
    ensure!(
        matches!(
            package_node.attribute("version"),
            Some("2.0" | "3.0" | "3.1" | "3.2" | "3.3")
        ),
        "Unsupported EPUB version"
    );
    ensure!(
        !package
            .descendants()
            .any(|n| n.attribute("property") == Some("rendition:layout")
                && text(n).trim() == "pre-paginated"),
        "Fixed-layout EPUB is not supported"
    );
    let meta = |name| {
        package
            .descendants()
            .filter(|n| n.has_tag_name(name))
            .map(|n| compact(&text(n)))
            .collect::<Vec<_>>()
    };
    let titles = meta("title");
    let authors = meta("creator");
    let languages = meta("language");
    let mut resources = BTreeMap::new();
    for item in package.descendants().filter(|n| n.has_tag_name("item")) {
        let id = item
            .attribute("id")
            .context("EPUB manifest item has no id")?;
        let (path, _) = resolve(
            opf,
            item.attribute("href")
                .context("EPUB manifest item has no href")?,
        )?;
        ensure!(
            resources
                .insert(
                    id.to_owned(),
                    Resource {
                        path,
                        media: item.attribute("media-type").unwrap_or("").into(),
                        properties: item.attribute("properties").unwrap_or("").into()
                    }
                )
                .is_none(),
            "Duplicate EPUB manifest id"
        );
    }
    let spine = package
        .descendants()
        .find(|n| n.has_tag_name("spine"))
        .context("Missing EPUB spine")?;
    let reading: Vec<_> = spine
        .children()
        .filter(|n| n.has_tag_name("itemref") && n.attribute("linear") != Some("no"))
        .map(|n| {
            ensure!(
                !n.attribute("properties")
                    .unwrap_or("")
                    .contains("rendition:layout-pre-paginated"),
                "Fixed-layout EPUB is not supported"
            );
            resources
                .get(n.attribute("idref").context("Missing EPUB spine idref")?)
                .cloned()
                .context("Unknown EPUB spine idref")
        })
        .collect::<Result<_>>()?;
    ensure!(!reading.is_empty(), "EPUB spine is empty");
    let mut content = Content {
        blocks: vec![],
        targets: BTreeMap::new(),
        images: HashMap::new(),
        warnings: BTreeSet::from([
            "Publisher CSS and embedded fonts are not applied; semantic reflow only".into(),
        ]),
        chapter: 0,
    };
    if archive.contains("META-INF/encryption.xml") {
        let encryption = archive.text("META-INF/encryption.xml")?;
        let encryption = parse(&encryption)?;
        for uri in encryption
            .descendants()
            .filter(|n| n.has_tag_name("CipherReference"))
            .filter_map(|n| n.attribute("URI"))
        {
            let (p, _) = resolve("container", uri)?;
            ensure!(
                !resources.values().any(|r| r.path == p
                    && (r.media == "application/xhtml+xml" || r.media.starts_with("image/"))),
                "Encrypted EPUB content/DRM is not supported"
            );
        }
        content
            .warnings
            .insert("Encrypted/obfuscated fonts are ignored".into());
    }
    let mut chapters = Vec::new();
    for (i, resource) in reading.iter().enumerate() {
        cancelled(cancel)?;
        ensure!(
            resource.media == "application/xhtml+xml",
            "Unsupported EPUB spine media: {}",
            resource.media
        );
        let source = archive.text(&resource.path)?;
        let source = parse(&source).with_context(|| format!("Invalid XHTML: {}", resource.path))?;
        let body = source
            .descendants()
            .find(|n| n.has_tag_name("body"))
            .context("EPUB XHTML has no body")?;
        let title = source
            .descendants()
            .find(|n| n.has_tag_name("title"))
            .map(|n| compact(&text(n)))
            .unwrap_or_default();
        content.chapter = i + 1;
        let first = content.blocks.len();
        content.walk(body, &resource.path, "body", &mut archive, root, cancel)?;
        if let Some(block) = content.blocks.get(first) {
            chapters
                .push(json!({"index":i+1,"href":resource.path,"title":title,"block":block["id"]}));
        }
        progress(i + 1, reading.len());
    }
    ensure!(!content.blocks.is_empty(), "EPUB has no readable content");
    let cover_id = package
        .descendants()
        .find(|n| n.has_tag_name("meta") && n.attribute("name") == Some("cover"))
        .and_then(|n| n.attribute("content"));
    let cover = resources
        .values()
        .find(|r| r.properties.split_whitespace().any(|p| p == "cover-image"))
        .or_else(|| cover_id.and_then(|id| resources.get(id)));
    let cover = cover
        .and_then(|r| content.image(&mut archive, root, &r.path).ok())
        .map(|image| image.0);
    let nav = resources
        .values()
        .find(|r| r.properties.split_whitespace().any(|p| p == "nav"));
    let ncx = spine
        .attribute("toc")
        .and_then(|id| resources.get(id))
        .or_else(|| {
            resources
                .values()
                .find(|r| r.media == "application/x-dtbncx+xml")
        });
    let mut nav_entries = Vec::new();
    if let Some(resource) = nav.or(ncx) {
        let source = archive.text(&resource.path)?;
        let source = parse(&source)?;
        if nav.is_some() {
            if let Some(toc) = source.descendants().find(|n| {
                n.has_tag_name("nav")
                    && n.attribute(("http://www.idpf.org/2007/ops", "type"))
                        .is_some_and(|t| t.split_whitespace().any(|t| t == "toc"))
            }) && let Some(ol) = toc.children().find(|n| n.has_tag_name("ol"))
            {
                for li in ol.children().filter(|n| n.has_tag_name("li")) {
                    collect_nav(li, &resource.path, 0, &mut nav_entries)?;
                }
            }
        } else if let Some(map) = source.descendants().find(|n| n.has_tag_name("navMap")) {
            for point in map.children().filter(|n| n.has_tag_name("navPoint")) {
                collect_nav(point, &resource.path, 0, &mut nav_entries)?;
            }
        }
    }
    let outline: Vec<_> = nav_entries
        .into_iter()
        .filter_map(|(title, target, depth)| {
            content
                .targets
                .get(&target)
                .or_else(|| content.targets.get(target.split('#').next().unwrap_or("")))
                .map(|id| json!({"title":title,"block":id,"depth":depth}))
        })
        .collect();
    let link_targets: BTreeMap<_, _> = content
        .targets
        .iter()
        .map(|(key, id)| (token(key), id.clone()))
        .collect();
    let warnings: Vec<_> = content.warnings.into_iter().collect();
    Ok(
        json!({"schema":2,"meta":{"title_en":titles.first().cloned().unwrap_or_default(),"title_zh":"","authors":authors,
        "language":languages.first(),"format":"epub","epub":"source.epub","pages":[],"page_count":0,"chapter_count":chapters.len(),"chapters":chapters,"cover":cover},
        "blocks":content.blocks,"outline":outline,"link_targets":link_targets,"import_warnings":warnings,
        "translation":{"scope":"未开始","done_pages":[],"en_pages":[],"note":"EPUB semantic reflow; source language is in meta.language"},"references":[],"glossary":[]}),
    )
}

/// Reader-only source HTML, used only while the source fields remain unedited.
/// Translations/edits always use the ordinary Markdown renderer.
pub fn source_html(
    raw: &Value,
    block: &crate::model::Block,
    state: &crate::model::ReaderState,
    original: bool,
) -> Option<String> {
    (block.source_kind == "epub-xhtml"
        && (original || state.text(&block.id, &block.zh, &block.en) == block.en))
        .then(|| raw["html"].as_str().map(str::to_owned))
        .flatten()
        .map(|html| {
            if raw["source_role"] == "figure-caption"
                || (raw["type"] == "heading" && block.kind == "para")
            {
                caption_html(html)
            } else {
                html
            }
        })
}
