//! Application-owned syntax colors, inserted only after Markdown sanitization.
use easyread_gpui::{code_syntax, formula};

pub fn render(source: &str) -> String {
    let mut html = super::cleaned(&formula::native_markdown_html(source));
    let mut replacements = Vec::new();
    // Parse each compiler-generated pre independently: unrelated HTML such as
    // <br> or <img> need not be XML-compatible. Never enable user-supplied CSS.
    for (start, _) in html.match_indices("<pre>") {
        let Some(end) = html[start..].find("</pre>") else {
            continue;
        };
        let fragment = &html[start..start + end + "</pre>".len()];
        let Ok(doc) = roxmltree::Document::parse(fragment) else {
            continue;
        };
        let Some(code) = doc
            .root_element()
            .children()
            .find(|node| node.has_tag_name("code"))
        else {
            continue;
        };
        let language = code.attribute("class").and_then(|classes| {
            classes
                .split_whitespace()
                .find_map(|class| class.strip_prefix("language-"))
        });
        let text = code
            .children()
            .filter_map(|node| node.text())
            .collect::<String>();
        let runs = code_syntax::highlight(&text, language);
        if !runs.first().is_some_and(|first| {
            runs.iter()
                .any(|(_, style)| style.foreground != first.1.foreground)
        }) {
            continue;
        }
        let mut colored = String::new();
        let mut cursor = 0;
        for (range, style) in runs {
            // Retain any uncolored text if the lexer cannot parse a line.
            colored.push_str(&escape(&text[cursor..range.start]));
            let color = style.foreground;
            colored.push_str(&format!(
                "<span style=\"color:#{:02x}{:02x}{:02x}\">{}</span>",
                color.r,
                color.g,
                color.b,
                escape(&text[range.clone()])
            ));
            cursor = range.end;
        }
        colored.push_str(&escape(&text[cursor..]));
        let range = code.range();
        let Some(open_end) = fragment[range.clone()].find('>') else {
            continue;
        };
        replacements.push((
            start + range.start + open_end + 1..start + range.end - "</code>".len(),
            colored,
        ));
    }
    for (range, colored) in replacements.into_iter().rev() {
        html.replace_range(range, &colored);
    }
    html
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn note_fences_keep_source_and_get_multiple_syntax_colors() {
        for (language, code) in [
            ("python", "# 注释\ndef multiply(a, b):\n    return a * b\n"),
            ("js", "function hello() { return \"中文 <&>\"; }\n"),
            ("rust", "fn main() { let count = 42; }\n"),
            ("json", "{\"name\": \"中文\", \"count\": 42}\n"),
            (
                "",
                "# Register tool\ntools = Tools()\ntools.add_tool(\n    name=\"multiply\",\n    func=multiply,\n    description=\"两个数相乘\"\n)\n",
            ),
        ] {
            let source = format!("Before `inline_code`.\n\n```{language}\n{code}```\n\nAfter.");
            let html = render(&source);
            let wrapped = format!("<root>{html}</root>");
            let doc = roxmltree::Document::parse(&wrapped).unwrap();
            let pre = doc
                .descendants()
                .find(|node| node.has_tag_name("pre"))
                .unwrap();
            let restored = pre
                .descendants()
                .filter(|node| node.is_text())
                .filter_map(|node| node.text())
                .collect::<String>();
            assert_eq!(restored, code, "{language}");
            let colors: HashSet<_> = pre
                .descendants()
                .filter_map(|node| node.attribute("style"))
                .collect();
            assert!(colors.len() >= 2, "{language}: {html}");
            assert!(html.contains("<code>inline_code</code>"));
            assert!(html.contains("After."));
        }
    }

    #[test]
    fn plain_unknown_and_unrecognized_blocks_stay_plain() {
        for source in [
            "```text\ndef hello():\n    return 42\n```",
            "```unknown-language\nprint(42)\n```",
            "```\nSample output: 42\n```",
            "Only `inline_code` and normal prose.",
        ] {
            assert_eq!(
                render(source),
                super::super::cleaned(&formula::native_markdown_html(source))
            );
        }
    }

    #[test]
    fn multiple_nested_and_tilde_fences_survive_non_xml_html() {
        let source = "Before.\n\n---\n\n> ~~~python\n> print(\"first\")\n> ~~~\n\n```json\n{\"count\": 42}\n```\n\nAfter.";
        let html = render(source);
        assert_eq!(html.matches("<pre>").count(), 2);
        assert!(html.contains("<blockquote>"));
        assert!(html.contains("<hr>"));
        assert!(html.matches("style=\"color:#").count() >= 4);
    }

    #[test]
    fn code_is_escaped_and_untrusted_html_or_css_is_not_enabled() {
        let source = "<script>alert(1)</script>\n\n<span style=\"color:red\" onclick=\"alert(1)\">untrusted</span>\n\n```python\nprint(\"</code><script>alert('x')</script> & 中文\")\n```";
        let html = render(source);
        assert!(!html.contains("<script"));
        let wrapped = format!("<root>{html}</root>");
        let doc = roxmltree::Document::parse(&wrapped).unwrap();
        for node in doc.descendants().filter(|node| node.is_element()) {
            assert!(node.attribute("onclick").is_none());
            if let Some(style) = node.attribute("style") {
                assert!(style.starts_with("color:#"));
                assert_eq!(style.len(), "color:#000000".len());
                assert!(style[7..].chars().all(|ch| ch.is_ascii_hexdigit()));
            }
        }
        assert!(html.contains("&lt;/code&gt;&lt;script&gt;"));
        assert!(html.contains("&amp; 中文"));
    }
}
