//! Allowlisted XHTML, source Markdown and figure-caption recognition.
use super::*;

pub(super) fn text(node: Node<'_, '_>) -> String {
    node.descendants()
        .filter(|n| n.is_text())
        .filter_map(|n| n.text())
        .collect::<String>()
}
pub(super) fn compact(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}
pub(super) fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
pub(super) fn token(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))[..20].to_owned()
}
pub(super) fn figure_caption<'a, 'input>(node: Node<'a, 'input>) -> Option<Node<'a, 'input>> {
    node.ancestors()
        .find(|n| n.has_tag_name("figure"))
        .and_then(|figure| {
            figure
                .descendants()
                .find(|n| n.has_tag_name("figcaption"))
                .or_else(|| figure.descendants().find(|n| n.has_tag_name("h6")))
        })
}
pub(super) fn numbered_figure_label(text: &str) -> bool {
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

pub(super) fn publisher_figure_caption(node: Node<'_, '_>) -> bool {
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

pub(super) fn caption_html(html: String) -> String {
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

pub(super) fn link(href: &str, source: &str) -> Option<String> {
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
pub(super) fn code_language(node: Node<'_, '_>) -> Option<String> {
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
pub(super) fn render(node: Node<'_, '_>, href: &str) -> String {
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
pub(super) fn markdown(node: Node<'_, '_>, pre: bool) -> String {
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
