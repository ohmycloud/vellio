//! Shared, cached syntax definitions for native book and chat code blocks.
use gpui_kit::{FontStyle, FontWeight, HighlightStyle, base::text::CodeBlock, rgb};
use std::{ops::Range, sync::LazyLock};
use syntect::{
    easy::HighlightLines,
    highlighting::{FontStyle as SyntaxFontStyle, ThemeSet},
    parsing::SyntaxSet,
    util::LinesWithEndings,
};

static SYNTAXES: LazyLock<SyntaxSet> = LazyLock::new(SyntaxSet::load_defaults_newlines);
static THEMES: LazyLock<ThemeSet> = LazyLock::new(ThemeSet::load_defaults);

// EPUBs frequently omit language metadata. Guess only distinctive syntax;
// prose, output, pseudocode and an explicit unknown language remain plain text.
fn inferred_language(code: &str) -> Option<&'static str> {
    let source = code.trim();
    if source.starts_with("Traceback (most recent call last)") {
        return None;
    }
    if (source.starts_with('{') || source.starts_with('['))
        && serde_json::from_str::<serde_json::Value>(source).is_ok()
    {
        return Some("json");
    }
    let lines: Vec<_> = source.lines().map(str::trim).collect();
    if lines.iter().any(|line| {
        let colon = line.ends_with(':')
            || line
                .rsplit_once('#')
                .is_some_and(|(code, _)| code.trim_end().ends_with(':'));
        (line.starts_with("def ") || line.starts_with("async def ") || line.starts_with("class "))
            && colon
            || line.starts_with("from ") && line.contains(" import ")
            || line.starts_with("import ") && !line.contains([';', '{']) && !line.contains(" from ")
            || (line.starts_with("with ")
                || line.starts_with("for ")
                || line.starts_with("if ")
                || line.starts_with("elif ")
                || line.starts_with("except "))
                && colon
            || line.starts_with("print(") && !line.ends_with(';')
    }) {
        return Some("python");
    }
    if lines.iter().any(|line| {
        line.starts_with("fn ")
            || line.starts_with("pub fn ")
            || line.starts_with("use ") && line.contains("::")
    }) {
        return Some("rust");
    }
    if lines.iter().any(|line| {
        line.starts_with("function ")
            || line.starts_with("export default ")
            || line.starts_with("const ") && line.contains("=>")
    }) {
        return Some("javascript");
    }
    if lines.iter().any(|line| {
        [
            "#!/bin/bash",
            "#!/bin/sh",
            "#!/usr/bin/env bash",
            "echo ",
            "export ",
            "pip install ",
            "pip3 install ",
            "npm install ",
            "cargo ",
        ]
        .iter()
        .any(|prefix| line.starts_with(prefix))
            || line.contains("=$(")
    }) {
        return Some("bash");
    }
    None
}

/// Use a document hint only when several distinctive examples overwhelmingly
/// agree. No filename/title-based rules, and no changes to persisted source.
pub(super) fn preferred_epub_language(
    blocks: &[easyread_gpui::model::Block],
) -> Option<&'static str> {
    let mut python = 0;
    let mut known = 0;
    for block in blocks.iter().filter(|b| b.source_kind == "epub-xhtml") {
        let source = block.en.trim();
        if !source.starts_with("```") {
            continue;
        }
        let Some((fence, body)) = source.split_once('\n') else {
            continue;
        };
        let Some((body, _)) = body.rsplit_once('\n') else {
            continue;
        };
        let declared = fence.trim_start_matches('`').trim();
        if !declared.is_empty() {
            known += 1;
            python += usize::from(matches!(
                declared.to_ascii_lowercase().as_str(),
                "python" | "python3" | "py"
            ));
        } else if let Some(lang) = inferred_language(body) {
            known += 1;
            python += usize::from(lang == "python");
        }
    }
    (python >= 3 && python * 5 >= known * 4).then_some("python")
}

fn identifier(value: &str) -> bool {
    let mut chars = value.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

// Weak syntax (assignments and ordinary calls) is accepted only with the
// document hint. Literal results, tensor reprs, logs and prose remain plain.
fn python_fragment(source: &str) -> bool {
    if [
        "tensor(",
        "array(",
        "torch.Size(",
        "OrderedDict(",
        "Traceback (most recent call last)",
    ]
    .iter()
    .any(|prefix| source.trim_start().starts_with(prefix))
    {
        return false;
    }
    source
        .lines()
        .map(str::trim)
        .filter(|line| !line.starts_with('#'))
        .any(|line| {
            if line.ends_with(';') {
                return false;
            }
            if let Some((left, right)) = line.split_once('=')
                && !right.starts_with('=')
                && !left.ends_with(['=', '!', '<', '>', ':'])
                && left
                    .split(',')
                    .all(|name| name.trim().split('.').all(identifier))
            {
                return true;
            }
            if let Some((head, _)) = line.split_once('(') {
                return !matches!(head, "tensor" | "array" | "torch.Size" | "OrderedDict")
                    && head.chars().next().is_some_and(char::is_lowercase)
                    && head.split('.').all(identifier);
            }
            false
        })
}

/// Add a display-only language hint to unlabeled code in Python-heavy EPUBs.
/// Keep all source text/links/marks intact and honor explicit unknown languages.
pub(super) fn epub_html(source: String, language: Option<&str>) -> String {
    if language != Some("python") || !source.contains("<pre") {
        return source;
    }
    let wrapped = format!("<root>{source}</root>");
    let Ok(doc) = roxmltree::Document::parse(&wrapped) else {
        return source;
    };
    let mut replacements = Vec::new();
    for pre in doc
        .descendants()
        .filter(|n| n.has_tag_name("pre") && !n.ancestors().skip(1).any(|a| a.has_tag_name("pre")))
    {
        let explicit = pre.descendants().any(|node| {
            ["data-code-language", "data-language", "data-lang"]
                .iter()
                .any(|attr| node.attribute(*attr).is_some())
                || node.attribute("class").is_some_and(|classes| {
                    classes
                        .split_whitespace()
                        .any(|class| class.starts_with("language-") || class.starts_with("lang-"))
                })
        });
        if explicit {
            continue;
        }
        let text = pre
            .descendants()
            .filter(|n| n.is_text())
            .filter_map(|n| n.text())
            .collect::<String>();
        if inferred_language(&text).is_some() || !python_fragment(&text) {
            continue;
        }
        let range = pre.range();
        let html = &wrapped[range.clone()];
        let Some(start) = html.find('>') else {
            continue;
        };
        let Some(end) = html.rfind("</pre>") else {
            continue;
        };
        replacements.push((
            range.start - 6..range.end - 6,
            format!(
                "{}<code class=\"language-python\">{}</code></pre>",
                &html[..start + 1],
                &html[start + 1..end]
            ),
        ));
    }
    let mut result = source;
    for (range, replacement) in replacements.into_iter().rev() {
        result.replace_range(range, &replacement);
    }
    result
}

pub fn highlight(block: &CodeBlock) -> Vec<(Range<usize>, HighlightStyle)> {
    let code = block.code();
    let declared = block.lang();
    let lang = declared
        .as_deref()
        .filter(|lang| !lang.trim().is_empty())
        .map(str::to_ascii_lowercase)
        .or_else(|| inferred_language(&code).map(str::to_owned))
        .unwrap_or_default();
    let lang = match lang.as_str() {
        "sh" | "shell" | "console" => "bash",
        "py" | "python3" => "python",
        "js" | "node" => "javascript",
        "rs" => "rust",
        "c++" => "cpp",
        "c#" => "cs",
        other => other,
    };
    let syntax = SYNTAXES
        .find_syntax_by_token(lang)
        .or_else(|| {
            (lang.is_empty())
                .then(|| SYNTAXES.find_syntax_by_first_line(&code))
                .flatten()
        })
        .unwrap_or_else(|| SYNTAXES.find_syntax_plain_text());
    let mut highlighter = HighlightLines::new(syntax, &THEMES.themes["InspiredGitHub"]);
    let mut highlights = Vec::new();
    let mut offset = 0;
    for line in LinesWithEndings::from(&code) {
        let Ok(runs) = highlighter.highlight_line(line, &SYNTAXES) else {
            offset += line.len();
            continue;
        };
        for (style, text) in runs {
            let color = style.foreground;
            highlights.push((
                offset..offset + text.len(),
                HighlightStyle {
                    color: Some(
                        rgb((u32::from(color.r) << 16)
                            | (u32::from(color.g) << 8)
                            | u32::from(color.b))
                        .into(),
                    ),
                    font_weight: style
                        .font_style
                        .contains(SyntaxFontStyle::BOLD)
                        .then_some(FontWeight::BOLD),
                    font_style: style
                        .font_style
                        .contains(SyntaxFontStyle::ITALIC)
                        .then_some(FontStyle::Italic),
                    ..Default::default()
                },
            ));
            offset += text.len();
        }
    }
    highlights
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_highlights_keep_utf8_offsets_and_multiline_state() {
        for (lang, code) in [
            (
                "rust",
                "fn main() {\n    let 名字 = \"中文\"; // comment\n}\n",
            ),
            ("python", "def hello():\n    return \"中文\" # comment\n"),
            ("json", "{\"title\": \"中文\", \"count\": 42}\n"),
            ("cpp", "/* first\n   second */\nint main() { return 42; }\n"),
        ] {
            let runs = highlight(&CodeBlock::from_code(code, Some(lang)));
            assert_eq!(runs.first().unwrap().0.start, 0);
            assert_eq!(runs.last().unwrap().0.end, code.len());
            assert!(runs.windows(2).all(|pair| pair[0].0.end == pair[1].0.start));
            assert!(
                runs.iter()
                    .all(|(range, _)| code.get(range.clone()).is_some())
            );
            assert!(runs.iter().any(|(_, style)| style.color != runs[0].1.color));
        }
    }

    #[test]
    fn unlabeled_epub_code_gets_conservative_syntax_colors_and_preserves_offsets() {
        for (lang, source) in [
            (
                "python",
                "from datetime import date\n\ndef today():\n    return \"中文\" # comment\n",
            ),
            ("rust", "fn main() {\n    println!(\"中文\");\n}\n"),
            ("json", "{\"name\": \"中文\", \"count\": 42}\n"),
            ("bash", "echo \"$HOME\"\n"),
            ("javascript", "function hello() { return \"中文\"; }\n"),
        ] {
            assert_eq!(inferred_language(source), Some(lang));
            let runs = highlight(&CodeBlock::from_code(source, None::<&str>));
            assert_eq!(runs.first().unwrap().0.start, 0);
            assert_eq!(runs.last().unwrap().0.end, source.len());
            assert!(
                runs.iter()
                    .all(|(range, _)| source.get(range.clone()).is_some())
            );
            assert!(
                runs.iter().any(|(_, style)| style.color != runs[0].1.color),
                "{lang}"
            );
        }
        for source in [
            "This is sample output.\n",
            "42\n",
            "input -> reasoning -> output\n",
            "class Example {}",
        ] {
            assert_eq!(inferred_language(source), None);
        }
        let runs = highlight(&CodeBlock::from_code(
            "def hello():\n    return 42\n",
            Some("unknown-language"),
        ));
        assert!(runs.iter().all(|(_, style)| style.color == runs[0].1.color));
    }

    #[test]
    fn python_short_statements_and_epub_document_hints_do_not_color_output_as_code() {
        for source in [
            "with open(\"book.txt\") as f:\n    text = f.read()\n",
            "print(len(text))\n",
            "for item in tokens:\n    print(item)\n",
            "class Example: #1\n    def run(self): #2\n        return 42\n",
        ] {
            assert_eq!(inferred_language(source), Some("python"));
            let runs = highlight(&CodeBlock::from_code(source, None::<&str>));
            assert!(runs.iter().any(|(_, style)| style.color != runs[0].1.color));
        }
        let seed = |source: &str| {
            serde_json::from_value::<easyread_gpui::model::Block>(serde_json::json!({"id":"seed","type":"para","source_kind":"epub-xhtml","en":format!("```\n{source}\n```")})).unwrap()
        };
        let mut blocks = vec![
            seed("import re"),
            seed("def run():\n    return 42"),
            seed("class Example:\n    pass"),
        ];
        assert_eq!(preferred_epub_language(&blocks), Some("python"));
        blocks.extend([seed("fn main() {}"), seed("function hello() {}")]);
        assert_eq!(preferred_epub_language(&blocks), None);
        blocks.truncate(3);
        let mut declared = seed("import math");
        declared.en = "```unknown-language\nimport math\n```".to_owned();
        blocks.extend([declared.clone(), declared]);
        assert_eq!(
            preferred_epub_language(&blocks),
            None,
            "explicit metadata wins in document voting too"
        );
        for source in [
            "result = re.split(r'(\\s)', text)\n",
            "torch.manual_seed(123)\n",
            "result = [token for token in tokens if token.strip()]\n",
        ] {
            let html = format!(
                "<pre><code>{}</code></pre>",
                source
                    .replace('&', "&amp;")
                    .replace('<', "&lt;")
                    .replace('>', "&gt;")
                    .replace('"', "&quot;")
            );
            let updated = epub_html(html.clone(), Some("python"));
            assert!(updated.contains("language-python"));
            assert_eq!(
                easyread_gpui::annotations::plain_text(&updated),
                easyread_gpui::annotations::plain_text(&html)
            );
            assert_eq!(epub_html(html.clone(), None), html);
        }
        let marked = "<pre id=\"anchor\"><code><b>total</b> = <a href=\"part.xhtml\">len(values)</a>\n</code></pre>";
        let updated = epub_html(marked.to_owned(), Some("python"));
        assert!(updated.contains("language-python"));
        assert!(updated.contains("<pre id=\"anchor\">"));
        assert!(updated.contains("<b>total</b> = <a href=\"part.xhtml\">len(values)</a>"));
        assert_eq!(
            easyread_gpui::annotations::plain_text(&updated),
            easyread_gpui::annotations::plain_text(marked)
        );
        assert_eq!(
            inferred_language(
                "Traceback (most recent call last):\n  print(text)\nValueError: input error"
            ),
            None
        );
        for html in [
            "<pre><code>tensor([[0.2, 0.3]])</code></pre>",
            "<pre><code>tensor([[0.2, 0.3]],\n       grad_fn=&lt;AddBackward0&gt;)</code></pre>",
            "<pre><code>Traceback (most recent call last):\n  print(text)\nValueError: input error</code></pre>",
            "<pre><code>['Hello', 'world']</code></pre>",
            "<pre><code>Total characters: 20</code></pre>",
            "<pre><code class=\"language-unknown\">value = 42</code></pre>",
            "<p><span>value = 42</span></p>",
        ] {
            assert_eq!(epub_html(html.to_owned(), Some("python")), html);
        }
    }

    #[test]
    fn unknown_language_stays_readable_and_shell_alias_is_highlighted() {
        let code = "  literal $variable\n\t中文 <tag>\n";
        for lang in [None, Some("unknown-language")] {
            let runs = highlight(&CodeBlock::from_code(code, lang));
            assert_eq!(runs.last().unwrap().0.end, code.len());
        }
        assert!(!highlight(&CodeBlock::from_code("echo \"$HOME\"\n", Some("shell"))).is_empty());
    }
}
