//! Shared light-theme syntax colors for native and Web rich text.
use std::{ops::Range, sync::LazyLock};
use syntect::{
    easy::HighlightLines,
    highlighting::{Style, ThemeSet},
    parsing::SyntaxSet,
    util::LinesWithEndings,
};

static SYNTAXES: LazyLock<SyntaxSet> = LazyLock::new(SyntaxSet::load_defaults_newlines);
static THEMES: LazyLock<ThemeSet> = LazyLock::new(ThemeSet::load_defaults);

/// Guess only distinctive syntax, never ordinary prose, output or assignments.
pub fn inferred_language(code: &str) -> Option<&'static str> {
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
    // Short notes may contain only a call with keyword arguments. Require a
    // call opener and an indented literal keyword argument, not just x = y.
    if !source.contains(';')
        && !["tensor(", "array(", "torch.Size(", "OrderedDict("]
            .iter()
            .any(|prefix| source.starts_with(prefix))
        && lines.iter().any(|line| {
            line.strip_suffix('(')
                .is_some_and(|head| head.split('.').all(identifier))
        })
        && source.lines().any(|line| {
            line.starts_with(char::is_whitespace)
                && line.trim().split_once('=').is_some_and(|(name, value)| {
                    let value = value.trim().trim_end_matches(',');
                    identifier(name.trim())
                        && (value.starts_with(['\'', '"'])
                            || matches!(value, "True" | "False" | "None"))
                })
        })
    {
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

fn identifier(value: &str) -> bool {
    let mut chars = value.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// UTF-8 byte ranges in the unchanged code, retaining multiline lexer state.
/// An explicit unknown/plain-text language always wins over inference.
pub fn highlight(code: &str, declared: Option<&str>) -> Vec<(Range<usize>, Style)> {
    let lang = declared
        .filter(|lang| !lang.trim().is_empty())
        .map(|lang| lang.trim().to_ascii_lowercase())
        .or_else(|| inferred_language(code).map(str::to_owned))
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
            lang.is_empty()
                .then(|| SYNTAXES.find_syntax_by_first_line(code))
                .flatten()
        })
        .unwrap_or_else(|| SYNTAXES.find_syntax_plain_text());
    let mut highlighter = HighlightLines::new(syntax, &THEMES.themes["InspiredGitHub"]);
    let mut highlights = Vec::new();
    let mut offset = 0;
    for line in LinesWithEndings::from(code) {
        let Ok(runs) = highlighter.highlight_line(line, &SYNTAXES) else {
            offset += line.len();
            continue;
        };
        for (style, text) in runs {
            highlights.push((offset..offset + text.len(), style));
            offset += text.len();
        }
    }
    highlights
}
