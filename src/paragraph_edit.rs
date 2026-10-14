//! A paragraph-scoped edit retains the surrounding native HTML. Stable block
//! IDs and source documents remain unchanged; Markdown is the export projection.
use anyhow::{Context, Result, ensure};
use roxmltree::{Document, Node};
use std::ops::Range;

fn paragraph_nodes<'a, 'input>(doc: &'a Document<'input>) -> Vec<Node<'a, 'input>> {
    doc.root_element()
        .descendants()
        .filter(|node| {
            node.is_element()
                && matches!(
                    node.tag_name().name(),
                    "p" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "pre" | "li"
                )
                && !node
                    .ancestors()
                    .skip(1)
                    .any(|parent| matches!(parent.tag_name().name(), "pre" | "li" | "table"))
        })
        .collect()
}

#[derive(Clone)]
pub struct ParagraphEdit {
    pub html: String,
    pub range: Range<usize>,
    pub markdown: String,
    pub text: String,
    source_range: Option<Range<usize>>,
}
impl ParagraphEdit {
    pub fn new(markdown: &str, html: &str, paragraph: usize) -> Result<Self> {
        let html = crate::translation_layout::xhtml(html);
        let wrapped = format!("<root>{html}</root>");
        let doc = Document::parse(&wrapped)?;
        let nodes = paragraph_nodes(&doc);
        let node = nodes.get(paragraph).context("无法确定要修订的段落")?;
        ensure!(
            matches!(
                node.tag_name().name(),
                "p" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6"
            ),
            "请直接选择正文段落或标题，列表、表格和代码块暂不支持原地修订"
        );
        ensure!(
            !crate::translation_layout::is_display_equation(*node),
            "独立公式无需翻译"
        );
        ensure!(
            !crate::annotations::plain_text(&wrapped[node.range()])
                .trim()
                .is_empty(),
            "该段落没有可修订文字"
        );
        let range = node.range().start - 6..node.range().end - 6;
        let projected = crate::translation_layout::to_markdown(&html[range.clone()])?;
        // Match duplicate paragraphs in order, rather than always replacing
        // the first occurrence. Exact matches keep neighboring Markdown bytes.
        let occurrence = nodes[..paragraph]
            .iter()
            .filter(|n| {
                crate::translation_layout::to_markdown(&wrapped[n.range()])
                    .ok()
                    .as_deref()
                    == Some(projected.as_str())
            })
            .count();
        let source_range = markdown::to_mdast(markdown, &markdown::ParseOptions::gfm())
            .ok()
            .and_then(|root| {
                root.children()?
                    .iter()
                    .filter_map(|node| node.position())
                    .map(|position| position.start.offset..position.end.offset)
                    .filter(|range| markdown[range.clone()] == projected)
                    .nth(occurrence)
            });
        let text = if let Some(range) = &source_range {
            markdown[range.clone()].to_owned()
        } else {
            projected
        };
        Ok(Self {
            html,
            range,
            markdown: markdown.to_owned(),
            text,
            source_range,
        })
    }

    pub fn merge(&self, text: &str) -> Result<(String, String)> {
        let replacement = crate::formula::native_markdown_html(text);
        let old = &self.html[self.range.clone()];
        // Retain centering/indentation attributes on ordinary single-paragraph
        // edits; a deliberate multi-paragraph edit becomes multiple nodes.
        let old_tag = old[1..].split([' ', '>']).next().unwrap_or("p");
        let new_opening = format!("<{old_tag}>");
        let replacement = if replacement.starts_with(&new_opening)
            && replacement.trim_end().ends_with(&format!("</{old_tag}>"))
            && replacement.matches(&new_opening).count() == 1
        {
            let opening = old.find('>').context("Invalid paragraph")? + 1;
            format!("{}{}", &old[..opening], &replacement[new_opening.len()..])
        } else {
            replacement
        };
        let mut html = self.html.clone();
        html.replace_range(self.range.clone(), &replacement);
        let markdown = if let Some(range) = &self.source_range {
            let mut markdown = self.markdown.clone();
            markdown.replace_range(range.clone(), text);
            markdown
        } else {
            // Recovered legacy PDF paragraphs can differ from the stored line
            // breaks. Preserve every untouched HTML node and derive its export.
            crate::translation_layout::to_markdown(&html)?
        };
        Ok((markdown, html))
    }
}

/// Resolve a painted text range against actual HTML nodes, not a guessed
/// ordinal. Tables/code can consume native paragraph ordinals of their own.
pub fn paragraph_at_plain_range(html: &str, selected: &Range<usize>) -> Option<usize> {
    let html = crate::translation_layout::xhtml(html);
    let wrapped = format!("<root>{html}</root>");
    let doc = Document::parse(&wrapped).ok()?;
    let mut marker = "VELLIOSCOPEMARKER".to_owned();
    while html.contains(&marker) {
        marker.push('X');
    }
    paragraph_nodes(&doc)
        .iter()
        .enumerate()
        .find_map(|(index, node)| {
            let range = node.range().start - 6..node.range().end - 6;
            let text = crate::annotations::plain_text(&html[range.clone()]);
            let marked = format!("{}{}{}", &html[..range.start], marker, &html[range.start..]);
            let plain = crate::annotations::plain_text(&marked);
            let start = plain.find(&marker)?;
            ((start..start + text.len()) == *selected).then_some(index)
        })
}

/// Split a render flow around a native paragraph while balancing enclosing
/// companion blockquotes. The selected paragraph itself is replaced by UI.
pub fn split_flow(html: &str, paragraph: usize) -> Result<(String, String)> {
    let html = crate::translation_layout::xhtml(html);
    let wrapped = format!("<root>{html}</root>");
    let doc = Document::parse(&wrapped)?;
    let nodes = paragraph_nodes(&doc);
    let node = nodes.get(paragraph).context("无法确定段落在阅读区的位置")?;
    let range = node.range().start - 6..node.range().end - 6;
    let ancestors: Vec<_> = node
        .ancestors()
        .skip(1)
        .take_while(|n| n.is_element() && !n.has_tag_name("root"))
        .collect();
    let mut before = html[..range.start].to_owned();
    for ancestor in &ancestors {
        before.push_str(&format!("</{}>", ancestor.tag_name().name()));
    }
    let mut after = String::new();
    for ancestor in ancestors.iter().rev() {
        let literal = &wrapped[ancestor.range()];
        after.push_str(&literal[..literal.find('>').context("Invalid ancestor")? + 1]);
    }
    after.push_str(&html[range.end..]);
    Ok((before, after))
}

pub fn edited_html<'a>(edit: &'a serde_json::Value, source: &str, active: &str) -> Option<&'a str> {
    let layout = &edit["native_layout"];
    (edit["zh"] == active
        && edit["reverted"] != true
        && layout["source_hash"] == crate::model::text_hash(source)
        && layout["translation_hash"] == crate::model::text_hash(active))
    .then(|| layout["html"].as_str())
    .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repeated_paragraph_edit_preserves_siblings_math_and_markdown_bytes() {
        let markdown = "第一段。\n\n**重复** $d_k$。\n\n**重复** $d_k$。\n\n末段。";
        let html = crate::formula::native_markdown_html(markdown);
        let edit = ParagraphEdit::new(markdown, &html, 2).unwrap();
        assert_eq!(edit.text, "**重复** $d_k$。");
        let (changed, native) = edit.merge("**修订** $d_v$。").unwrap();
        assert_eq!(
            changed,
            "第一段。\n\n**重复** $d_k$。\n\n**修订** $d_v$。\n\n末段。"
        );
        assert!(native.contains("data-vellio-math=\"d_v\""));
        assert!(native.starts_with(&html[..edit.range.start]));
        assert!(native.ends_with(&html[edit.range.end..]));
    }
    #[test]
    fn painted_range_selects_the_later_duplicate_after_non_prose_content() {
        let html = "<table><tr><td>表格</td></tr></table><p>重复段落。</p><p>重复段落。</p>";
        let plain = crate::annotations::plain_text(html);
        let start = plain.rfind("重复段落。").unwrap();
        assert_eq!(
            paragraph_at_plain_range(html, &(start..start + "重复段落。".len())),
            Some(1)
        );
    }

    #[test]
    fn matching_substring_in_another_paragraph_is_not_the_edit_target() {
        let markdown = "一段中的目标。\n\n目标。\n\n末段。";
        let html = crate::formula::native_markdown_html(markdown);
        let edit = ParagraphEdit::new(markdown, &html, 1).unwrap();
        assert_eq!(
            edit.merge("修订。").unwrap().0,
            "一段中的目标。\n\n修订。\n\n末段。"
        );
    }

    #[test]
    fn updated_translator_text_is_not_overwritten_by_a_stale_paragraph_buffer() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = crate::store::Workspace {
            root: temp.path().to_owned(),
        };
        crate::store::write_json(&workspace.root.join("paper.json"), &serde_json::json!({"meta":{}, "blocks":[{"id":"page", "type":"para", "en":"Source.", "zh":"译者新稿。"}]})).unwrap();
        crate::store::write_json(
            &workspace.root.join("reader.json"),
            &serde_json::json!({"edits":{}}),
        )
        .unwrap();
        let before = std::fs::read(workspace.root.join("reader.json")).unwrap();
        let op = serde_json::json!({"op":"edit", "block":"page", "zh":"基于旧稿的修订。", "expected_edit":null, "expected_text":crate::model::text_hash("译者旧稿。")});
        assert!(workspace.reader_ops(&[op]).is_err());
        assert_eq!(
            std::fs::read(workspace.root.join("reader.json")).unwrap(),
            before
        );
    }

    #[test]
    fn native_edit_layout_is_hash_bound_and_concurrent_save_is_transactional() {
        let mut reader = serde_json::json!({"edits":{}});
        let op = serde_json::json!({"op":"edit", "block":"page", "zh":"修订。", "expected_edit":null,
            "native_layout":{"source_hash":crate::model::text_hash("Source."), "translation_hash":crate::model::text_hash("修订。"), "html":"<p>修订。</p>"}});
        crate::store::apply_ops(&mut reader, std::slice::from_ref(&op)).unwrap();
        assert_eq!(
            edited_html(&reader["edits"]["page"], "Source.", "修订。"),
            Some("<p>修订。</p>")
        );
        assert!(edited_html(&reader["edits"]["page"], "Changed source.", "修订。").is_none());
        let before = reader.clone();
        assert!(crate::store::apply_ops(&mut reader, &[op]).is_err());
        assert_eq!(reader, before);
    }

    #[test]
    fn companion_split_is_balanced_and_preserves_original_flow() {
        let html = "<p>Original.</p><blockquote data-easyread-companion=\"true\"><p>译文。</p></blockquote><p>Next.</p>";
        let (before, after) = split_flow(html, 1).unwrap();
        assert!(before.contains("Original."));
        assert!(after.contains("Next."));
        for part in [before, after] {
            Document::parse(&format!("<root>{part}</root>")).unwrap();
        }
    }
    #[test]
    fn legacy_projection_retains_media_and_caption_alignment() {
        let html = "<p>前段。</p><p><img src=\"figure.png\"/></p><p style=\"text-align:center\">图注。</p><p>末段。</p>";
        let edit = ParagraphEdit::new("前段。 图注。 末段。", html, 2).unwrap();
        let (_, changed) = edit.merge("修订图注。").unwrap();
        assert!(
            changed.contains("<p style=\"text-align:center\">") && changed.contains("修订图注。"),
            "{changed}"
        );
        assert!(changed.contains("<img src=\"figure.png\"/>") && changed.contains("末段。"));
    }
}
