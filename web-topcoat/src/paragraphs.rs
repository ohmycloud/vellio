//! DOM paragraph IDs use the same node order as the desktop ParagraphEdit.
//! These IDs are display metadata, never persisted block IDs.
use easyread_gpui::paragraph_edit::ParagraphEdit;
use roxmltree::Document;

pub struct Paragraph {
    pub index: usize,
    pub text: String,
}

pub fn prepare(html: &str, markdown: Option<&str>) -> (String, Vec<Paragraph>) {
    let html = easyread_gpui::translation_layout::xhtml(html);
    let wrapped = format!("<root>{html}</root>");
    let Ok(doc) = Document::parse(&wrapped) else {
        return (html, Vec::new());
    };
    let nodes: Vec<_> = doc
        .root_element()
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
        .collect();
    let mut output = html.clone();
    let mut paragraphs = Vec::new();
    let mut attributes = Vec::new();
    for (index, node) in nodes.iter().enumerate() {
        let plan = markdown.and_then(|text| ParagraphEdit::new(text, &html, index).ok());
        let mut attribute = format!(" data-read-paragraph=\"{index}\"");
        if let Some(plan) = plan {
            attribute.push_str(&format!(" data-edit-paragraph=\"{index}\""));
            paragraphs.push(Paragraph {
                index,
                text: plan.text,
            });
        }
        attributes.push((
            node.range().start - 6 + 1 + node.tag_name().name().len(),
            attribute,
        ));
    }
    for (offset, attribute) in attributes.into_iter().rev() {
        output.insert_str(offset, &attribute);
    }
    (output, paragraphs)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn paragraph_ids_match_desktop_and_ignore_nested_list_table_nodes() {
        let html = "<p>首段。</p><ul><li><p>列表。</p></li></ul><table><tr><td><p>表格。</p></td></tr></table><p>末段。</p>";
        let (tagged, plans) = prepare(html, Some("首段。\n\n末段。"));
        assert_eq!(
            plans.iter().map(|p| p.index).collect::<Vec<_>>(),
            vec![0, 2]
        );
        assert!(tagged.contains("<li data-read-paragraph=\"1\">"));
        assert!(tagged.contains("<p data-read-paragraph=\"2\" data-edit-paragraph=\"2\">末段。"));
        assert!(tagged.contains("<td><p>表格。</p>"));
    }
}
