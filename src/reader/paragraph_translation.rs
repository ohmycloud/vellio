//! Display-only paragraph companions. Never split persisted block IDs or guess
//! alignments when the recovered source and translation have different roles.
use roxmltree::{Document, Node};
use std::{collections::HashSet, ops::Range};

#[derive(Clone, Copy, PartialEq)]
enum Role {
    Prose,
    Heading,
    Caption,
    Equation,
    Media,
}
impl Role {
    fn translatable(self) -> bool {
        !matches!(self, Self::Media | Self::Equation)
    }
}
struct Unit {
    range: Range<usize>,
    role: Role,
    paragraphs: usize,
    label: bool,
    numeric: bool,
    companion_html: String,
}
struct Page {
    html: String,
    units: Vec<Unit>,
}
impl Page {
    fn parse(html: &str) -> Option<Self> {
        let html = crate::translation_layout::xhtml(html);
        let wrapped = format!("<root>{html}</root>");
        let doc = Document::parse(&wrapped).ok()?;
        let mut units = Vec::new();
        for node in doc.root_element().children().filter(Node::is_element) {
            let tag = node.tag_name().name();
            // Lists, tables, code and nested containers have independent native
            // paragraph counters. Leave them on the full-block companion path.
            if !matches!(tag, "p" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6") {
                return None;
            }
            let plain = crate::annotations::plain_text(&wrapped[node.range()]);
            let role = if plain.trim().is_empty() {
                Role::Media
            } else if tag.starts_with('h') {
                Role::Heading
            } else if crate::translation_layout::is_display_equation(node) {
                Role::Equation
            } else if node
                .attribute("style")
                .is_some_and(|s| s.replace(' ', "").contains("text-align:center"))
            {
                Role::Caption
            } else {
                Role::Prose
            };
            // A source-backed bold label can begin a logical paragraph which
            // PDF geometry split across several <p>s (e.g. Encoder:/Decoder:).
            let label = node
                .descendants()
                .filter(|n| n.has_tag_name("strong") || n.has_tag_name("b"))
                .any(|n| {
                    let text = crate::annotations::plain_text(&wrapped[n.range()]);
                    text.trim_end().ends_with(':') && plain.trim_start().starts_with(text.trim())
                });
            let range = node.range();
            // Inline math stays with translated prose, while display math
            // and images belong only to the primary flow.
            let companion_html = companion_fragment(node, &wrapped);
            units.push(Unit {
                range: range.start - 6..range.end - 6,
                role,
                paragraphs: 1,
                label,
                numeric: !plain.trim().is_empty()
                    && plain.trim().chars().all(|c| c.is_ascii_digit()),
                companion_html,
            });
        }
        Some(Self { html, units })
    }

    fn join_label_continuations(&mut self) {
        let mut joined: Vec<Unit> = Vec::new();
        for unit in self.units.drain(..) {
            if unit.role == Role::Prose
                && !unit.label
                && !unit.numeric
                && let Some(previous) = joined
                    .last_mut()
                    .filter(|p| p.role == Role::Prose && p.label)
            {
                previous.range.end = unit.range.end;
                previous.paragraphs += unit.paragraphs;
                previous.companion_html.push_str(&unit.companion_html);
            } else {
                joined.push(unit);
            }
        }
        self.units = joined;
    }
}

fn companion_fragment(root: Node<'_, '_>, source: &str) -> String {
    let range = root.range();
    let mut output = String::new();
    let mut cursor = range.start;
    for node in root.descendants().filter(Node::is_element) {
        let node_range = node.range();
        if node_range.start < cursor {
            continue;
        } // Already omitted its parent.
        let tag = node.tag_name().name();
        let equation_row = matches!(
            tag,
            "p" | "div"
                | "blockquote"
                | "li"
                | "td"
                | "th"
                | "h1"
                | "h2"
                | "h3"
                | "h4"
                | "h5"
                | "h6"
        ) && crate::translation_layout::is_display_equation(node);
        let display_math = node.attribute("data-vellio-math").is_some()
            && node.attribute("data-display") == Some("true");
        if tag == "img" || display_math || equation_row {
            let omit = if equation_row && matches!(tag, "td" | "th") {
                // Keep the table's cells/column structure even when their
                // shared formula has no second-language content.
                let children: Vec<_> = node.children().collect();
                let Some(first) = children.first() else {
                    continue;
                };
                first.range().start..children.last().unwrap().range().end
            } else {
                node_range
            };
            output.push_str(&source[cursor..omit.start]);
            cursor = omit.end;
        }
    }
    output.push_str(&source[cursor..range.end]);
    output
}

/// Shared figures and standalone display equations (including their numbers)
/// are rendered only in the primary flow, even on the whole-block fallback.
/// Inline math, prose, links, code and table structure are retained.
pub fn without_shared_content(html: &str) -> String {
    if !html.contains("<img") && !html.contains("data-display") {
        return html.to_owned();
    }
    let normalized = crate::translation_layout::xhtml(html);
    let wrapped = format!("<root>{normalized}</root>");
    let Ok(doc) = Document::parse(&wrapped) else {
        return html.to_owned();
    };
    let fragment = companion_fragment(doc.root_element(), &wrapped);
    fragment[6..fragment.len() - 7].to_owned()
}

pub struct PairLayout {
    primary: Page,
    secondary: Page,
    pairs: Vec<(usize, usize)>,
}
impl PairLayout {
    /// Source and target text must have the same ordered structural roles.
    /// Media and display equations stay in the primary flow, not companions.
    pub fn new(original: &str, translation: &str, original_first: bool) -> Option<Self> {
        Self::build(original, translation, original_first, None, 2)
    }

    pub fn for_pdf_page(
        original: &str,
        translation: &str,
        original_first: bool,
        page: usize,
    ) -> Option<Self> {
        Self::build(original, translation, original_first, Some(page), 2)
    }

    /// Bilingual mode also pairs a single paragraph/caption, so even a page
    /// consisting of one image and caption never repeats the image.
    pub fn for_bilingual(
        original: &str,
        translation: &str,
        original_first: bool,
        page: Option<usize>,
    ) -> Option<Self> {
        Self::build(original, translation, original_first, page, 1)
    }

    fn build(
        original: &str,
        translation: &str,
        original_first: bool,
        page: Option<usize>,
        minimum_pairs: usize,
    ) -> Option<Self> {
        let mut source = Page::parse(original)?;
        let target = Page::parse(translation)?;
        // Legacy translated layouts omit the extracted page-number footer.
        // Keep it in source-first flow, but it has no translated companion.
        if let Some(page) = page
            && let Some(last) = source.units.last_mut()
            && last.role == Role::Prose
            && crate::annotations::plain_text(&source.html[last.range.clone()]).trim()
                == page.to_string()
        {
            last.role = Role::Media;
        }
        let roles = |page: &Page| {
            page.units
                .iter()
                .filter(|u| u.role.translatable())
                .map(|u| u.role)
                .collect::<Vec<_>>()
        };
        if roles(&source) != roles(&target) {
            source.join_label_continuations();
        }
        let shared_equations = source
            .units
            .iter()
            .chain(&target.units)
            .any(|u| u.role == Role::Equation);
        if roles(&source) != roles(&target)
            || (!shared_equations && roles(&source).len() < minimum_pairs)
        {
            return None;
        }
        let (primary, secondary) = if original_first {
            (source, target)
        } else {
            (target, source)
        };
        let pairs = primary
            .units
            .iter()
            .enumerate()
            .filter(|(_, u)| u.role.translatable())
            .map(|(i, _)| i)
            .zip(
                secondary
                    .units
                    .iter()
                    .enumerate()
                    .filter(|(_, u)| u.role.translatable())
                    .map(|(i, _)| i),
            )
            .collect();
        Some(Self {
            primary,
            secondary,
            pairs,
        })
    }

    pub fn all_pairs(&self) -> HashSet<usize> {
        (0..self.pairs.len()).collect()
    }

    /// Map native painted paragraph ordinals back to the stable logical pair,
    /// including inserted companions and physically split source paragraphs.
    pub fn pair_at(&self, paragraph: usize, expanded: &HashSet<usize>) -> Option<usize> {
        let unit = self.unit_at(paragraph, expanded)?;
        self.pairs.iter().position(|(primary, _)| *primary == unit)
    }

    /// Locate a paired translation in both its own language flow and the
    /// combined native flow. The caller expands a source-first companion first.
    pub fn translation_target(
        &self,
        pair: usize,
        expanded: &HashSet<usize>,
        original_first: bool,
    ) -> Option<(usize, usize)> {
        let &(primary, secondary) = self.pairs.get(pair)?;
        let (page, unit) = if original_first {
            (&self.secondary, secondary)
        } else {
            (&self.primary, primary)
        };
        let translation = page.units[..unit].iter().map(|u| u.paragraphs).sum();
        let mut display = 0;
        for (index, unit) in self.primary.units.iter().enumerate() {
            if index == primary {
                return if original_first {
                    expanded
                        .contains(&pair)
                        .then_some((translation, display + unit.paragraphs))
                } else {
                    Some((translation, display))
                };
            }
            display += unit.paragraphs;
            if let Some(pair) = self
                .pairs
                .iter()
                .position(|(p, _)| *p == index)
                .filter(|p| expanded.contains(p))
            {
                display += self.secondary.units[self.pairs[pair].1].paragraphs;
            }
        }
        None
    }

    pub fn is_equation_at(&self, paragraph: usize, expanded: &HashSet<usize>) -> bool {
        self.unit_at(paragraph, expanded)
            .is_some_and(|unit| self.primary.units[unit].role == Role::Equation)
    }

    fn unit_at(&self, paragraph: usize, expanded: &HashSet<usize>) -> Option<usize> {
        let mut ordinal = 0;
        for (index, unit) in self.primary.units.iter().enumerate() {
            let pair = self.pairs.iter().position(|(p, _)| *p == index);
            if (ordinal..ordinal + unit.paragraphs).contains(&paragraph) {
                return Some(index);
            }
            ordinal += unit.paragraphs;
            if let Some(pair) = pair.filter(|p| expanded.contains(p)) {
                let secondary = &self.secondary.units[self.pairs[pair].1];
                if (ordinal..ordinal + secondary.paragraphs).contains(&paragraph) {
                    return Some(index);
                }
                ordinal += secondary.paragraphs;
            }
        }
        None
    }

    pub fn render(&self, expanded: &HashSet<usize>) -> String {
        let mut html = String::with_capacity(self.primary.html.len());
        let mut cursor = 0;
        for (pair, (primary, secondary)) in self.pairs.iter().enumerate() {
            let primary = &self.primary.units[*primary];
            html.push_str(&self.primary.html[cursor..primary.range.end]);
            cursor = primary.range.end;
            if expanded.contains(&pair) {
                let secondary = &self.secondary.units[*secondary];
                html.push_str("<blockquote data-easyread-companion=\"true\">");
                html.push_str(&secondary.companion_html);
                html.push_str("</blockquote>");
            }
        }
        html.push_str(&self.primary.html[cursor..]);
        html
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bilingual_display_equations_are_shared_but_inline_math_stays_with_prose() {
        let inline = crate::formula::native_html("d_k", false);
        let equation = crate::formula::native_html(
            r"\operatorname{Attention}(Q,K,V)=\operatorname{softmax}\left(\frac{QK^T}{\sqrt{d_k}}\right)V",
            true,
        );
        let source = format!(
            "<p>Before {inline}.</p><p style=\"text-align:center\" data-vellio-equation=\"true\">{equation} &#8195;(1)</p><p>After {inline}.</p>"
        );
        let target = format!(
            "<p>之前 {inline}。</p><p style=\"text-align:center\" data-vellio-equation=\"true\">{equation} &#8195;(1)</p><p>之后 {inline}。</p>"
        );
        for original_first in [true, false] {
            let layout = PairLayout::for_bilingual(&source, &target, original_first, None).unwrap();
            let expanded = layout.all_pairs();
            let html = layout.render(&expanded);
            assert_eq!(
                html.matches("data-display=\"true\"").count(),
                1,
                "The numbered display equation must not have a translated copy"
            );
            assert_eq!(html.matches("(1)").count(), 1);
            assert_eq!(html.matches("data-display=\"false\"").count(), 4);
            assert_eq!(
                html.matches("<blockquote data-easyread-companion=\"true\">")
                    .count(),
                2
            );
            assert_eq!(layout.pair_at(2, &expanded), None);
            assert_eq!(layout.pair_at(3, &expanded), Some(1));
            assert_eq!(layout.pair_at(4, &expanded), Some(1));
        }
    }

    #[test]
    fn equation_only_layout_has_no_companion_or_language_toggle_pair() {
        let equation = format!("<p>{} (A.1)</p>", crate::formula::native_html("x=y", true));
        for original_first in [true, false] {
            let layout =
                PairLayout::for_bilingual(&equation, &equation, original_first, None).unwrap();
            assert!(layout.all_pairs().is_empty());
            assert_eq!(layout.render(&layout.all_pairs()), equation);
            assert!(layout.is_equation_at(0, &HashSet::new()));
            assert_eq!(layout.pair_at(0, &HashSet::new()), None);
        }
    }

    #[test]
    fn complex_companion_keeps_inline_math_and_table_structure_not_display_rows() {
        let display = crate::formula::native_html("x=y", true);
        let inline = crate::formula::native_html("d_k", false);
        let html = format!(
            "<ul><li><p>{display} (1)</p><p>Keep {inline} prose.</p></li></ul><table><tr><td>{display} (2)</td><td>Keep cell.</td></tr></table>"
        );
        let companion = without_shared_content(&html);
        assert!(!companion.contains("data-display=\"true\""));
        assert!(!companion.contains("(1)") && !companion.contains("(2)"));
        assert!(companion.contains("data-display=\"false\""));
        assert!(companion.contains("Keep cell."));
        assert!(companion.contains("<td></td>"));
        assert_eq!(companion.matches("<td>").count(), 2);
    }

    #[test]
    fn prose_around_display_math_is_not_discarded_as_an_equation_row() {
        let display = crate::formula::native_html("x=y", true);
        let en = format!("<p>Explain {display} in words.</p>");
        let zh = format!("<p>用文字解释 {display}。</p>");
        let layout = PairLayout::for_bilingual(&en, &zh, true, None).unwrap();
        let html = layout.render(&layout.all_pairs());
        assert_eq!(html.matches("data-display=\"true\"").count(), 1);
        assert!(html.contains("Explain") && html.contains("用文字解释"));
        assert_eq!(layout.pair_at(0, &layout.all_pairs()), Some(0));
    }

    #[test]
    fn encoder_companion_is_between_encoder_and_decoder_without_duplicating_image() {
        let image = "<p><img src=\"picture.png\"></p>";
        let en = format!(
            "{image}<p style=\"text-align:center\">Figure 1</p><h2>3.1 Stacks</h2><p><strong>Encoder:</strong> English encoder.</p><p>Its continuation.</p><p><strong>Decoder:</strong> English decoder.</p><p>Decoder continuation.</p>"
        );
        let zh = format!(
            "{image}<p style=\"text-align:center\">图 1</p><h2>3.1 堆栈</h2><p>编码器译文。</p><p>解码器译文。</p>"
        );
        let layout = PairLayout::new(&en, &zh, false).unwrap();
        let expanded = HashSet::from([2]);
        let html = layout.render(&expanded);
        assert_eq!(html.matches("<img ").count(), 1);
        assert!(html.find("编码器译文") < html.find("English encoder"));
        assert!(html.find("Its continuation") < html.find("解码器译文"));
        assert!(!html.contains("English decoder"));
        assert_eq!(layout.pair_at(3, &expanded), Some(2));
        assert_eq!(layout.pair_at(4, &expanded), Some(2));
        assert_eq!(layout.pair_at(5, &expanded), Some(2));
        assert_eq!(layout.pair_at(6, &expanded), Some(3));
        assert!(!layout.render(&HashSet::new()).contains("English encoder"));
    }

    #[test]
    fn source_first_supports_multiple_independent_companions_and_repeated_paragraphs() {
        let layout = PairLayout::new(
            "<p>Same.</p><p>Same.</p><p>Last.</p>",
            "<p>第一段。</p><p>第二段。</p><p>最后一段。</p>",
            true,
        )
        .unwrap();
        let expanded = HashSet::from([0, 2]);
        let html = layout.render(&expanded);
        assert!(html.contains("第一段"));
        assert!(!html.contains("第二段"));
        assert!(html.contains("最后一段"));
        assert_eq!(layout.pair_at(2, &expanded), Some(1));
        assert_eq!(layout.pair_at(4, &expanded), Some(2));
    }

    #[test]
    fn bilingual_all_pairs_keep_inline_and_caption_images_only_once() {
        for (source, target) in [
            (
                "<p><img src=\"figure.png\"></p><p style=\"text-align:center\">Figure 1</p>",
                "<p><img src=\"figure.png\"></p><p style=\"text-align:center\">图 1</p>",
            ),
            (
                "<p>Before <a href=\"image\"><img src=\"figure.png\"></a> after.</p>",
                "<p>之前 <a href=\"image\"><img src=\"figure.png\"></a> 之后。</p>",
            ),
        ] {
            for original_first in [true, false] {
                let layout =
                    PairLayout::for_bilingual(source, target, original_first, None).unwrap();
                let html = layout.render(&layout.all_pairs());
                assert_eq!(html.matches("<img ").count(), 1);
                assert_eq!(
                    html.matches("<blockquote data-easyread-companion=\"true\">")
                        .count(),
                    1
                );
                let companion = html
                    .split_once("<blockquote data-easyread-companion=\"true\">")
                    .unwrap()
                    .1;
                assert!(!companion.contains("<img "));
                assert!(
                    companion.contains(if original_first { "图 1" } else { "Figure 1" })
                        || companion.contains(if original_first { "之后" } else { "after" })
                );
            }
        }
    }

    #[test]
    fn complex_companion_omits_images_without_rewriting_text_code_or_links() {
        let html = "<ul><li>Keep <strong>bold</strong> <img src=\"photo.png\"></li></ul><pre>&lt;img src=\"literal\"&gt;</pre><p><a href=\"destination\">Keep link</a></p>";
        let companion = without_shared_content(html);
        assert!(!companion.contains("<img "));
        assert!(companion.contains("<strong>bold</strong>"));
        assert!(companion.contains("&lt;img src=\"literal\"&gt;"));
        assert!(companion.contains("<a href=\"destination\">Keep link</a>"));
    }

    #[test]
    fn known_pdf_footer_is_not_mistaken_for_a_missing_translation() {
        let en = "<h2>Stacks</h2><p><strong>Encoder:</strong> Source.</p><p>Continuation.</p><p><strong>Decoder:</strong> Source.</p><p>3</p>";
        let zh = "<h2>堆栈</h2><p>编码器。</p><p>解码器。</p>";
        assert!(PairLayout::new(en, zh, false).is_none());
        let layout = PairLayout::for_pdf_page(en, zh, false, 3).unwrap();
        let html = layout.render(&HashSet::from([1]));
        assert!(html.contains("Encoder:"));
        assert!(!html.contains("Decoder:"));
        assert!(!html.contains("<p>3</p>"));
        assert!(PairLayout::for_pdf_page(en, zh, false, 4).is_none());
    }

    #[test]
    fn ambiguous_counts_roles_and_nested_blocks_do_not_guess_an_alignment() {
        assert!(PairLayout::new("<p>A.</p><p>B.</p>", "<p>合并后的译文。</p>", false).is_none());
        assert!(
            PairLayout::new(
                "<h2>Title</h2><p>Body.</p>",
                "<p>标题。</p><p>正文。</p>",
                false
            )
            .is_none()
        );
        assert!(
            PairLayout::new(
                "<ul><li>A</li><li>B</li></ul>",
                "<p>甲。</p><p>乙。</p>",
                false
            )
            .is_none()
        );
    }
}
