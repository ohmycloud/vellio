//! Translate prose inside a recovered document skeleton, never its layout or TeX.
//! Legacy repair is display-only; stored text, IDs and reader anchors stay intact.
use anyhow::{Context, Result, ensure};
use roxmltree::{Document, Node};
use serde_json::{Value, json};
use std::{collections::BTreeMap, ops::Range};

const END: &str = "[[VELLIO_END]]";

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

// The application emits HTML void tags. Normalize only those tags for the
// read-only XML walker; retain all source attributes and text/entity offsets.
pub(crate) fn xhtml(html: &str) -> String {
    let mut out = String::new();
    let mut rest = html;
    while let Some(start) = rest.find('<') {
        out.push_str(&rest[..start]);
        rest = &rest[start..];
        let Some(end) = rest.find('>') else { break };
        let tag = &rest[..end + 1];
        if ["<img ", "<br>", "<br ", "<hr>", "<hr "]
            .iter()
            .any(|p| tag.starts_with(p))
            && !tag.ends_with("/>")
        {
            out.push_str(tag.trim_end_matches('>'));
            out.push_str("/>");
        } else {
            out.push_str(tag);
        }
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    out.replace("&nbsp;", "&#160;")
}

fn container(node: Node<'_, '_>) -> bool {
    matches!(
        node.tag_name().name(),
        "p" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "li" | "td" | "th"
    )
}
/// A display-math row (including its equation label) is shared source content,
/// not prose to translate. Inline math and prose surrounding a formula remain
/// part of their containing language paragraph.
pub(crate) fn is_display_equation(node: Node<'_, '_>) -> bool {
    let math: Vec<_> = node
        .descendants()
        .filter(|n| n.attribute("data-vellio-math").is_some())
        .collect();
    if math.is_empty()
        || math
            .iter()
            .any(|n| n.attribute("data-display") != Some("true"))
    {
        return false;
    }
    let outside = node
        .descendants()
        .filter(|n| {
            n.is_text()
                && !n
                    .ancestors()
                    .any(|a| a.attribute("data-vellio-math").is_some())
        })
        .filter_map(|n| n.text())
        .collect::<String>();
    let outside = outside.trim();
    if outside.is_empty() {
        return true;
    }
    let label = [('(', ')'), ('[', ']'), ('（', '）')]
        .into_iter()
        .find_map(|(left, right)| outside.strip_prefix(left)?.strip_suffix(right));
    label.is_some_and(|label| {
        !label.is_empty()
            && label
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+' | '*'))
    })
}

fn role(node: Node<'_, '_>) -> &'static str {
    if node.tag_name().name().starts_with('h') {
        "heading"
    } else if is_display_equation(node) {
        "equation"
    } else if node
        .attribute("style")
        .is_some_and(|s| s.replace(' ', "").contains("text-align:center"))
    {
        "caption"
    } else {
        "paragraph"
    }
}

#[derive(Debug)]
struct Unit {
    marker: String,
    range: Range<usize>,
    text: String,
    role: &'static str,
    keep: Vec<(String, String)>,
}
#[derive(Debug)]
pub struct Plan {
    html: String,
    units: Vec<Unit>,
}
#[derive(Debug)]
pub struct Translated {
    pub markdown: String,
    pub html: String,
}
impl Plan {
    pub fn from_html(html: &str) -> Result<Self> {
        let html = xhtml(html);
        let wrapped = format!("<root>{html}</root>");
        let doc = Document::parse(&wrapped).context("Cannot read recovered translation layout")?;
        let mut units = Vec::new();
        for node in doc
            .descendants()
            .filter(|n| n.is_element() && container(*n))
        {
            // A list item can contain paragraphs/nested lists; translate the
            // innermost prose containers, never overlap replacement ranges.
            if node
                .descendants()
                .skip(1)
                .any(|n| n.is_element() && container(n))
            {
                continue;
            }
            if is_display_equation(node) {
                // Leave the full row and number untouched in the skeleton;
                // neither is sent to the model as a translation unit.
                continue;
            }
            let source = &wrapped[node.range()];
            let start = source.find('>').context("Invalid layout opening tag")? + 1;
            let Some(end) = source.rfind("</") else {
                continue; // Empty/self-closing cells have nothing to translate.
            };
            let mut unit = Unit {
                marker: format!("[[VELLIO_UNIT_{}]]", units.len()),
                range: node.range().start + start - 6..node.range().start + end - 6,
                text: String::new(),
                role: role(node),
                keep: vec![],
            };
            for child in node.children() {
                collect(child, &wrapped, &mut unit);
            }
            if !unit.text.trim().is_empty() {
                units.push(unit);
            }
        }
        ensure!(
            !units.is_empty()
                || doc
                    .descendants()
                    .any(|n| n.is_element() && container(n) && is_display_equation(n)),
            "Recovered layout has no translation units"
        );
        Ok(Self { html, units })
    }
    pub fn has_translatable_units(&self) -> bool {
        !self.units.is_empty()
    }

    pub fn input(&self) -> String {
        let mut text = String::new();
        for unit in &self.units {
            text.push_str(&unit.marker);
            text.push('\n');
            text.push_str(&unit.text);
            text.push('\n');
        }
        text.push_str(END);
        text
    }
    pub fn roles(&self) -> Value {
        json!(
            self.units
                .iter()
                .map(|u| json!({"marker":u.marker,"kind":u.role,"protected":u.keep.iter().map(|(token,literal)| {
                    let description = if literal.contains("data-vellio-math=") {
                        let wrapped = format!("<root>{literal}</root>");
                        Document::parse(&wrapped).ok().and_then(|d| d.descendants()
                            .find_map(|n| n.attribute("data-vellio-math").map(str::to_owned)))
                            .unwrap_or_default()
                    } else if literal.starts_with("<img") { "image".to_owned() }
                    else if literal.starts_with("<code") || literal.starts_with("<pre") {
                        crate::annotations::plain_text(literal)
                    } else { literal.clone() };
                    json!({"token":token,"source":description})
                }).collect::<Vec<_>>()}))
                .collect::<Vec<_>>()
        )
    }
    pub fn translate(&self, answer: &str) -> Result<Translated> {
        let mut rest = answer.trim();
        let mut replacements = Vec::new();
        for (index, unit) in self.units.iter().enumerate() {
            rest = rest
                .strip_prefix(&unit.marker)
                .context("Translation omitted or reordered a layout unit")?;
            let next = self
                .units
                .get(index + 1)
                .map(|u| u.marker.as_str())
                .unwrap_or(END);
            let end = rest
                .find(next)
                .context("Translation omitted a layout boundary")?;
            let content = rest[..end].trim();
            ensure!(
                !content.is_empty(),
                "Translation omitted a paragraph/heading/caption"
            );
            let mut translated = String::new();
            let mut tail = content;
            for (token, literal) in &unit.keep {
                let offset = tail
                    .find(token)
                    .context("Translation lost protected math, code or formatting")?;
                let prose = &tail[..offset];
                ensure!(
                    !prose.contains("[[VELLIO_"),
                    "Translation reordered or duplicated a protected token"
                );
                translated.push_str(&escape(prose));
                translated.push_str(literal);
                tail = &tail[offset + token.len()..];
            }
            ensure!(
                !tail.contains("[[VELLIO_"),
                "Translation contains unexpected layout tokens"
            );
            translated.push_str(&escape(tail));
            replacements.push((unit.range.clone(), translated));
            rest = &rest[end..];
        }
        ensure!(
            rest == END,
            "Translation added content outside the document layout"
        );
        let mut html = self.html.clone();
        for (range, translated) in replacements.into_iter().rev() {
            html.replace_range(range, &translated);
        }
        let markdown = to_markdown(&html)?;
        Ok(Translated { markdown, html })
    }
}
fn keep(unit: &mut Unit, literal: String) {
    let token = format!("[[VELLIO_KEEP_{}]]", unit.keep.len());
    unit.text.push_str(&token);
    unit.keep.push((token, literal));
}
fn collect(node: Node<'_, '_>, source: &str, unit: &mut Unit) {
    if node.is_text() {
        unit.text.push_str(node.text().unwrap_or(""));
        return;
    }
    if !node.is_element() {
        return;
    }
    let literal = &source[node.range()];
    if node.attribute("data-vellio-math").is_some()
        || matches!(node.tag_name().name(), "img" | "br" | "hr" | "pre" | "code")
    {
        keep(unit, literal.to_owned());
        return;
    }
    let open = literal.find('>').unwrap() + 1;
    let close = literal.rfind("</").unwrap_or(literal.len());
    keep(unit, literal[..open].to_owned());
    for child in node.children() {
        collect(child, source, unit);
    }
    if close < literal.len() {
        keep(unit, literal[close..].to_owned());
    }
}

/// A semantic Markdown projection for old readers/editing/export; native HTML
/// retains caption centering, source links and the original paragraph skeleton.
pub fn to_markdown(html: &str) -> Result<String> {
    let wrapped = format!("<root>{}</root>", xhtml(html));
    let doc = Document::parse(&wrapped)?;
    Ok(markdown_node(doc.root_element()).trim().to_owned())
}
fn markdown_node(node: Node<'_, '_>) -> String {
    if node.is_text() {
        return node.text().unwrap_or("").to_owned();
    }
    if let Some(tex) = node.attribute("data-vellio-math") {
        return if node.attribute("data-display") == Some("true") {
            format!("\n\n$${tex}$$\n\n")
        } else {
            format!("${tex}$")
        };
    }
    let inner = || node.children().map(markdown_node).collect::<String>();
    let name = node.tag_name().name();
    match name {
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => format!(
            "{} {}\n\n",
            "#".repeat(name[1..].parse().unwrap_or(2)),
            inner().trim()
        ),
        "p" | "div" | "blockquote" => format!("{}\n\n", inner().trim()),
        "strong" | "b" => format!("**{}**", inner()),
        "em" | "i" => format!("*{}*", inner()),
        "pre" => format!(
            "\n\n```\n{}\n```\n\n",
            node.descendants()
                .filter(Node::is_text)
                .filter_map(|n| n.text())
                .collect::<String>()
                .trim_end()
        ),
        "code" => format!("`{}`", inner()),
        "li" => format!("- {}\n", inner().trim()),
        "ul" | "ol" => format!("{}\n", inner()),
        "br" => "\n".into(),
        "a" => format!("[{}]({})", inner(), node.attribute("href").unwrap_or("")),
        "img" => format!(
            "![{}]({})",
            node.attribute("alt").unwrap_or(""),
            node.attribute("src").unwrap_or("")
        ),
        "table" => {
            let rows: Vec<Vec<_>> = node
                .descendants()
                .filter(|n| n.has_tag_name("tr"))
                .map(|r| {
                    r.children()
                        .filter(|c| c.has_tag_name("td") || c.has_tag_name("th"))
                        .map(|c| markdown_node(c).replace('|', "\\|").replace('\n', " "))
                        .collect()
                })
                .collect();
            let mut table = String::new();
            for (i, row) in rows.iter().enumerate() {
                table.push_str(&format!("| {} |\n", row.join(" | ")));
                if i == 0 {
                    table.push_str(&format!("| {} |\n", vec!["---"; row.len()].join(" | ")));
                }
            }
            format!("{table}\n")
        }
        _ => inner(),
    }
}

pub fn metadata(source: &str, translated: &Translated) -> Value {
    json!({"version":1,"source_hash":crate::model::text_hash(source),
        "translation_hash":crate::model::text_hash(&translated.markdown),"html":translated.html})
}
pub fn saved_html(block: &Value, active_text: &str) -> Option<String> {
    let layout = &block["native_translation_layout"];
    (layout["version"] == 1
        && layout["source_hash"] == crate::model::text_hash(block["en"].as_str()?)
        && layout["translation_hash"] == crate::model::text_hash(active_text))
    .then(|| layout["html"].as_str().map(str::to_owned))
    .flatten()
}

fn number_prefix(text: &str) -> Option<&str> {
    let end = text.find(|c: char| !c.is_ascii_digit() && c != '.')?;
    let number = text[..end].trim_end_matches('.');
    (number.contains('.') && text[end..].starts_with(char::is_whitespace)).then_some(number)
}
fn caption_number(text: &str) -> Option<String> {
    let text = text.trim_start();
    let rest = ["Figure", "Fig.", "Table", "图", "表"]
        .iter()
        .find_map(|prefix| text.strip_prefix(prefix))?
        .trim_start();
    let number: String = rest
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '-' || *c == '.')
        .collect();
    (!number.is_empty()).then_some(number.trim_end_matches('.').to_owned())
}

// Conservative aliases for old flattened translations. Never infer a new
// expression: only put back TeX already recovered from this exact source page.
fn legacy_alias(tex: &str) -> Option<String> {
    let tex = tex.trim();
    if let Some(inner) = tex
        .strip_prefix("\\sqrt{")
        .and_then(|s| s.strip_suffix('}'))
    {
        return Some(format!("√{}", legacy_alias(inner)?));
    }
    if let Some(inner) = tex
        .strip_prefix("\\frac{1}{")
        .and_then(|s| s.strip_suffix('}'))
    {
        return Some(format!("1/{}", legacy_alias(inner)?));
    }
    if let Some((base, script)) = tex.split_once('_') {
        let script = script.trim_start_matches('{').trim_end_matches('}');
        if base.chars().all(|c| c.is_ascii_alphabetic())
            && script.chars().all(|c| c.is_ascii_alphanumeric())
        {
            return Some(format!("{base}{script}"));
        }
    }
    (tex.len() == 1 && tex.chars().all(|c| c.is_ascii_alphabetic())).then(|| tex.to_owned())
}
fn legacy_math(html: &str, aliases: &BTreeMap<String, String>) -> String {
    let text = crate::annotations::plain_text(html);
    // Old translations may spell a source-backed formula as `1/ √ dk`.
    // Ignore only layout whitespace inside the alias, retaining original
    // offsets and word boundaries so SDK / ordinary prose are not rewritten.
    let mut compact = String::new();
    let mut offsets = Vec::new();
    for (i, ch) in text.char_indices().filter(|(_, ch)| !ch.is_whitespace()) {
        compact.push(ch);
        offsets.extend(std::iter::repeat_n(i..i + ch.len_utf8(), ch.len_utf8()));
    }
    let mut edits = Vec::new();
    let mut occupied: Vec<Range<usize>> = vec![];
    let mut aliases: Vec<_> = aliases.iter().collect();
    aliases.sort_by_key(|(alias, _)| std::cmp::Reverse(alias.len()));
    for (alias, tex) in aliases {
        for (offset, _) in compact.match_indices(alias) {
            let range = offsets[offset].start..offsets[offset + alias.len() - 1].end;
            if occupied
                .iter()
                .any(|r| range.start < r.end && range.end > r.start)
                || text[..range.start]
                    .chars()
                    .next_back()
                    .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
                || text[range.end..]
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
            {
                continue;
            }
            let (prefix, suffix) = crate::annotations::context(&text, range.clone());
            edits.push((
                text[range.clone()].to_owned(),
                prefix,
                suffix,
                crate::formula::native_html(tex, false),
            ));
            occupied.push(range);
        }
    }
    crate::annotations::source_math_edits(html, &edits)
}

/// Display old line-aligned PDF translations using only source-backed roles.
/// Do not guess structure for a one-line translation or overwrite user edits.
pub fn legacy_html(
    source: &str,
    translation: &str,
    recovered: &str,
    page: usize,
) -> Option<String> {
    if translation.lines().count() < 2
        || translation.contains("\n\n")
        || translation.lines().any(|line| {
            let line = line.trim_start();
            ["#", "```", "~~~", "|", "> ", "- ", "* "]
                .iter()
                .any(|prefix| line.starts_with(prefix))
        })
    {
        return None;
    }
    let wrapped = format!("<root>{}</root>", xhtml(recovered));
    let doc = Document::parse(&wrapped).ok()?;
    let mut headings = BTreeMap::new();
    let mut captions = Vec::new();
    let mut equations = BTreeMap::new();
    let mut aliases = BTreeMap::new();
    let mut caption_images = BTreeMap::new();
    let mut pending_images = String::new();
    for node in doc.root_element().children().filter(Node::is_element) {
        let has_images = node.descendants().any(|n| n.has_tag_name("img"));
        let has_text = node
            .descendants()
            .filter(Node::is_text)
            .any(|n| !n.text().unwrap_or("").trim().is_empty());
        if has_images && !has_text {
            pending_images.push_str(&wrapped[node.range()]);
        } else {
            if role(node) == "caption"
                && let Some(number) =
                    caption_number(&crate::annotations::plain_text(&wrapped[node.range()]))
            {
                caption_images.insert(number, pending_images.clone());
            }
            pending_images.clear();
        }
    }
    for node in doc.descendants().filter(Node::is_element) {
        if matches!(
            node.tag_name().name(),
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6"
        ) {
            let plain = crate::annotations::plain_text(&wrapped[node.range()]);
            if let Some(number) = number_prefix(&plain) {
                headings.insert(number.to_owned(), node.tag_name().name());
            }
        }
        if node.has_tag_name("p")
            && role(node) == "caption"
            && let Some(number) =
                caption_number(&crate::annotations::plain_text(&wrapped[node.range()]))
        {
            captions.push(number);
        }
        if let Some(tex) = node.attribute("data-vellio-math") {
            if node.attribute("data-display") == Some("true") {
                if let Some(parent) = node.parent().filter(|p| p.is_element()) {
                    let html = &wrapped[parent.range()];
                    let plain = crate::annotations::plain_text(html);
                    if let Some(start) = plain.rfind('(').filter(|_| plain.ends_with(')')) {
                        let number = &plain[start..];
                        if number[1..number.len() - 1]
                            .chars()
                            .all(|c| c.is_ascii_digit())
                        {
                            equations.insert(number.to_owned(), html.to_owned());
                        }
                    }
                }
            } else if let Some(alias) = legacy_alias(tex) {
                aliases.insert(alias, tex.to_owned());
            }
        }
    }
    let lines: Vec<_> = translation
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    let first_source = source.lines().next().unwrap_or("");
    let source_prose = crate::annotations::plain_text(recovered);
    let suppress_diagram_labels = lines
        .get(1)
        .and_then(|l| caption_number(l))
        .is_some_and(|n| captions.contains(&n))
        && !first_source.is_empty()
        && !source_prose.contains(first_source)
        && doc
            .descendants()
            .filter(|n| n.is_element() && container(*n))
            .find(|n| {
                n.descendants()
                    .filter(Node::is_text)
                    .any(|t| !t.text().unwrap_or("").trim().is_empty())
            })
            .is_some_and(|n| role(n) == "caption");
    let mut result = String::new();
    for (index, line) in lines.iter().enumerate() {
        if (index == 0 && suppress_diagram_labels)
            || (index + 1 == lines.len()
                && *line == page.to_string()
                && source
                    .lines()
                    .last()
                    .is_some_and(|s| s.trim() == page.to_string()))
        {
            continue;
        }
        if line.contains('=')
            && let Some((_, equation)) = equations.iter().find(|(n, _)| line.ends_with(n.as_str()))
        {
            result.push_str(equation);
            continue;
        }
        let html = crate::formula::native_markdown_html(line);
        let html = legacy_math(&html, &aliases);
        if let Some(tag) = number_prefix(line).and_then(|n| headings.get(n)) {
            result.push_str(&format!(
                "<{tag}>{}</{tag}>",
                html.strip_prefix("<p><span>")
                    .and_then(|s| s.strip_suffix("</span></p>"))
                    .unwrap_or(&html)
            ));
        } else if let Some(number) = caption_number(line).filter(|n| captions.contains(n)) {
            if let Some(images) = caption_images.remove(&number) {
                result.push_str(&images);
            }
            result.push_str(&html.replace("<p>", "<p style=\"text-align:center\">"));
        } else {
            result.push_str(&html);
        }
    }
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn source() -> String {
        format!(
            "<h3>3.2.1 Scaled Attention</h3><p><span>Use {} and <em>keys</em>.</span></p><p style=\"text-align:center\"><span>Figure 2: Left and right.</span></p><p>{} (1)</p>",
            crate::formula::native_html("d_{k}", false),
            crate::formula::native_html(r"\frac{QK^{T}}{\sqrt{d_{k}}}", true)
        )
    }
    #[test]
    fn plan_preserves_headings_captions_math_and_emphasis_and_escapes_model_markup() {
        let plan = Plan::from_html(&source()).unwrap();
        let answer = plan
            .input()
            .replace("Scaled Attention", "缩放注意力")
            .replace("Use ", "使用 <script> ")
            .replace("keys", "键")
            .replace("Figure 2: Left and right.", "图 2：左与右。");
        let translated = plan.translate(&answer).unwrap();
        assert!(translated.html.contains("<h3>3.2.1 缩放注意力</h3>"));
        assert!(
            translated
                .html
                .contains("text-align:center\"><span>图 2：左与右。")
        );
        assert!(translated.html.contains("<em>键</em>"));
        assert!(translated.html.contains("&lt;script&gt;"));
        assert_eq!(translated.html.matches("data-vellio-math").count(), 2);
        assert!(translated.markdown.contains("### 3.2.1 缩放注意力"));
        assert!(translated.markdown.contains("$d_{k}$"));
        let meta = metadata("original", &translated);
        let block = json!({"en":"original","native_translation_layout":meta});
        assert_eq!(
            saved_html(&block, &translated.markdown),
            Some(translated.html)
        );
        assert!(saved_html(&block, "用户修改").is_none());
    }
    #[test]
    fn display_equations_are_retained_without_requesting_a_translation_unit() {
        let source = source();
        let plan = Plan::from_html(&source).unwrap();
        assert!(
            !plan.input().contains("(1)"),
            "Display equations and their numbers are not text to translate"
        );
        assert!(
            plan.roles()
                .as_array()
                .unwrap()
                .iter()
                .all(|u| u["kind"] != "equation")
        );
        let translated = plan
            .translate(&plan.input().replace("Use ", "使用 "))
            .unwrap();
        assert_eq!(translated.html.matches("data-display=\"true\"").count(), 1);
        assert!(
            translated
                .html
                .ends_with(&source[source.rfind("<p>").unwrap()..])
        );
        assert!(translated.html.contains("data-display=\"false\""));
        assert!(translated.markdown.contains("$$"));
    }

    #[test]
    fn formula_only_plan_is_preserved_locally_without_translation_units() {
        let source = format!("<p>{} (1)</p>", crate::formula::native_html("x=y", true));
        let plan = Plan::from_html(&source).unwrap();
        assert!(!plan.has_translatable_units());
        assert_eq!(plan.input(), END);
        assert_eq!(plan.roles(), json!([]));
        assert_eq!(plan.translate(END).unwrap().html, source);
    }

    #[test]
    fn mixed_formula_prose_still_has_a_protected_translation_unit() {
        let source = format!(
            "<p>Explain {} in words.</p>",
            crate::formula::native_html("x=y", true)
        );
        let plan = Plan::from_html(&source).unwrap();
        assert!(plan.has_translatable_units());
        assert!(plan.input().contains("Explain"));
        assert!(
            plan.translate(&plan.input().replace("Explain", "解释"))
                .unwrap()
                .html
                .contains("data-display=\"true\"")
        );
    }

    #[test]
    fn omitted_units_or_protected_tokens_are_never_accepted() {
        let plan = Plan::from_html(&source()).unwrap();
        for broken in [
            plan.input().replace("[[VELLIO_UNIT_0]]", ""),
            plan.input().replace("[[VELLIO_KEEP_0]]", ""),
            plan.input().replace(END, ""),
            format!("{} extra", plan.input()),
        ] {
            assert!(plan.translate(&broken).is_err());
        }
    }
    #[test]
    fn code_images_and_tables_remain_in_the_skeleton() {
        let html = "<p>Hello <code>x = 1</code> <img src=\"local.png\" alt=\"figure\"></p><table><tr><th>Name</th></tr><tr><td>Value</td></tr></table>";
        let plan = Plan::from_html(html).unwrap();
        let translated = plan
            .translate(
                &plan
                    .input()
                    .replace("Hello", "你好")
                    .replace("Name", "名称")
                    .replace("Value", "数值"),
            )
            .unwrap();
        assert!(translated.html.contains("<code>x = 1</code>"));
        assert!(translated.html.contains("src=\"local.png\""));
        assert!(translated.markdown.contains("| 名称 |\n| --- |\n| 数值 |"));
    }
    #[test]
    fn empty_cells_and_paragraphs_are_not_reported_as_missing_translations() {
        let plan = Plan::from_html("<p></p><table><tr><td/></tr></table><p>Text</p>").unwrap();
        let translated = plan
            .translate(&plan.input().replace("Text", "正文"))
            .unwrap();
        assert!(translated.html.contains("<td/>"));
        assert!(translated.html.contains("<p>正文</p>"));
    }

    #[test]
    fn legacy_inline_fractions_recover_spacing_from_source_backed_aliases() {
        let tex = r"\frac{1}{\sqrt{d_{k}}}";
        let recovered = format!(
            "<p>Scale by {}.</p>",
            crate::formula::native_html(tex, false)
        );
        let source = "Scale by sqrt 1 dk.\n4";
        let translation = "将点积乘以 1/ √ dk。不要改 SDK 或 11/√dk。\n4";
        let html = legacy_html(source, translation, &recovered, 4).unwrap();
        assert_eq!(html.matches("data-vellio-math=").count(), 1, "{html}");
        assert!(html.contains(&crate::formula::native_html(tex, false)));
        assert!(html.contains("SDK 或 11/√dk"));
        let no_evidence = legacy_html(source, translation, "<p>Plain source</p>", 4).unwrap();
        assert!(!no_evidence.contains("data-vellio-math="));
    }

    #[test]
    fn legacy_page_recovers_source_backed_roles_without_mutating_text() {
        let recovered = format!(
            "<p><a href=\"easyread-image:figure-2\"><img src=\"source.png\"/></a></p><p style=\"text-align:center\"><span>Figure 2: Left and right.</span></p><h3>3.2.1 Scaled Attention</h3><p>Use {}.</p><p>{} (1)</p>",
            crate::formula::native_html("d_{k}", false),
            crate::formula::native_html(r"\frac{QK^{T}}{\sqrt{d_{k}}}", true)
        );
        let original = "Diagram titles\nFigure 2: Left and right.\n3.2.1 Scaled Attention\nUse dk.\nFormula (1)\n4";
        let zh = "图内标签\n图 2：左与右。\n3.2.1 缩放注意力\n使用 dk，不要改 SDK。\nAttention(Q, K, V) = softmax(QKᵀ/√dk)V (1)\n4";
        let html = legacy_html(original, zh, &recovered, 4).unwrap();
        assert!(!html.contains("图内标签"));
        assert!(html.find("<img ") < html.find("图 2："));
        assert_eq!(html.matches("easyread-image:figure-2").count(), 1);
        assert!(html.contains("<h3>3.2.1 缩放注意力</h3>"));
        assert!(html.contains("text-align:center\"><span>图 2：左与右。"));
        assert!(html.contains("data-vellio-math=\"d_{k}\""));
        assert!(html.contains("data-display=\"true\""));
        assert!(html.contains("SDK"));
        assert!(!html.ends_with("<span>4</span></p>"));
        assert!(legacy_html(original, "## 用户标题\n正文", &recovered, 4).is_none());
        assert!(legacy_html(original, "一行译文", &recovered, 4).is_none());
        for markdown in [
            "代码：\n```rust\nx = 1\n```",
            "| Name |\n| --- |\n| Value |",
            "- first\n- second",
        ] {
            assert!(legacy_html(original, markdown, &recovered, 4).is_none());
        }
    }
}
