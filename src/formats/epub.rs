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

struct Archive<R: Read + Seek> {
    zip: ZipArchive<R>,
    max_entry: u64,
}
impl<R: Read + Seek> Archive<R> {
    fn open(reader: R, max_entry: u64, max_total: u64) -> Result<Self> {
        let mut zip = ZipArchive::new(reader)?;
        ensure!(
            zip.len() <= MAX_ENTRIES,
            "EPUB has too many archive entries"
        );
        let mut total = 0u64;
        let mut names = BTreeSet::new();
        for i in 0..zip.len() {
            let entry = zip.by_index(i)?;
            let name = entry.name().trim_end_matches('/');
            ensure!(safe_archive_path(name), "Unsafe EPUB archive path: {name}");
            ensure!(
                names.insert(entry.name().to_owned()),
                "Duplicate EPUB archive path"
            );
            ensure!(
                entry.size() <= max_entry,
                "EPUB archive entry exceeds size limit"
            );
            total = total
                .checked_add(entry.size())
                .context("EPUB size overflow")?;
            ensure!(total <= max_total, "EPUB exceeds unpacked size limit");
        }
        let mut archive = Self { zip, max_entry };
        ensure!(
            archive.read("mimetype")? == b"application/epub+zip",
            "Not an EPUB archive"
        );
        Ok(archive)
    }
    fn read(&mut self, name: &str) -> Result<Vec<u8>> {
        let entry = self
            .zip
            .by_name(name)
            .with_context(|| format!("Missing EPUB resource: {name}"))?;
        let mut bytes = Vec::new();
        entry.take(self.max_entry + 1).read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() as u64 <= self.max_entry,
            "EPUB resource exceeds size limit"
        );
        Ok(bytes)
    }
    fn text(&mut self, name: &str) -> Result<String> {
        let source = String::from_utf8(self.read(name)?)
            .with_context(|| format!("EPUB resource is not UTF-8: {name}"))?;
        Ok(normalize_entities(&source))
    }
    fn contains(&self, name: &str) -> bool {
        self.zip.file_names().any(|n| n == name)
    }
}
// XHTML's external DTD often supplies HTML names such as &copy; and &uuml;.
// Use local HTML5 data, not a network DTD. Numeric XML references preserve
// attribute quoting and do not reinterpret literal CDATA/comment content.
fn normalize_entities(source: &str) -> String {
    static ENTITIES: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    let entities = ENTITIES.get_or_init(|| {
        entities::ENTITIES
            .iter()
            .map(|e| (e.entity, e.characters))
            .collect()
    });
    let mut result = String::with_capacity(source.len());
    let mut offset = 0;
    while offset < source.len() {
        let rest = &source[offset..];
        let Some(next) = rest.find(['<', '&']) else {
            result.push_str(rest);
            break;
        };
        result.push_str(&rest[..next]);
        offset += next;
        let rest = &source[offset..];
        let end = if rest.starts_with("<![CDATA[") {
            rest.find("]]>").map(|end| end + 3)
        } else if rest.starts_with("<!--") {
            rest.find("-->").map(|end| end + 3)
        } else {
            None
        };
        if let Some(end) = end {
            result.push_str(&rest[..end]);
            offset += end;
            continue;
        }
        if rest.starts_with('&')
            && let Some(end) = rest.find(';').filter(|end| *end <= 34)
        {
            let entity = &rest[..=end];
            if !matches!(entity, "&amp;" | "&lt;" | "&gt;" | "&quot;" | "&apos;")
                && let Some(characters) = entities.get(entity)
            {
                for c in characters.chars() {
                    result.push_str(&format!("&#x{:X};", c as u32));
                }
                offset += end + 1;
                continue;
            }
        }
        result.push(rest.chars().next().unwrap());
        offset += 1; // The delimiter is ASCII (< or &).
    }
    result
}
fn safe_archive_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains(['\\', ':', '\0'])
        && path
            .split('/')
            .all(|p| !p.is_empty() && p != "." && p != "..")
}

/// Resolve a URI within the package, never within the host filesystem.
fn resolve(base: &str, href: &str) -> Result<(String, Option<String>)> {
    let (path, fragment) = href
        .split_once('#')
        .map_or((href, None), |(p, f)| (p, Some(f)));
    let path = percent_decode_str(path).decode_utf8()?;
    ensure!(
        !path.starts_with('/') && !path.contains(['\\', ':', '\0', '?']),
        "Unsafe EPUB URI: {href}"
    );
    let mut parts: Vec<&str> = base.split('/').collect();
    if !path.is_empty() {
        parts.pop();
        for part in path.split('/') {
            match part {
                "" | "." => {}
                ".." => {
                    ensure!(parts.pop().is_some(), "EPUB URI escapes package");
                }
                other => parts.push(other),
            }
        }
    }
    let path = parts.join("/");
    ensure!(safe_archive_path(&path), "Unsafe resolved EPUB URI");
    let fragment = fragment
        .map(|f| percent_decode_str(f).decode_utf8().map(|f| f.into_owned()))
        .transpose()?;
    Ok((path, fragment))
}
fn target_key(path: &str, fragment: Option<&str>) -> String {
    fragment
        .filter(|s| !s.is_empty())
        .map_or_else(|| path.to_owned(), |f| format!("{path}#{f}"))
}
fn parse(text: &str) -> Result<Document<'_>> {
    // Standard XHTML DOCTYPEs are accepted, but external entities are not loaded.
    let document = Document::parse_with_options(
        text,
        roxmltree::ParsingOptions {
            allow_dtd: true,
            nodes_limit: 1_000_000,
            ..Default::default()
        },
    )?;
    ensure!(
        document
            .descendants()
            .all(|n| n.ancestors().take(129).count() <= 128),
        "EPUB XML nesting exceeds limit"
    );
    Ok(document)
}
fn text(node: Node<'_, '_>) -> String {
    node.descendants()
        .filter(|n| n.is_text())
        .filter_map(|n| n.text())
        .collect::<String>()
}
fn compact(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}
fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
fn token(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))[..20].to_owned()
}
fn figure_caption<'a, 'input>(node: Node<'a, 'input>) -> Option<Node<'a, 'input>> {
    node.ancestors()
        .find(|n| n.has_tag_name("figure"))
        .and_then(|figure| {
            figure
                .descendants()
                .find(|n| n.has_tag_name("figcaption"))
                .or_else(|| figure.descendants().find(|n| n.has_tag_name("h6")))
        })
}
fn numbered_figure_label(text: &str) -> bool {
    let mut words = text.split_whitespace();
    if !matches!(words.next(), Some("Figure" | "Fig.")) {
        return false;
    }
    let Some(number) = words.next() else {
        return false;
    };
    let number = number.trim_end_matches(['.', ':', ')']);
    let mut parts = number.split('.');
    let first = parts.next().unwrap_or("");
    let digits = |part: &str| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit());
    if digits(first) {
        return parts.all(digits);
    }
    // Appendix figures use letters (A.6, D.2, E.5, etc.), not just digits.
    // Require a numeric suffix so ordinary headings such as "Figure design"
    // are never treated as captions merely because they follow an image.
    !first.is_empty()
        && first.chars().all(|c| c.is_ascii_uppercase())
        && parts.clone().next().is_some()
        && parts.all(digits)
}

fn publisher_figure_caption(node: Node<'_, '_>) -> bool {
    if !matches!(node.tag_name().name(), "h5" | "h6") {
        return false;
    }
    let Some(parent) = node.parent_element() else {
        return false;
    };
    let has_image = parent
        .children()
        .any(|n| n.has_tag_name("img") || n.has_tag_name("image"));
    let caption_class = node.attribute("class").is_some_and(|classes| {
        classes.split_whitespace().any(|class| {
            matches!(
                class,
                "figure-container-h5" | "figure-container-h6" | "figure-caption"
            )
        })
    });
    has_image && (caption_class || numbered_figure_label(&text(node)))
}

/// Presentation compatibility for books imported before publisher h5/h6
/// figure captions were recognized. Keep IDs, source text and stored data intact.
pub fn is_figure_caption(blocks: &[Value], index: usize) -> bool {
    let Some(block) = blocks.get(index) else {
        return false;
    };
    if block["source_kind"] != "epub-xhtml" {
        return false;
    }
    if block["source_role"] == "figure-caption" {
        return true;
    }
    if block["type"] != "heading"
        || !matches!(block["level"].as_u64(), Some(5 | 6))
        || !numbered_figure_label(block["en"].as_str().unwrap_or(""))
    {
        return false;
    }
    let Some(image) = index.checked_sub(1).and_then(|i| blocks.get(i)) else {
        return false;
    };
    let parent_dom = |block: &Value| {
        block["source_location"]["dom"]
            .as_str()
            .and_then(|dom| dom.rsplit_once('/').map(|(parent, _)| parent.to_owned()))
    };
    image["source_kind"] == "epub-image"
        && image["type"] == "figure"
        && block["source_location"]["href"].as_str().is_some()
        && block["source_location"]["href"] == image["source_location"]["href"]
        && parent_dom(block).is_some()
        && parent_dom(block) == parent_dom(image)
}

fn caption_html(html: String) -> String {
    for tag in ["h5", "h6"] {
        if let Some(body) = html
            .strip_prefix(&format!("<{tag}>"))
            .and_then(|body| body.strip_suffix(&format!("</{tag}>")))
        {
            return format!("<p><span>{body}</span></p>");
        }
    }
    html
}

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
struct Content {
    blocks: Vec<Value>,
    targets: BTreeMap<String, String>,
    images: HashMap<String, (String, u32, u32)>,
    warnings: BTreeSet<String>,
    chapter: usize,
}
impl Content {
    fn image<R: Read + Seek>(
        &mut self,
        archive: &mut Archive<R>,
        root: &Path,
        path: &str,
    ) -> Result<(String, u32, u32)> {
        if let Some(image) = self.images.get(path) {
            return Ok(image.clone());
        }
        let bytes = archive.read(path)?;
        let format = image::guess_format(&bytes)?;
        let extension = match format {
            image::ImageFormat::Png => "png",
            image::ImageFormat::Jpeg => "jpg",
            image::ImageFormat::WebP => "webp",
            image::ImageFormat::Gif => "gif",
            _ => bail!("Unsupported EPUB image format: {path}"),
        };
        let (w, h) = image::ImageReader::new(std::io::Cursor::new(&bytes))
            .with_guessed_format()?
            .into_dimensions()?;
        ensure!(
            w > 0 && h > 0 && u64::from(w) * u64::from(h) <= 100_000_000,
            "EPUB image dimensions exceed limit"
        );
        let relative = format!("images/epub-{}.{}", token(path), extension);
        fs::create_dir_all(root.join("images"))?;
        fs::write(root.join(&relative), &bytes)?;
        let src = relative;
        let image = (src, w, h);
        self.images.insert(path.to_owned(), image.clone());
        Ok(image)
    }
    fn register(&mut self, node: Node<'_, '_>, href: &str, id: &str) {
        self.targets
            .entry(href.to_owned())
            .or_insert_with(|| id.to_owned());
        for n in node.descendants().chain(node.ancestors()) {
            if let Some(fragment) = n
                .attribute("id")
                .or_else(|| n.attribute(("http://www.w3.org/XML/1998/namespace", "id")))
            {
                self.targets
                    .entry(target_key(href, Some(fragment)))
                    .or_insert_with(|| id.to_owned());
            }
        }
    }
    fn add(
        &mut self,
        node: Node<'_, '_>,
        href: &str,
        dom: &str,
        kind: &str,
        source: String,
        html: String,
    ) {
        if source.trim().is_empty() {
            return;
        }
        let id = format!("epub-{}", token(&format!("{href}:{dom}")));
        let caption = kind == "figure-caption";
        let kind = if caption { "para" } else { kind };
        let level = (kind == "heading")
            .then(|| {
                node.tag_name()
                    .name()
                    .strip_prefix('h')
                    .and_then(|n| n.parse::<usize>().ok())
            })
            .flatten();
        self.register(node, href, &id);
        self.blocks
            .push(json!({"id":id,"type":kind,"page":0,"chapter":self.chapter,
            "source_kind":"epub-xhtml","en":source,"zh":"","html":html,"level":level,
            "source_location":{"href":href,"fragment":node.attribute("id"),"dom":dom}}));
        if caption {
            self.blocks.last_mut().unwrap()["source_role"] = json!("figure-caption");
        }
    }
    fn walk<R: Read + Seek>(
        &mut self,
        node: Node<'_, '_>,
        href: &str,
        dom: &str,
        archive: &mut Archive<R>,
        root: &Path,
        cancel: &AtomicBool,
    ) -> Result<()> {
        cancelled(cancel)?;
        let tag = node.tag_name().name();
        if matches!(
            tag,
            "head" | "script" | "style" | "iframe" | "object" | "audio" | "video"
        ) {
            return Ok(());
        }
        if node.attribute("hidden").is_some() {
            return Ok(());
        }
        if matches!(tag, "img" | "image") {
            let src = node
                .attribute("src")
                .or_else(|| node.attribute("href"))
                .or_else(|| node.attribute(("http://www.w3.org/1999/xlink", "href")));
            if let Some(src) = src {
                match resolve(href, src).and_then(|(path, _)| self.image(archive, root, &path)) {
                    Ok((src, width, height)) => {
                        let id = format!("epub-{}", token(&format!("{href}:{dom}")));
                        self.register(node, href, &id);
                        let caption = figure_caption(node)
                            .map(|n| compact(&text(n)))
                            .unwrap_or_default();
                        self.blocks.push(json!({"id":id,"type":"figure","page":0,"chapter":self.chapter,
                            "source_kind":"epub-image","src":src,"width":width,"height":height,
                            "caption_en":caption,"caption_zh":"","alt":node.attribute("alt").unwrap_or(""),
                            "source_location":{"href":href,"dom":dom}}));
                    }
                    Err(e) => {
                        self.warnings.insert(format!("Image skipped: {src}: {e}"));
                        let alt = node.attribute("alt").unwrap_or("Image unavailable");
                        self.add(
                            node,
                            href,
                            dom,
                            "para",
                            alt.to_owned(),
                            format!("<p>{}</p>", escape(alt)),
                        );
                    }
                }
            }
            return Ok(());
        }
        if figure_caption(node) == Some(node) {
            return Ok(());
        }
        let semantic = matches!(
            tag,
            "h1" | "h2"
                | "h3"
                | "h4"
                | "h5"
                | "h6"
                | "p"
                | "pre"
                | "ul"
                | "ol"
                | "dl"
                | "table"
                | "blockquote"
        );
        if semantic {
            let caption = publisher_figure_caption(node);
            let heading = !caption && matches!(tag, "h1" | "h2" | "h3" | "h4" | "h5" | "h6");
            let source = if heading {
                compact(&text(node))
            } else {
                markdown(node, false)
            };
            self.add(
                node,
                href,
                dom,
                if caption {
                    "figure-caption"
                } else if heading {
                    "heading"
                } else {
                    "para"
                },
                source.trim().to_owned(),
                if caption {
                    caption_html(render(node, href))
                } else {
                    render(node, href)
                },
            );
            // Ordinary EPUB illustrations are block images. Mixed inline images
            // are retained after their containing semantic block, not discarded.
            for (i, image) in node
                .descendants()
                .filter(|n| n.has_tag_name("img") || n.has_tag_name("image"))
                .enumerate()
            {
                self.walk(
                    image,
                    href,
                    &format!("{dom}/image-{i}"),
                    archive,
                    root,
                    cancel,
                )?;
            }
            return Ok(());
        }
        if tag == "figcaption" && node.ancestors().any(|n| n.has_tag_name("figure")) {
            return Ok(());
        }
        let mut loose = String::new();
        for (i, child) in node.children().enumerate() {
            if child.is_text() {
                loose.push_str(child.text().unwrap_or(""));
            } else if child.is_element() {
                if !loose.trim().is_empty() {
                    let source = compact(&loose);
                    self.add(
                        node,
                        href,
                        &format!("{dom}/text-{i}"),
                        "para",
                        source.clone(),
                        format!("<p>{}</p>", escape(&source)),
                    );
                }
                loose.clear();
                self.walk(child, href, &format!("{dom}/{i}"), archive, root, cancel)?;
            }
        }
        if !loose.trim().is_empty() {
            let source = compact(&loose);
            self.add(
                node,
                href,
                &format!("{dom}/text-end"),
                "para",
                source.clone(),
                format!("<p>{}</p>", escape(&source)),
            );
        }
        Ok(())
    }
}

fn link(href: &str, source: &str) -> Option<String> {
    let source = source.trim();
    if source.starts_with("https://")
        || source.starts_with("http://")
        || source.starts_with("mailto:")
    {
        Some(source.to_owned())
    } else {
        resolve(href, source)
            .ok()
            .map(|(p, f)| format!("vellio-epub:{}", token(&target_key(&p, f.as_deref()))))
    }
}
fn code_language(node: Node<'_, '_>) -> Option<String> {
    let hint = |node: Node<'_, '_>| {
        node.attribute("data-code-language")
            .or_else(|| node.attribute("data-language"))
            .or_else(|| node.attribute("data-lang"))
            .or_else(|| {
                node.attribute("class").and_then(|classes| {
                    classes.split_whitespace().find_map(|class| {
                        class
                            .strip_prefix("language-")
                            .or_else(|| class.strip_prefix("lang-"))
                    })
                })
            })
            .filter(|lang| {
                !lang.is_empty()
                    && lang
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '+' | '#'))
            })
            .map(str::to_owned)
    };
    hint(node).or_else(|| {
        node.descendants()
            .filter(|n| n.has_tag_name("code"))
            .find_map(hint)
    })
}

/// Only reconstruct supported semantic tags; never copy publisher attributes.
fn render(node: Node<'_, '_>, href: &str) -> String {
    if node.is_text() {
        return escape(node.text().unwrap_or(""));
    }
    let tag = node.tag_name().name();
    if matches!(
        tag,
        "script" | "style" | "img" | "image" | "iframe" | "object" | "audio" | "video"
    ) {
        return String::new();
    }
    let children = node.children().map(|n| render(n, href)).collect::<String>();
    if tag == "pre" {
        // One direct code child carries the language. Publisher token-level
        // <code> siblings must not reset the native parser's language hint.
        let attrs = code_language(node)
            .map(|lang| format!(" class=\"language-{lang}\""))
            .unwrap_or_default();
        return format!("<pre><code{attrs}>{children}</code></pre>");
    }
    if tag == "code" && node.ancestors().any(|n| n.has_tag_name("pre")) {
        return children;
    }
    if tag == "br" {
        return "<br />".into();
    }
    if tag == "a" {
        return node
            .attribute("href")
            .and_then(|s| link(href, s))
            .map_or(children.clone(), |url| {
                format!("<a href=\"{}\">{children}</a>", escape(&url))
            });
    }
    let tag = match tag {
        "b" => "strong",
        "i" => "em",
        "s" | "strike" => "del",
        other => other,
    };
    if matches!(
        tag,
        "h1" | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "p"
            | "pre"
            | "code"
            | "strong"
            | "em"
            | "u"
            | "del"
            | "sub"
            | "sup"
            | "ul"
            | "ol"
            | "li"
            | "dl"
            | "dt"
            | "dd"
            | "table"
            | "thead"
            | "tbody"
            | "tr"
            | "th"
            | "td"
            | "blockquote"
    ) {
        if tag == "p" {
            return format!("<p><span>{children}</span></p>");
        }
        let attrs = if tag == "code" {
            node.attribute("class")
                .and_then(|s| {
                    s.split_whitespace().find(|s| {
                        s.starts_with("language-")
                            && s.chars()
                                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
                    })
                })
                .map(|s| format!(" class=\"{s}\""))
                .unwrap_or_default()
        } else {
            String::new()
        };
        format!("<{tag}{attrs}>{children}</{tag}>")
    } else {
        children
    }
}
/// A semantic fallback for translation, edits and export, independent of HTML.
fn markdown(node: Node<'_, '_>, pre: bool) -> String {
    if node.is_text() {
        let source = node.text().unwrap_or("");
        if pre {
            return source.into();
        }
        let mut out = String::new();
        for c in source.chars() {
            if c.is_whitespace() {
                if !out.ends_with(' ') {
                    out.push(' ');
                }
            } else {
                if matches!(c, '\\' | '*' | '_' | '[' | ']' | '`' | '<' | '>') {
                    out.push('\\');
                }
                out.push(c);
            }
        }
        return out;
    }
    let tag = node.tag_name().name();
    if matches!(
        tag,
        "script" | "style" | "img" | "image" | "iframe" | "object"
    ) {
        return String::new();
    }
    if tag == "pre" {
        let source = text(node);
        let fence = "`".repeat(
            source
                .split(|c| c != '`')
                .map(str::len)
                .max()
                .unwrap_or(0)
                .max(2)
                + 1,
        );
        let language = code_language(node).unwrap_or_default();
        return format!("\n\n{fence}{language}\n{source}\n{fence}\n\n");
    }
    if tag == "code" {
        let source = text(node);
        let fence = "`".repeat(source.split(|c| c != '`').map(str::len).max().unwrap_or(0) + 1);
        return format!("{fence} {source} {fence}");
    }
    if matches!(tag, "ul" | "ol") {
        return node
            .children()
            .filter(|n| n.has_tag_name("li"))
            .enumerate()
            .map(|(i, n)| {
                let body = n.children().map(|n| markdown(n, pre)).collect::<String>();
                let prefix = if tag == "ol" {
                    format!("{}. ", i + 1)
                } else {
                    "- ".into()
                };
                format!("{prefix}{}", body.trim().replace('\n', "\n  "))
            })
            .collect::<Vec<_>>()
            .join("\n")
            + "\n\n";
    }
    let children = node
        .children()
        .map(|n| markdown(n, pre))
        .collect::<String>();
    match tag {
        "b" | "strong" => format!("**{children}**"),
        "i" | "em" => format!("*{children}*"),
        "br" => "  \n".into(),
        "p" | "blockquote" | "dl" | "tr" => format!("{}\n\n", children.trim()),
        "dt" | "dd" | "th" | "td" => format!("{}\n", children.trim()),
        _ => children,
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_uris_are_normalized_without_escaping_or_using_host_paths() {
        assert_eq!(
            resolve("OEBPS/Text/ch.xhtml", "../Images/a%20b.png").unwrap(),
            ("OEBPS/Images/a b.png".into(), None)
        );
        assert_eq!(
            resolve("OEBPS/Text/ch.xhtml", "#%E4%B8%AD%E6%96%87").unwrap(),
            ("OEBPS/Text/ch.xhtml".into(), Some("中文".into()))
        );
        for uri in [
            "../../../outside",
            "/etc/passwd",
            "file:///etc/passwd",
            "https://host/a",
            "..\\outside",
            "%2Fetc/passwd",
            "%00",
        ] {
            assert!(resolve("OEBPS/Text/ch.xhtml", uri).is_err(), "{uri}");
        }
        assert!(link("OEBPS/Text/ch.xhtml", "javascript:alert(1)").is_none());
        assert!(link("OEBPS/Text/ch.xhtml", "//example.com/image").is_none());
    }

    #[test]
    fn xml_accepts_standard_doctype_but_does_not_load_external_entities_or_deep_trees() {
        assert!(parse(r#"<!DOCTYPE html PUBLIC "-//W3C//DTD XHTML 1.1//EN" "http://www.w3.org/TR/xhtml11/DTD/xhtml11.dtd"><html><body>ok</body></html>"#).is_ok());
        assert!(parse(r#"<!DOCTYPE html [<!ENTITY secret SYSTEM "file:///etc/passwd">]><html>&secret;</html>"#).is_err());
        assert!(
            parse(&format!(
                "{}text{}",
                "<div>".repeat(130),
                "</div>".repeat(130)
            ))
            .is_err()
        );
    }

    #[test]
    fn xhtml_named_entities_are_local_and_cdata_stays_literal() {
        let source = normalize_entities(
            r#"<p title="&quot;&copy;">&uuml; &copy; &amp;<![CDATA[&copy;]]><!-- &copy; --></p>"#,
        );
        let doc = parse(&source).unwrap();
        assert_eq!(doc.root_element().attribute("title"), Some("\"©"));
        assert_eq!(text(doc.root_element()), "ü © &&copy;");
        assert!(source.contains("<!-- &copy; -->"));
        assert!(source.contains("<![CDATA[&copy;]]>"));
    }

    #[test]
    fn pre_preserves_publisher_language_and_whitespace_without_token_wrappers() {
        for (attrs, child_attrs, language) in [
            ("data-code-language=\"python\"", "class=\"k\"", "python"),
            ("class=\"lang-python\"", "", "python"),
            ("", "class=\"extra language-python\"", "python"),
        ] {
            let source = format!(
                "<pre {attrs}><code {child_attrs}>def</code> hello():\n    <code>return</code> &quot;中文&quot;\n</pre>"
            );
            let doc = parse(&source).unwrap();
            let html = render(doc.root_element(), "OEBPS/ch.xhtml");
            assert_eq!(
                html,
                format!(
                    "<pre><code class=\"language-{language}\">def hello():\n    return &quot;中文&quot;\n</code></pre>"
                )
            );
            assert!(
                markdown(doc.root_element(), false)
                    .contains("```python\ndef hello():\n    return \"中文\"\n")
            );
        }
        let doc =
            parse(r#"<pre data-code-language='python&quot; onclick=&quot;bad'>plain output</pre>"#)
                .unwrap();
        assert_eq!(
            render(doc.root_element(), "ch.xhtml"),
            "<pre><code>plain output</code></pre>"
        );
    }

    #[test]
    fn sanitizer_keeps_code_literal_and_drops_executable_attributes() {
        let doc = parse(r#"<p onclick="bad()">literal __init__ <code>&lt;tag&gt; &amp; `tick`</code><script>bad()</script><a href="file:///secret">blocked</a></p>"#).unwrap();
        let html = render(doc.root_element(), "OEBPS/ch.xhtml");
        assert_eq!(
            html,
            "<p><span>literal __init__ <code>&lt;tag&gt; &amp; `tick`</code>blocked</span></p>"
        );
        assert!(markdown(doc.root_element(), false).contains("\\_\\_init\\_\\_"));
    }
}
