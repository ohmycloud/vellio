//! Bounded ZIP access, safe package paths and offline XML parsing.
use super::*;

pub(super) struct Archive<R: Read + Seek> {
    zip: ZipArchive<R>,
    max_entry: u64,
}
impl<R: Read + Seek> Archive<R> {
    pub(super) fn open(reader: R, max_entry: u64, max_total: u64) -> Result<Self> {
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
    pub(super) fn read(&mut self, name: &str) -> Result<Vec<u8>> {
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
    pub(super) fn text(&mut self, name: &str) -> Result<String> {
        let source = String::from_utf8(self.read(name)?)
            .with_context(|| format!("EPUB resource is not UTF-8: {name}"))?;
        Ok(normalize_entities(&source))
    }
    pub(super) fn contains(&self, name: &str) -> bool {
        self.zip.file_names().any(|n| n == name)
    }
}
// XHTML's external DTD often supplies HTML names such as &copy; and &uuml;.
// Use local HTML5 data, not a network DTD. Numeric XML references preserve
// attribute quoting and do not reinterpret literal CDATA/comment content.
pub(super) fn normalize_entities(source: &str) -> String {
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
pub(super) fn safe_archive_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains(['\\', ':', '\0'])
        && path
            .split('/')
            .all(|p| !p.is_empty() && p != "." && p != "..")
}

/// Resolve a URI within the package, never within the host filesystem.
pub(super) fn resolve(base: &str, href: &str) -> Result<(String, Option<String>)> {
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
pub(super) fn target_key(path: &str, fragment: Option<&str>) -> String {
    fragment
        .filter(|s| !s.is_empty())
        .map_or_else(|| path.to_owned(), |f| format!("{path}#{f}"))
}
pub(super) fn parse(text: &str) -> Result<Document<'_>> {
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
