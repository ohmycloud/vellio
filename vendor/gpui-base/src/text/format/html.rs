use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;

use gpui::{DefiniteLength, Hsla, SharedString, px, relative};
use html5ever::tendril::TendrilSink;
use html5ever::{LocalName, ParseOpts, local_name, parse_document};
use markup5ever_rcdom::{Node, NodeData, RcDom};

use crate::text::document::ParsedDocument;
use crate::text::node::{
    self, BlockNode, CodeBlock, ImageNode, InlineNode, LinkMark, NodeContext, Paragraph, Table,
    TableRow, TextMark,
};

const BLOCK_ELEMENTS: [&str; 37] = [
    "html",
    "body",
    "head",
    "address",
    "article",
    "aside",
    "blockquote",
    "details",
    "summary",
    "dialog",
    "div",
    "dl",
    "dt",
    "dd",
    "fieldset",
    "figcaption",
    "figure",
    "footer",
    "form",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "header",
    "hr",
    "main",
    "nav",
    "ol",
    "p",
    "pre",
    "section",
    "table",
    "ul",
    "style",
    "script",
];

/// Parse HTML into AST Node.
pub(crate) fn parse(source: &str, cx: &mut NodeContext) -> Result<ParsedDocument, SharedString> {
    let opts = ParseOpts {
        ..Default::default()
    };

    let bytes = cleanup_html(&source);
    let mut cursor = std::io::Cursor::new(bytes);
    // Ref
    // https://github.com/servo/html5ever/blob/main/rcdom/examples/print-rcdom.rs
    let dom = parse_document(RcDom::default(), opts)
        .from_utf8()
        .read_from(&mut cursor)
        .map_err(|e| SharedString::from(format!("{:?}", e)))?;

    let mut paragraph = Paragraph::default();
    // NOTE: The outer paragraph is not used.
    let node: BlockNode =
        parse_node(&dom.document, &mut paragraph, cx).unwrap_or(BlockNode::Unknown);
    let node = node.compact();

    Ok(ParsedDocument {
        source: source.to_string().into(),
        blocks: Arc::new(vec![node]),
    })
}

fn cleanup_html(source: &str) -> Vec<u8> {
    let mut w = std::io::Cursor::new(vec![]);
    let mut r = std::io::Cursor::new(source);
    let mut minify = super::html5minify::Minifier::new(&mut w);
    minify.omit_doctype(true);
    if let Ok(()) = minify.minify(&mut r) {
        w.into_inner()
    } else {
        source.bytes().collect()
    }
}

fn attr_value(attrs: &RefCell<Vec<html5ever::Attribute>>, name: LocalName) -> Option<String> {
    attrs.borrow().iter().find_map(|attr| {
        if attr.name.local == name {
            Some(attr.value.to_string())
        } else {
            None
        }
    })
}

/// Get the highlight background color for a `<mark>` element.
///
/// Reads the `color` attribute first, then the `background-color` declaration
/// from the `style` attribute. Base accepts CSS hex plus common named colors.
fn mark_color(attrs: &RefCell<Vec<html5ever::Attribute>>) -> Option<Hsla> {
    let color_attr = attrs.borrow().iter().find_map(|attr| {
        if &*attr.name.local == "color" {
            Some(attr.value.to_string())
        } else {
            None
        }
    });

    if let Some(value) = color_attr
        && let Some(color) = parse_mark_color(value.trim())
    {
        return Some(color);
    }

    style_attrs(attrs)
        .get("background-color")
        .and_then(|v| parse_mark_color(v.trim()))
}

fn parse_mark_color(value: &str) -> Option<Hsla> {
    if value.starts_with('#') {
        return gpui::Rgba::try_from(value).ok().map(Into::into);
    }
    match value.to_ascii_lowercase().as_str() {
        "black" => Some(gpui::rgb(0x000000).into()),
        "white" => Some(gpui::rgb(0xffffff).into()),
        "blue" => Some(gpui::rgb(0x3b82f6).into()),
        "yellow" => Some(gpui::rgb(0xfacc15).into()),
        _ => None,
    }
}

/// Get style properties to HashMap
/// TODO: Use cssparser to parse style attribute.
fn style_attrs(attrs: &RefCell<Vec<html5ever::Attribute>>) -> HashMap<String, String> {
    let mut styles = HashMap::new();
    let Some(css_text) = attr_value(attrs, local_name!("style")) else {
        return styles;
    };

    for decl in css_text.split(';') {
        let mut parts = decl.splitn(2, ':');
        if let (Some(key), Some(value)) = (parts.next(), parts.next()) {
            styles.insert(
                key.trim().to_lowercase().to_string(),
                value.trim().to_string(),
            );
        }
    }

    styles
}

/// Parse length value from style attribute.
///
/// When is percentage, it will be converted to relative length.
/// Else, it will be converted to pixels.
fn value_to_length(value: &str) -> Option<DefiniteLength> {
    if value.ends_with("%") {
        value
            .trim_end_matches("%")
            .parse::<f32>()
            .ok()
            .map(|v| relative(v / 100.))
    } else {
        value
            .trim_end_matches("px")
            .parse()
            .ok()
            .map(|v| px(v).into())
    }
}

/// Get width, height from attributes or parse them from style attribute.
fn attr_width_height(
    attrs: &RefCell<Vec<html5ever::Attribute>>,
) -> (Option<DefiniteLength>, Option<DefiniteLength>) {
    let mut width = None;
    let mut height = None;

    if let Some(value) = attr_value(attrs, local_name!("width")) {
        width = value_to_length(&value);
    }

    if let Some(value) = attr_value(attrs, local_name!("height")) {
        height = value_to_length(&value);
    }

    if width.is_none() || height.is_none() {
        let styles = style_attrs(attrs);
        if width.is_none() {
            width = styles.get("width").and_then(|v| value_to_length(&v));
        }
        if height.is_none() {
            height = styles.get("height").and_then(|v| value_to_length(&v));
        }
    }

    (width, height)
}

fn attr_aspect_ratio(attrs: &RefCell<Vec<html5ever::Attribute>>) -> Option<f32> {
    let styles = style_attrs(attrs);
    let value = styles.get("aspect-ratio")?;
    let ratio = if let Some((width, height)) = value.split_once('/') {
        width.trim().parse::<f32>().ok()? / height.trim().parse::<f32>().ok()?
    } else {
        value.trim().parse::<f32>().ok()?
    };
    (ratio.is_finite() && ratio > 0.).then_some(ratio)
}

fn parse_table_row(table: &mut Table, node: &Rc<Node>) {
    let mut row = TableRow::default();
    let mut count = 0;
    let mut has_header = false;
    for child in node.children.borrow().iter() {
        match child.data {
            NodeData::Element {
                ref name,
                ref attrs,
                ..
            } if name.local == local_name!("td") || name.local == local_name!("th") => {
                count += 1;
                has_header |= name.local == local_name!("th");
                parse_table_cell(&mut row, child, attrs);
            }
            _ => {}
        }
    }

    if count > 0 {
        if table.children.is_empty() {
            table.header = Some(has_header);
        }
        table.children.push(row);
    }
}

fn parse_table_cell(
    row: &mut node::TableRow,
    node: &Rc<Node>,
    attrs: &RefCell<Vec<html5ever::Attribute>>,
) {
    let mut paragraph = Paragraph::default();
    for child in node.children.borrow().iter() {
        parse_paragraph(&mut paragraph, child);
    }
    let width = attr_width_height(attrs).0;
    let table_cell = node::TableCell {
        children: paragraph,
        width,
    };
    row.children.push(table_cell);
}

/// Trim text but leave at least one space.
///
/// - Before: " \r\n Hello world \t "
/// - After: " Hello world "
#[allow(dead_code)]
fn trim_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());

    for (i, c) in text.chars().enumerate() {
        if c.is_whitespace() {
            if i > 0 && out.ends_with(' ') {
                continue;
            }
        }
        out.push(c);
    }

    out
}

fn parse_paragraph(paragraph: &mut Paragraph, node: &Rc<Node>) {
    fn push_merged(
        paragraph: &mut Paragraph,
        text: String,
        marks: Vec<(Range<usize>, TextMark)>,
        new_mark: Option<TextMark>,
    ) {
        if text.is_empty() {
            return;
        }
        let mut node = InlineNode::new(text).marks(marks);
        if let Some(new_mark) = new_mark {
            let len = node.text.len();
            let range = if new_mark.link.is_some() {
                let start = len - node.text.trim_start().len();
                start..node.text.trim_end().len().max(start)
            } else {
                0..len
            };
            if range.is_empty() {
                paragraph.push(node);
                return;
            }
            if let Some(last) = node.marks.last_mut()
                && last.0 == range
            {
                last.1.merge(new_mark);
            } else {
                node.marks.push((range, new_mark));
            }
        }
        paragraph.push(node);
    }

    fn merge_children_with_mark(
        node: &Node,
        paragraph: &mut Paragraph,
        new_mark: Option<TextMark>,
    ) {
        let mut merged_text = String::new();
        let mut merged_marks = Vec::new();

        for child in node.children.borrow().iter() {
            let mut child_paragraph = Paragraph::default();
            parse_paragraph(&mut child_paragraph, &child);

            for mut node in child_paragraph.children {
                // Native inline objects must survive nested emphasis/links.
                if node.custom.is_some() {
                    push_merged(
                        paragraph,
                        std::mem::take(&mut merged_text),
                        std::mem::take(&mut merged_marks),
                        new_mark.clone(),
                    );
                    if let Some(mark) = &new_mark {
                        node.marks.push((0..node.text.len(), mark.clone()));
                    }
                    paragraph.push(node);
                    continue;
                }
                let offset = merged_text.len();
                merged_text.push_str(&node.text);
                for (range, child_mark) in node.marks {
                    merged_marks.push((range.start + offset..range.end + offset, child_mark));
                }

                if let Some(mut image) = node.image {
                    if let Some(link_mark) = new_mark.as_ref().and_then(|mark| mark.link.clone()) {
                        image.link = Some(link_mark);
                    }

                    push_merged(
                        paragraph,
                        std::mem::take(&mut merged_text),
                        std::mem::take(&mut merged_marks),
                        new_mark.clone(),
                    );

                    paragraph.push(InlineNode::image(image));
                }
            }
        }

        push_merged(paragraph, merged_text, merged_marks, new_mark.clone());
    }

    match &node.data {
        NodeData::Text { contents } => {
            let part = &contents.borrow();
            paragraph.push_str(&part);
        }
        NodeData::Element { name, attrs, .. } => match name.local {
            local_name!("br") => paragraph.push_str("\n"),
            local_name!("em") | local_name!("i") => {
                merge_children_with_mark(node, paragraph, Some(TextMark::default().italic()));
            }
            local_name!("strong") | local_name!("b") => {
                merge_children_with_mark(node, paragraph, Some(TextMark::default().bold()));
            }
            local_name!("del") | local_name!("s") => {
                merge_children_with_mark(
                    node,
                    paragraph,
                    Some(TextMark::default().strikethrough()),
                );
            }
            local_name!("u") => {
                let underline_color = style_attrs(&attrs)
                    .get("text-decoration-color")
                    .and_then(|value| parse_mark_color(value));
                merge_children_with_mark(
                    node,
                    paragraph,
                    Some(
                        TextMark {
                            underline_color,
                            ..Default::default()
                        }
                        .underline(),
                    ),
                );
            }
            local_name!("sup") => {
                merge_children_with_mark(
                    node,
                    paragraph,
                    Some(TextMark {
                        superscript: true,
                        ..Default::default()
                    }),
                );
            }
            local_name!("code") => {
                merge_children_with_mark(node, paragraph, Some(TextMark::default().code()));
            }
            local_name!("mark") => {
                let color = mark_color(&attrs).unwrap_or_else(|| gpui::rgb(0xfef08a).into());
                merge_children_with_mark(
                    node,
                    paragraph,
                    Some(TextMark::default().highlight(color)),
                );
            }
            local_name!("span") => {
                let table_key = attrs
                    .borrow()
                    .iter()
                    .find(|a| a.name.local.as_ref() == "data-vellio-table-key")
                    .map(|a| a.value.to_string());
                if let Some(key) = table_key {
                    let text = attrs
                        .borrow()
                        .iter()
                        .find(|a| a.name.local.as_ref() == "data-vellio-table-text-json")
                        .and_then(|a| serde_json::from_str::<String>(a.value.as_ref()).ok())
                        .unwrap_or_default();
                    paragraph.push(InlineNode::custom(
                        crate::text::MarkdownNode::new("vellio-table", key)
                            .text(text.clone())
                            .markdown(text),
                    ));
                    return;
                }
                let tex = attrs
                    .borrow()
                    .iter()
                    .find(|a| a.name.local.as_ref() == "data-vellio-math")
                    .map(|a| a.value.to_string());
                if let Some(tex) = tex {
                    let display = attrs.borrow().iter().any(|a| {
                        a.name.local.as_ref() == "data-display" && a.value.as_ref() == "true"
                    });
                    let text = format!("${tex}$");
                    // Saved quote decorations sit INSIDE the native span.
                    // Keep their marks/annotation identity even though TeX
                    // remains one atomic object rather than rendered children.
                    let mut decorated = Paragraph::default();
                    merge_children_with_mark(node, &mut decorated, None);
                    let marks = decorated
                        .children
                        .into_iter()
                        .flat_map(|child| child.marks)
                        .filter(|(range, _)| !range.is_empty())
                        .map(|(_, mark)| (0..text.len(), mark))
                        .collect();
                    paragraph.push(
                        InlineNode::custom(
                            crate::text::MarkdownNode::new("vellio-math", (tex, display))
                                .text(text.clone())
                                .markdown(text),
                        )
                        .marks(marks),
                    );
                    return;
                }
                let styles = style_attrs(attrs);
                let annotation_id = attrs.borrow().iter().find_map(|attr| {
                    (attr.name.local.as_ref() == "data-easyread-note")
                        .then(|| attr.value.to_string().into())
                });
                merge_children_with_mark(
                    node,
                    paragraph,
                    Some(TextMark {
                        annotation_id,
                        color: styles
                            .get("color")
                            .and_then(|value| parse_mark_color(value)),
                        italic: styles
                            .get("font-style")
                            .is_some_and(|value| value == "italic"),
                        bold: styles
                            .get("font-weight")
                            .is_some_and(|value| value == "bold" || value == "700"),
                        ..Default::default()
                    }),
                );
            }
            local_name!("a") => {
                let link_mark = LinkMark {
                    url: attr_value(&attrs, local_name!("href"))
                        .unwrap_or_default()
                        .into(),
                    title: attr_value(&attrs, local_name!("title")).map(Into::into),
                    ..Default::default()
                };

                merge_children_with_mark(
                    node,
                    paragraph,
                    Some(TextMark::default().link(link_mark)),
                );
            }
            local_name!("img") => {
                let Some(src) = attr_value(attrs, local_name!("src")) else {
                    if cfg!(debug_assertions) {
                        tracing::warn!("Image node missing src attribute");
                    }
                    return;
                };

                let alt = attr_value(attrs, local_name!("alt"));
                let title = attr_value(attrs, local_name!("title"));
                let (width, height) = attr_width_height(attrs);

                paragraph.push_image(ImageNode {
                    url: src.into(),
                    alt: alt.map(Into::into),
                    width,
                    height,
                    aspect_ratio: attr_aspect_ratio(attrs),
                    title: title.map(Into::into),
                    ..Default::default()
                });
            }
            _ => {
                merge_children_with_mark(node, paragraph, None);
            }
        },
        _ => {
            merge_children_with_mark(node, paragraph, None);
        }
    }
}

fn parse_node(
    node: &Rc<Node>,
    paragraph: &mut Paragraph,
    cx: &mut NodeContext,
) -> Option<BlockNode> {
    match node.data {
        NodeData::Text { ref contents } => {
            let text = contents.borrow().to_string();
            if text.len() > 0 {
                paragraph.push_str(&text);
            }

            None
        }
        NodeData::Element {
            ref name,
            ref attrs,
            ..
        } => match name.local {
            local_name!("br") => Some(BlockNode::Break {
                html: true,
                span: None,
            }),
            local_name!("pre") => {
                let mut children = vec![];
                consume_paragraph(&mut children, paragraph);
                let mut code = Paragraph::default();
                let mut lang = None;
                for child in node.children.borrow().iter() {
                    if let NodeData::Element { name, attrs, .. } = &child.data
                        && name.local == local_name!("code")
                    {
                        lang = attr_value(attrs, local_name!("class")).and_then(|classes| {
                            classes.split_whitespace().find_map(|class| {
                                class
                                    .strip_prefix("language-")
                                    .filter(|lang| !lang.is_empty())
                                    .map(SharedString::from)
                            })
                        });
                    }
                    parse_paragraph(&mut code, child);
                }
                let mut text = String::new();
                let mut marks = vec![];
                let has_image = code.children.iter().any(|inline| inline.image.is_some());
                for inline in &mut code.children {
                    let offset = text.len();
                    text.push_str(&inline.text);
                    for (range, mark) in &mut inline.marks {
                        // The whole block is monospace already. Inline code
                        // styling would add chips and shrink individual runs.
                        mark.code = false;
                        marks.push((offset + range.start..offset + range.end, mark.clone()));
                    }
                }
                let mut block = CodeBlock::new(text.into(), lang, None::<node::Span>);
                block.marks = marks;
                if has_image {
                    block.inline_content = Some(code);
                }
                children.push(BlockNode::CodeBlock(block));
                Some(
                    BlockNode::Root {
                        children,
                        span: None,
                    }
                    .compact(),
                )
            }
            local_name!("h1")
            | local_name!("h2")
            | local_name!("h3")
            | local_name!("h4")
            | local_name!("h5")
            | local_name!("h6") => {
                let mut children = vec![];
                consume_paragraph(&mut children, paragraph);

                let level = name
                    .local
                    .chars()
                    .last()
                    .unwrap_or('6')
                    .to_digit(10)
                    .unwrap_or(6) as u8;

                let mut paragraph = Paragraph::default();
                for child in node.children.borrow().iter() {
                    parse_paragraph(&mut paragraph, child);
                }

                let heading = BlockNode::Heading {
                    level,
                    children: paragraph,
                    span: None,
                };
                if children.len() > 0 {
                    children.push(heading);

                    Some(BlockNode::Root {
                        children,
                        span: None,
                    })
                } else {
                    Some(heading)
                }
            }
            local_name!("img") => {
                let mut children = vec![];
                consume_paragraph(&mut children, paragraph);

                let Some(src) = attr_value(attrs, local_name!("src")) else {
                    if cfg!(debug_assertions) {
                        tracing::warn!("image node missing src attribute");
                    }
                    return None;
                };

                let alt = attr_value(&attrs, local_name!("alt"));
                let title = attr_value(&attrs, local_name!("title"));
                let (width, height) = attr_width_height(&attrs);

                let mut paragraph = Paragraph::default();
                paragraph.push_image(ImageNode {
                    url: src.into(),
                    title: title.map(Into::into),
                    alt: alt.map(Into::into),
                    width,
                    height,
                    aspect_ratio: attr_aspect_ratio(attrs),
                    ..Default::default()
                });

                if children.len() > 0 {
                    children.push(BlockNode::Paragraph(paragraph));
                    Some(BlockNode::Root {
                        children,
                        span: None,
                    })
                } else {
                    Some(BlockNode::Paragraph(paragraph))
                }
            }
            local_name!("ul") | local_name!("ol") => {
                let ordered = name.local == local_name!("ol");
                let children = consume_children_nodes(node, paragraph, cx);
                Some(BlockNode::List {
                    children,
                    ordered,
                    span: None,
                })
            }
            local_name!("li") => {
                let mut children = vec![];
                consume_paragraph(&mut children, paragraph);

                for child in node.children.borrow().iter() {
                    let mut child_paragraph = Paragraph::default();
                    if let Some(child_node) = parse_node(child, &mut child_paragraph, cx) {
                        children.push(child_node);
                    }
                    if child_paragraph.text_len() > 0 {
                        // If last child is paragraph, merge child
                        if let Some(last_child) = children.last_mut() {
                            if let BlockNode::Paragraph(last_paragraph) = last_child {
                                last_paragraph.merge(child_paragraph);
                                continue;
                            }
                        }

                        children.push(BlockNode::Paragraph(child_paragraph));
                    }
                }

                consume_paragraph(&mut children, paragraph);

                Some(BlockNode::ListItem {
                    children,
                    spread: false,
                    checked: None,
                    span: None,
                })
            }
            local_name!("table") => {
                let mut children = vec![];
                consume_paragraph(&mut children, paragraph);

                let mut table = Table::default();
                for child in node.children.borrow().iter() {
                    match child.data {
                        NodeData::Element { ref name, .. }
                            if name.local == local_name!("tbody")
                                || name.local == local_name!("thead") =>
                        {
                            for sub_child in child.children.borrow().iter() {
                                parse_table_row(&mut table, &sub_child);
                            }
                        }
                        _ => {
                            parse_table_row(&mut table, &child);
                        }
                    }
                }
                consume_paragraph(&mut children, paragraph);

                let table = BlockNode::Table(table);
                if children.len() > 0 {
                    children.push(table);
                    Some(BlockNode::Root {
                        children,
                        span: None,
                    })
                } else {
                    Some(table)
                }
            }
            local_name!("blockquote") => {
                let children = consume_children_nodes(node, paragraph, cx);
                Some(BlockNode::Blockquote {
                    children,
                    span: None,
                    companion: attr_value(attrs, "data-easyread-companion".into()).as_deref()
                        == Some("true"),
                })
            }
            local_name!("style") | local_name!("script") => None,
            _ => {
                if BLOCK_ELEMENTS.contains(&name.local.trim()) {
                    let mut children: Vec<BlockNode> = vec![];

                    // Case:
                    //
                    // Hello <p>Inner text of block element</p> World

                    // Insert before text as a node -- The "Hello"
                    consume_paragraph(&mut children, paragraph);

                    if name.local == local_name!("p") {
                        paragraph.centered = style_attrs(attrs)
                            .get("text-align")
                            .is_some_and(|value| value.eq_ignore_ascii_case("center"));
                    }
                    if name.local == local_name!("dd") {
                        paragraph.indent_em = attrs
                            .borrow()
                            .iter()
                            .find_map(|attr| {
                                (attr.name.local.as_ref() == "data-easyread-indent")
                                    .then(|| attr.value.parse::<f32>().ok())
                                    .flatten()
                            })
                            .filter(|v| v.is_finite() && *v >= 0. && *v <= 6.)
                            .unwrap_or(1.5);
                    }

                    // Inner of the block element -- The "Inner text of block element"
                    for child in node.children.borrow().iter() {
                        if let Some(child_node) = parse_node(child, paragraph, cx) {
                            children.push(child_node);
                        }
                    }
                    consume_paragraph(&mut children, paragraph);

                    if children.is_empty() {
                        None
                    } else {
                        Some(BlockNode::Root {
                            children,
                            span: None,
                        })
                    }
                } else {
                    // Others to as Inline
                    parse_paragraph(paragraph, node);

                    if paragraph.is_image() {
                        Some(BlockNode::Paragraph(paragraph.take()))
                    } else {
                        None
                    }
                }
            }
        },
        NodeData::Document => {
            let children = consume_children_nodes(node, paragraph, cx);
            Some(BlockNode::Root {
                children,
                span: None,
            })
        }
        NodeData::Doctype { .. }
        | NodeData::Comment { .. }
        | NodeData::ProcessingInstruction { .. } => None,
    }
}

fn consume_children_nodes(
    node: &Node,
    paragraph: &mut Paragraph,
    cx: &mut NodeContext,
) -> Vec<BlockNode> {
    let mut children = vec![];
    consume_paragraph(&mut children, paragraph);
    for child in node.children.borrow().iter() {
        if let Some(child_node) = parse_node(child, paragraph, cx) {
            children.push(child_node);
        }
        consume_paragraph(&mut children, paragraph);
    }

    children
}

fn consume_paragraph(children: &mut Vec<BlockNode>, paragraph: &mut Paragraph) {
    if paragraph.is_empty() {
        return;
    }

    children.push(BlockNode::Paragraph(paragraph.take()));
}

#[cfg(test)]
mod tests {
    use gpui::{px, relative};
    use std::sync::Arc;

    use crate::text::{
        document::ParsedDocument,
        node::{BlockNode, ImageNode, InlineNode, NodeContext, Paragraph},
    };

    use super::trim_text;

    #[test]
    fn bilingual_companions_are_distinct_from_publisher_quotes() {
        for (attribute, expected) in [("", false), (" data-easyread-companion=\"true\"", true)] {
            let html = format!("<blockquote{attribute}><p>Text.</p></blockquote>");
            let doc = super::parse(&html, &mut NodeContext::default()).unwrap();
            let BlockNode::Blockquote { companion, .. } = &doc.blocks[0] else {
                panic!("blockquote")
            };
            assert_eq!(*companion, expected);
        }
    }

    #[test]
    fn html_table_preserves_column_widths_empty_cells_and_inline_breaks() {
        let html = "<table><tr><th width=\"18%\">Type</th><th width=\"82%\">Benefits</th></tr><tr><td width=\"18%\"><strong>Projection-<br>based</strong></td><td width=\"82%\"> </td></tr></table>";
        let doc = super::parse(html, &mut NodeContext::default()).unwrap();
        let BlockNode::Table(table) = &doc.blocks[0] else {
            panic!("table")
        };
        assert_eq!(table.children.len(), 2);
        assert!(table.has_header());
        assert_eq!(table.children[1].children.len(), 2);
        assert_eq!(table.children[0].children[0].width, Some(relative(0.18)));
        assert_eq!(table.children[1].children[0].width, Some(relative(0.18)));
        assert_eq!(
            table.children[1].children[0].children.text(),
            "Projection-\nbased"
        );
        assert!(
            table.children[1].children[0]
                .children
                .children
                .iter()
                .flat_map(|node| node.marks.iter())
                .any(|(_, mark)| mark.bold)
        );
    }

    #[test]
    fn headerless_html_table_keeps_first_row_as_data_in_rendering_and_copy() {
        let doc = super::parse(
            "<table><tr><td width=\"18%\"> </td><td width=\"82%\"><em>Lower ceiling</em> Body text.</td></tr></table>",
            &mut NodeContext::default(),
        ).unwrap();
        let BlockNode::Table(table) = &doc.blocks[0] else {
            panic!("table")
        };
        assert!(!table.has_header());
        let data = table.table_data();
        assert!(data.headers.is_empty());
        assert_eq!(data.rows, vec![vec!["", "Lower ceiling Body text."]]);
        assert_eq!(
            data.markdown,
            "|  |  |\n| :-- | :-- |\n|  | *Lower ceiling* Body text. |"
        );
    }

    #[test]
    fn html_pre_keeps_language_whitespace_entities_and_annotations() {
        let mut cx = NodeContext::default();
        let code = "  fn main() {\n\tlet 名字 = \"<&>\";\n\n    return;\n}\n";
        let html = "<p>Before</p><pre><code class=\"extra language-rust\">  fn main() {\n\tlet <span data-easyread-note=\"note\"><mark color=\"#d4e3c9\"><u style=\"text-decoration-color:#477fc1\">名字</u></mark></span> = &quot;&lt;&amp;&gt;&quot;;\n\n    return;\n}\n</code></pre><p>After</p>";
        let doc = super::parse(html, &mut cx).unwrap();
        let BlockNode::Root { children, .. } = &doc.blocks[0] else {
            panic!("root")
        };
        assert_eq!(children.len(), 3);
        let BlockNode::CodeBlock(block) = &children[1] else {
            panic!("code block")
        };
        assert_eq!(block.code().as_ref(), code);
        assert_eq!(block.lang().as_deref(), Some("rust"));
        assert!(block.marks.iter().all(|(_, mark)| !mark.code));
        let (range, mark) = block
            .marks
            .iter()
            .find(|(_, mark)| mark.annotation_id.is_some())
            .unwrap();
        assert_eq!(&code[range.clone()], "名字");
        assert_eq!(mark.annotation_id.as_deref(), Some("note"));
        assert!(mark.highlight.is_some());
        assert_eq!(mark.underline_color, Some(gpui::rgb(0x477fc1).into()));
        assert!(mark.underline);
    }

    #[test]
    fn bare_and_nested_pre_are_code_blocks_without_inline_chips() {
        fn find_code(node: &BlockNode) -> Option<&crate::text::node::CodeBlock> {
            match node {
                BlockNode::CodeBlock(code) => Some(code),
                BlockNode::Root { children, .. } | BlockNode::Blockquote { children, .. } => {
                    children.iter().find_map(find_code)
                }
                _ => None,
            }
        }
        for html in [
            "<pre>  first\n\n\tsecond\n</pre>",
            "<blockquote><pre><code>  first\n\n\tsecond\n</code></pre></blockquote>",
        ] {
            let doc = super::parse(html, &mut NodeContext::default()).unwrap();
            let code = find_code(&doc.blocks[0]).expect("code block");
            assert_eq!(code.code().as_ref(), "  first\n\n\tsecond\n");
            assert_eq!(code.lang(), None);
            assert!(doc.blocks[0].to_markdown().contains("```"));
        }
        let doc = super::parse(
            "<p>Use <code>x &lt; y</code>.</p>",
            &mut NodeContext::default(),
        )
        .unwrap();
        assert_eq!(doc.blocks[0].to_markdown(), "Use `x < y`.");
    }

    #[test]
    fn code_source_foreground_survives_annotation_overlay() {
        let html = "<pre><code><span style=\"color:#000088;font-style:italic\"><span data-easyread-note=\"note\"><mark color=\"#d4e3c9\">中文</mark></span></span>\n</code></pre>";
        let doc = super::parse(html, &mut NodeContext::default()).unwrap();
        let BlockNode::CodeBlock(code) = &doc.blocks[0] else {
            panic!("code block")
        };
        assert_eq!(code.code().as_ref(), "中文\n");
        let (_, mark) = code
            .marks
            .iter()
            .find(|(_, mark)| mark.annotation_id.is_some())
            .unwrap();
        assert_eq!(mark.color, Some(gpui::rgb(0x000088).into()));
        assert!(mark.italic);
        assert_eq!(mark.highlight, Some(gpui::rgb(0xd4e3c9).into()));
        assert!(!mark.code);
    }

    #[test]
    fn pre_keeps_inline_image_between_colored_runs_without_changing_code_offsets() {
        let doc = super::parse(
            "<pre><code>  <span style=\"color:#ff3300\">a </span><img src=\"data:image/png;base64,aGVsbG8=\" alt=\"\"><span style=\"color:#000088\">\"</span>\n\nnext\n</code></pre>",
            &mut NodeContext::default(),
        ).unwrap();
        let BlockNode::CodeBlock(code) = &doc.blocks[0] else {
            panic!("code block")
        };
        assert_eq!(code.code().as_ref(), "  a \"\n\nnext\n");
        let content = code.inline_content.as_ref().expect("inline code content");
        assert_eq!(content.text(), code.code().as_ref());
        assert_eq!(
            content
                .children
                .iter()
                .filter(|c| c.image.is_some())
                .count(),
            1
        );
        let image = content
            .children
            .iter()
            .position(|c| c.image.is_some())
            .unwrap();
        assert!(
            content.children[..image]
                .iter()
                .map(|c| c.text.as_ref())
                .collect::<String>()
                .ends_with("a ")
        );
        assert!(
            content.children[image + 1..]
                .iter()
                .map(|c| c.text.as_ref())
                .collect::<String>()
                .starts_with('"')
        );
        assert!(code.marks.iter().all(|(_, mark)| !mark.code));
        assert!(
            code.marks
                .iter()
                .any(|(range, mark)| &code.code()[range.clone()] == "\"" && mark.color.is_some())
        );
    }

    #[test]
    fn test_cleanup_html() {
        let html = r#"<p>
            and
            <code>code</code>
            text
        </p>"#;
        let cleaned = super::cleanup_html(html);
        assert_eq!(
            String::from_utf8(cleaned).unwrap(),
            "<p>and <code>code</code> text"
        );

        let html = r#"<p>
            and
            <em>   <code>code</code>   <i>italic</i>   </em>
            text
        </p>"#;
        let cleaned = super::cleanup_html(html);
        assert_eq!(
            String::from_utf8(cleaned).unwrap(),
            "<p>and <em><code>code</code> <i>italic</i></em> text"
        );
    }

    #[test]
    fn test_trim_text() {
        assert_eq!(trim_text("  \n\tHello world \t\r "), " Hello world ",);
    }

    #[test]
    fn centered_caption_alignment_stays_on_its_paragraph() {
        let mut cx = NodeContext::default();
        let node = super::parse(
            r#"<p style="text-align:center"><em>Figure 1. Caption</em></p><p>Following body.</p>"#,
            &mut cx,
        )
        .unwrap();
        let BlockNode::Root { children, .. } = &node.blocks[0] else {
            panic!("expected separate caption and body")
        };
        let BlockNode::Paragraph(caption) = children[0].clone().compact() else {
            panic!("expected caption")
        };
        let BlockNode::Paragraph(body) = children[1].clone().compact() else {
            panic!("expected body")
        };
        assert!(caption.centered);
        assert!(!body.centered);
        assert_eq!(node.to_markdown(), "*Figure 1. Caption*\n\nFollowing body.");
    }

    #[test]
    fn definition_terms_keep_code_emphasis_hanging_indent_and_following_prose() {
        let doc=super::parse("<p>Use <strong><code>Tools</code></strong>.</p><dl><dt><strong><code>description</code></strong></dt><dd data-easyread-indent=\"1.5\">A wrapped <a href=\"https://example.org\">description</a>.</dd><dt><strong><code>prompt</code></strong></dt><dd>Its instructions.</dd></dl><p>Following body.</p>",&mut NodeContext::default()).unwrap();
        fn paragraphs<'a>(node: &'a BlockNode, out: &mut Vec<&'a Paragraph>) {
            match node {
                BlockNode::Paragraph(p) => out.push(p),
                BlockNode::Root { children, .. } => {
                    for child in children {
                        paragraphs(child, out);
                    }
                }
                _ => {}
            }
        }
        let mut ps = vec![];
        paragraphs(&doc.blocks[0], &mut ps);
        assert_eq!(ps.len(), 6);
        assert_eq!(
            ps.iter().map(|p| p.indent_em).collect::<Vec<_>>(),
            vec![0., 0., 1.5, 0., 1.5, 0.]
        );
        assert!(
            ps[1]
                .children
                .iter()
                .flat_map(|n| n.marks.iter())
                .any(|(_, m)| m.code && m.bold)
        );
        assert!(
            ps[2]
                .children
                .iter()
                .flat_map(|n| n.marks.iter())
                .any(|(_, m)| m.link.is_some())
        );
        assert!(doc.blocks[0].to_markdown().contains("**`description`**"));
        for value in ["NaN", "-1", "100"] {
            let doc = super::parse(
                &format!("<dd data-easyread-indent=\"{value}\">Body</dd>"),
                &mut NodeContext::default(),
            )
            .unwrap();
            let mut ps = vec![];
            paragraphs(&doc.blocks[0], &mut ps);
            assert_eq!(ps[0].indent_em, 1.5);
        }
    }

    #[test]
    fn test_mark() {
        let mut cx = NodeContext::default();

        // `<mark>` is rendered as a highlight, kept as `==...==` in markdown.
        let html = r#"<p>Hello <mark>world</mark></p>"#;
        let node = super::parse(html, &mut cx).unwrap();
        assert_eq!(node.to_markdown(), "Hello ==world==");

        let html = r#"<p><mark color="blue">blue</mark> and <mark style="background-color: #336699">hex</mark></p>"#;
        let node = super::parse(html, &mut cx).unwrap();
        assert_eq!(node.to_markdown(), "==blue== and ==hex==");
    }

    #[test]
    fn native_math_retains_saved_marks_inside_and_outside_its_span() {
        let formula = r##"<span data-vellio-math="d_k" data-display="false"><span data-easyread-note="formula-note"><mark color="#f2df8a"><u style="text-decoration-color:#477fc1">$d_k$</u></mark></span></span>"##;
        for html in [
            format!("<p>{formula}</p>"),
            format!("<p><strong>{formula}</strong></p>"),
        ] {
            let doc = super::parse(&html, &mut NodeContext::default()).unwrap();
            let BlockNode::Paragraph(p) = &doc.blocks[0] else {
                panic!("expected paragraph")
            };
            assert_eq!(p.children.len(), 1);
            let node = &p.children[0];
            assert_eq!(node.custom.as_ref().unwrap().as_text(), "$d_k$");
            assert!(node.marks.iter().all(|(range, _)| range == &(0..5)));
            assert!(
                node.marks
                    .iter()
                    .any(|(_, mark)| mark.highlight == Some(gpui::rgb(0xf2df8a).into()))
            );
            assert!(node.marks.iter().any(|(_, mark)| mark.underline
                && mark.underline_color == Some(gpui::rgb(0x477fc1).into())));
            assert!(
                node.marks
                    .iter()
                    .any(|(_, mark)| mark.annotation_id.as_deref() == Some("formula-note"))
            );
            assert_eq!(
                node.marks.iter().any(|(_, mark)| mark.bold),
                html.contains("<strong>")
            );
        }
    }

    #[test]
    fn easyread_annotation_identity_survives_nested_document_links() {
        let mut cx = NodeContext::default();
        let node = super::parse(r##"<p><span data-easyread-note="n1"><mark color="#d4e3c9"><strong>one</strong> <a href="https://example.org">two</a></mark></span> plain</p>"##, &mut cx).unwrap();
        let BlockNode::Paragraph(paragraph) = &node.blocks[0] else {
            panic!("expected paragraph")
        };
        assert!(
            paragraph
                .children
                .iter()
                .flat_map(|n| &n.marks)
                .any(|(_, m)| m.annotation_id.as_deref() == Some("n1"))
        );
        assert!(
            paragraph
                .children
                .iter()
                .flat_map(|n| &n.marks)
                .any(|(_, m)| m
                    .link
                    .as_ref()
                    .is_some_and(|l| l.url == "https://example.org"))
        );
        assert_eq!(node.text().trim_end(), "one two plain");
    }

    #[test]
    fn linked_superscript_keeps_marks_text_and_selection_source() {
        let node = super::parse(
            r#"<p>(ViViT). <a href="https://example.org"><sup><u>8</u></sup></a> This method.</p>"#,
            &mut NodeContext::default(),
        )
        .unwrap();
        let BlockNode::Paragraph(paragraph) = &node.blocks[0] else {
            panic!("paragraph")
        };
        assert_eq!(node.text().trim_end(), "(ViViT). 8 This method.");
        let mark = paragraph
            .children
            .iter()
            .flat_map(|n| &n.marks)
            .find(|(_, m)| m.superscript)
            .unwrap();
        assert!(mark.1.underline);
        assert_eq!(mark.1.link.as_ref().unwrap().url, "https://example.org");
        let source: String = paragraph
            .children
            .iter()
            .map(|n| crate::text::node::reconstruct_markdown(&n.text, &n.marks, 0..n.text.len()))
            .collect();
        assert!(source.contains("<sup>8</sup>"), "{source}");
    }

    #[test]
    fn colored_underlines_preserve_text_and_source_decoration() {
        let html = r#"<p>before <u style="text-decoration-color: #477fc1"><strong>blue</strong> text</u> after <u>plain</u></p>"#;
        let mut cx = NodeContext::default();
        let node = super::parse(html, &mut cx).unwrap();
        assert_eq!(node.text().trim_end(), "before blue text after plain");
        let BlockNode::Paragraph(paragraph) = &node.blocks[0] else {
            panic!("expected paragraph")
        };
        let source: String = paragraph
            .children
            .iter()
            .map(|inline| {
                crate::text::node::reconstruct_markdown(
                    &inline.text,
                    &inline.marks,
                    0..inline.text.len(),
                )
            })
            .collect();
        assert!(source.contains("text-decoration-color:#477fc1"));
        assert!(source.contains("<u>plain</u>"));
    }

    #[test]
    fn nested_links_keep_boundary_spaces_and_exact_label_ranges() {
        for html in [
            r#"<p><span>Key from the <a href="https://example.org"><span>website</span></a>. We then create</span></p>"#,
            r#"<p><span>Key from the<a href="https://example.org"><span> website</span></a>. We then create</span></p>"#,
            r#"<p><span>Key from the<a href="https://example.org"><span> website</span></a><span>. We then create</span></span></p>"#,
        ] {
            let parsed = super::parse(html, &mut NodeContext::default()).unwrap();
            assert_eq!(
                parsed.text().trim_end(),
                "Key from the website. We then create"
            );
        }
        let html = r#"<p><span>Key from the <a href="https://example.org"><span>website</span></a> <em>and docs</em>.</span></p>"#;
        let parsed = super::parse(html, &mut NodeContext::default()).unwrap();
        assert_eq!(parsed.text().trim_end(), "Key from the website and docs.");
        let BlockNode::Paragraph(paragraph) = &parsed.blocks[0] else {
            panic!("expected paragraph")
        };
        let labels: Vec<_> = paragraph
            .children
            .iter()
            .flat_map(|node| {
                node.marks
                    .iter()
                    .filter(|(_, mark)| mark.link.is_some())
                    .map(|(range, _)| node.text[range.clone()].to_string())
            })
            .collect();
        assert_eq!(labels, ["website"]);
    }

    #[test]
    fn link_marks_exclude_edge_spaces_and_keep_internal_spaces() {
        for html in [
            r#"<p>the<a href="https://example.org"><span> web &amp; docs </span></a>next</p>"#,
            r#"<p>the<a href="https://example.org"> <em>web &amp; docs</em> </a>next</p>"#,
        ] {
            let parsed = super::parse(html, &mut NodeContext::default()).unwrap();
            assert_eq!(parsed.text().trim_end(), "the web & docs next");
            let BlockNode::Paragraph(paragraph) = &parsed.blocks[0] else {
                panic!("expected paragraph")
            };
            let labels: Vec<_> = paragraph
                .children
                .iter()
                .flat_map(|node| {
                    node.marks
                        .iter()
                        .filter(|(_, mark)| mark.link.is_some())
                        .map(|(range, _)| node.text[range.clone()].to_string())
                })
                .collect();
            assert_eq!(labels, ["web & docs"]);
        }
    }

    #[test]
    fn test_keep_spaces() {
        let html = r#"<p>and <code>code</code> text</p>"#;
        let mut cx = NodeContext::default();
        let node = super::parse(html, &mut cx).unwrap();
        assert_eq!(node.to_markdown(), "and `code` text");

        let html = r#"
            <div>
            <p>
                and
                <em>   <code>code</code>   <i>italic</i>   </em>
                text
            </p>
            <p>
                <img src="https://example.com/image.png" alt="Example" width="100" height="200" title="Example Image" />
            </p>
            <ul>
                <li>Item 1</li>
                <li>Item 2
                </li>
            </ul>
            </div>
        "#;
        let node = super::parse(html, &mut cx).unwrap();
        assert_eq!(
            node.to_markdown(),
            indoc::indoc! {r#"
            and *`code` italic* text

            ![Example](https://example.com/image.png "Example Image")

            - Item 1
            - Item 2
            "#}
            .trim()
        );
    }

    #[test]
    fn test_value_to_length() {
        assert_eq!(super::value_to_length("100px"), Some(px(100.).into()));
        assert_eq!(super::value_to_length("100%"), Some(relative(1.)));
        assert_eq!(super::value_to_length("56%"), Some(relative(0.56)));
        assert_eq!(super::value_to_length("240"), Some(px(240.).into()));
    }

    #[test]
    fn test_image() {
        let html = r#"<img src="https://example.com/image.png" alt="Example" width="100" height="200" title="Example Image" />"#;
        let mut cx = NodeContext::default();
        let node = super::parse(html, &mut cx).unwrap();
        assert_eq!(
            node,
            ParsedDocument {
                source: html.to_string().into(),
                blocks: Arc::new(vec![BlockNode::Paragraph(Paragraph {
                    span: None,
                    children: vec![InlineNode::image(ImageNode {
                        url: "https://example.com/image.png".to_string().into(),
                        alt: Some("Example".to_string().into()),
                        width: Some(px(100.).into()),
                        height: Some(px(200.).into()),
                        title: Some("Example Image".to_string().into()),
                        ..Default::default()
                    })],
                    ..Default::default()
                })])
            }
        );

        let html = r#"<img src="https://example.com/image.png" alt="Example" style="width: 80%" title="Example Image" />"#;
        let node = super::parse(html, &mut cx).unwrap();
        assert_eq!(
            node,
            ParsedDocument {
                source: html.to_string().into(),
                blocks: Arc::new(vec![BlockNode::Paragraph(Paragraph {
                    span: None,
                    children: vec![InlineNode::image(ImageNode {
                        url: "https://example.com/image.png".to_string().into(),
                        alt: Some("Example".to_string().into()),
                        width: Some(relative(0.8)),
                        height: None,
                        title: Some("Example Image".to_string().into()),
                        ..Default::default()
                    })],
                    ..Default::default()
                })])
            }
        );
    }
}
