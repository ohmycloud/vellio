//! Display-only PDF footnote metadata for the web reader.
//! Stored text, paragraph order and annotation IDs are never rewritten.
use crate::{paragraph_translation::PairLayout, translation_layout::xhtml};
use roxmltree::Document;

/// Old translations may have the correct skeleton but no footnote attributes.
/// Transfer only across role-checked one-to-one paragraphs with matching labels.
pub fn inherit(original: &str, translation: &str) -> String {
    if !original.contains("data-vellio-footnote=\"true\"")
        || PairLayout::for_bilingual(original, translation, true, None).is_none()
    {
        return translation.to_owned();
    }
    let source = format!("<root>{}</root>", xhtml(original));
    let target = format!("<root>{}</root>", xhtml(translation));
    let (Ok(source_doc), Ok(target_doc)) = (Document::parse(&source), Document::parse(&target))
    else {
        return translation.to_owned();
    };
    let source_nodes: Vec<_> = source_doc
        .root_element()
        .children()
        .filter(|n| n.is_element())
        .collect();
    let target_nodes: Vec<_> = target_doc
        .root_element()
        .children()
        .filter(|n| n.is_element())
        .collect();
    if source_nodes.len() != target_nodes.len() {
        return translation.to_owned();
    }
    let mut output = target[6..target.len() - 7].to_owned();
    for (source_node, target_node) in source_nodes.into_iter().zip(target_nodes).rev() {
        if source_node.attribute("data-vellio-footnote") != Some("true")
            || !target_node.has_tag_name("p")
            || target_node.attribute("data-vellio-footnote").is_some()
        {
            continue;
        }
        let label = |node: roxmltree::Node<'_, '_>, html: &str| {
            crate::annotations::plain_text(&html[node.range()])
                .trim_start()
                .chars()
                .take_while(|c| c.is_ascii_digit() || matches!(c, '*' | '†' | '‡'))
                .collect::<String>()
        };
        let source_label = label(source_node, &source);
        if !source_label.is_empty() && source_label == label(target_node, &target) {
            output.insert_str(
                target_node.range().start - 6 + 2,
                " data-vellio-footnote=\"true\"",
            );
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_metadata_is_inherited_only_with_matching_labels_and_structure() {
        let source = "<p>Body.</p><p data-vellio-footnote=\"true\"><sup>4</sup>Footnote.</p>";
        let target = "<p>正文。</p><p><sup>4</sup>脚注。</p>";
        assert_eq!(
            inherit(source, target),
            "<p>正文。</p><p data-vellio-footnote=\"true\"><sup>4</sup>脚注。</p>"
        );
        assert_eq!(
            inherit(source, "<p>合并正文和脚注。</p>"),
            "<p>合并正文和脚注。</p>"
        );
        assert_eq!(
            inherit(source, "<p>正文。</p><p>5不同的脚注。</p>"),
            "<p>正文。</p><p>5不同的脚注。</p>"
        );
        assert_eq!(
            inherit(source, "<p>正文。</p><p>4独立脚注。</p>"),
            "<p>正文。</p><p data-vellio-footnote=\"true\">4独立脚注。</p>"
        );
    }
}
