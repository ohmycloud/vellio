//! Book navigation is distinct from pagination. Prefer publisher navigation,
//! preserving source labels/order/depth instead of collecting every heading.
use easyread_gpui::{model::ReaderState, pdf};
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use serde_json::Value;
use std::{collections::HashMap, path::Path};

#[derive(Debug)]
pub struct Entry {
    pub title: String,
    pub page: usize,
    pub block: Option<String>,
    pub depth: usize,
}

#[derive(Default)]
pub struct Contents {
    pub entries: Vec<Entry>,
    pub inferred: bool,
}

pub fn load(raw: &Value, state: &ReaderState, source_pdf: Option<&Path>, total: usize) -> Contents {
    let epub = raw["paper"]["meta"]["format"] == "epub";
    let position = if epub { "chapter" } else { "page" };
    let blocks = raw["paper"]["blocks"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let targets: HashMap<_, _> = blocks
        .iter()
        .filter_map(|block| Some((block["id"].as_str()?, block)))
        .collect();
    // PDF bookmarks are kept in the source file rather than paper.json.
    if !epub
        && let Some(path) = source_pdf
        && let Ok(bookmarks) = pdf::outline(path)
        && !bookmarks.is_empty()
    {
        return Contents {
            entries: bookmarks
                .into_iter()
                .filter(|entry| entry.page <= total)
                .map(|entry| Entry {
                    title: entry.title,
                    page: entry.page,
                    block: None,
                    depth: entry.depth,
                })
                .collect(),
            inferred: false,
        };
    }
    let outline = raw["paper"]["outline"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    if !outline.is_empty() {
        return Contents {
            entries: outline
                .iter()
                .filter_map(|entry| {
                    let title = entry["title"].as_str()?.to_owned();
                    if title.trim().is_empty() {
                        return None;
                    }
                    let block = entry["block"].as_str();
                    let page = block
                        .and_then(|id| targets.get(id))
                        .and_then(|block| block[position].as_u64())
                        .or_else(|| (!epub).then(|| entry["page"].as_u64()).flatten())?;
                    let page = usize::try_from(page).ok()?;
                    if page == 0 || page > total {
                        return None;
                    }
                    Some(Entry {
                        title,
                        page,
                        block: block
                            .filter(|id| targets.contains_key(id))
                            .map(str::to_owned),
                        depth: usize::try_from(entry["depth"].as_u64().unwrap_or(0))
                            .unwrap_or(usize::MAX),
                    })
                })
                .collect(),
            inferred: false,
        };
    }
    // No publisher navigation: use only real source headings, never captions
    // reclassified as prose by ReaderState, user edits, or fabricated page links.
    let entries = state
        .paper
        .blocks
        .iter()
        .zip(blocks)
        .filter_map(|(block, source)| {
            if block.kind != "heading" {
                return None;
            }
            let title = if block.en.trim().is_empty() {
                &block.zh
            } else {
                &block.en
            };
            let page = usize::try_from(source[position].as_u64()?).ok()?;
            if title.trim().is_empty() || page == 0 || page > total {
                return None;
            }
            Some(Entry {
                title: title.clone(),
                page,
                block: Some(block.id.clone()),
                depth: source["level"].as_u64().unwrap_or(1).clamp(1, 32) as usize - 1,
            })
        })
        .collect();
    Contents {
        entries,
        inferred: true,
    }
}

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
fn segment(value: &str) -> String {
    const SAFE: &percent_encoding::AsciiSet = &NON_ALPHANUMERIC
        .remove(b'-')
        .remove(b'_')
        .remove(b'.')
        .remove(b'~');
    utf8_percent_encode(value, SAFE).to_string()
}

/// Build genuine nested lists (all subsections expanded), not ARIA tree roles
/// that would require a second keyboard-navigation implementation. Only escaped
/// metadata and application-owned URLs reach the trusted HTML output boundary.
pub fn render(contents: &Contents, book: &str, mode: &str, current_page: usize) -> String {
    if contents.entries.is_empty() {
        return String::new();
    }
    let mut html = String::from("<ol class=\"contents-tree\">");
    let mut previous = 0;
    let mut ancestors = Vec::new();
    let mut current_marked = false;
    for (index, entry) in contents.entries.iter().enumerate() {
        // Skip unresolved parents without inventing nodes or turning their
        // surviving children into parents of their own siblings.
        while ancestors.last().is_some_and(|depth| *depth >= entry.depth) {
            ancestors.pop();
        }
        let depth = ancestors.len();
        ancestors.push(entry.depth);
        if index > 0 {
            if depth > previous {
                html.push_str("<ol>");
            } else {
                html.push_str("</li>");
                for _ in depth..previous {
                    html.push_str("</ol></li>");
                }
            }
        }
        html.push_str("<li>");
        let mut href = format!(
            "/read/{}?page={}&mode={}",
            segment(book),
            entry.page,
            segment(mode)
        );
        if let Some(block) = &entry.block {
            href.push_str(&format!("#block-{}", segment(block)));
        }
        let current = !current_marked && entry.page == current_page;
        current_marked |= current;
        html.push_str(&format!(
            "<a class=\"contents-link\" href=\"{}\" data-toc-page=\"{}\" data-toc-block=\"{}\"{}>{}</a>",
            escape(&href), entry.page, escape(entry.block.as_deref().unwrap_or("")),
            if current { " aria-current=\"location\"" } else { "" }, escape(&entry.title),
        ));
        previous = depth;
    }
    html.push_str("</li>");
    for _ in 0..previous {
        html.push_str("</ol></li>");
    }
    html.push_str("</ol>");
    html
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fixture() -> Value {
        json!({"paper":{"meta":{"format":"epub"},"blocks":[
            {"id":"chapter","type":"heading","chapter":1,"level":1,"en":"Source heading","zh":"译文"},
            {"id":"sub","type":"heading","chapter":1,"level":2,"en":"Subheading"},
            {"id":"deep","type":"heading","chapter":2,"level":3,"en":"Deep heading"},
            {"id":"example","type":"heading","chapter":2,"level":6,"en":"Example, not in publisher navigation"}
        ],"outline":[
            {"block":"chapter","title":"Book chapter title","depth":0},
            {"block":"sub","title":"Source subsection title","depth":1},
            {"block":"deep","title":"Original nested section","depth":2}
        ]},"reader":{"edits":{"chapter":{"zh":"Reader's renamed heading"}}}})
    }

    #[test]
    fn source_navigation_preserves_titles_order_subsections_and_targets() {
        let raw = fixture();
        let state = ReaderState::from_value(raw.clone()).unwrap();
        let contents = load(&raw, &state, None, 2);
        assert!(!contents.inferred);
        assert_eq!(
            contents
                .entries
                .iter()
                .map(|e| e.title.as_str())
                .collect::<Vec<_>>(),
            [
                "Book chapter title",
                "Source subsection title",
                "Original nested section"
            ]
        );
        assert_eq!(
            contents.entries.iter().map(|e| e.depth).collect::<Vec<_>>(),
            [0, 1, 2]
        );
        let html = render(&contents, "book", "bilingual", 2);
        let doc = roxmltree::Document::parse(&html).unwrap();
        let links: Vec<_> = doc.descendants().filter(|n| n.has_tag_name("a")).collect();
        assert_eq!(
            links[2]
                .ancestors()
                .filter(|n| n.has_tag_name("ol"))
                .count(),
            3
        );
        assert_eq!(
            links[2].attribute("href"),
            Some("/read/book?page=2&mode=bilingual#block-deep")
        );
        assert!(!html.contains("Example") && !html.contains("Reader&#39;s"));
    }

    #[test]
    fn fallback_uses_source_headings_and_reports_that_it_is_inferred() {
        let mut raw = fixture();
        raw["paper"]["outline"] = json!([]);
        let state = ReaderState::from_value(raw.clone()).unwrap();
        let contents = load(&raw, &state, None, 2);
        assert!(contents.inferred);
        assert_eq!(contents.entries[0].title, "Source heading");
        raw["paper"]["blocks"] = json!([]);
        let state = ReaderState::from_value(raw.clone()).unwrap();
        assert!(load(&raw, &state, None, 2).entries.is_empty());
    }

    #[test]
    fn unresolved_parents_do_not_reparent_sibling_subsections() {
        let mut raw = fixture();
        raw["paper"]["outline"] = json!([
            {"block":"missing","title":"Unresolvable parent","depth":0},
            {"block":"sub","title":"First child","depth":1},
            {"block":"deep","title":"Grandchild","depth":2},
            {"block":"sub","title":"Sibling child","depth":1},
            {"block":"chapter","title":"Next chapter","depth":0}
        ]);
        let state = ReaderState::from_value(raw.clone()).unwrap();
        let contents = load(&raw, &state, None, 2);
        let html = render(&contents, "book", "original", 1);
        let doc = roxmltree::Document::parse(&html).unwrap();
        let depths: Vec<_> = doc
            .descendants()
            .filter(|n| n.has_tag_name("a"))
            .map(|n| n.ancestors().filter(|n| n.has_tag_name("ol")).count() - 1)
            .collect();
        assert_eq!(depths, [0, 1, 0, 0]);
        assert!(!html.contains("Unresolvable parent"));
    }

    #[test]
    fn metadata_is_escaped_hierarchy_is_valid_and_no_entries_are_truncated() {
        let contents = Contents {
            inferred: false,
            entries: (0..650)
                .map(|i| Entry {
                    title: format!("{i} <script>alert('title')</script> & \"long full title\""),
                    page: 1,
                    block: Some("中文 #/anchor".into()),
                    depth: if i == 0 { 4 } else { i % 4 },
                })
                .collect(),
        };
        let html = render(&contents, "book", "original", 1);
        let doc = roxmltree::Document::parse(&html).unwrap();
        assert_eq!(
            doc.descendants().filter(|n| n.has_tag_name("a")).count(),
            650
        );
        assert!(!html.contains("<script>"));
        assert!(html.contains("#block-%E4%B8%AD%E6%96%87%20%23%2Fanchor"));
        assert_eq!(
            doc.descendants()
                .filter(|n| n.attribute("aria-current").is_some())
                .count(),
            1
        );
    }
}
