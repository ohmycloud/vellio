//! Typed presentation adapter for EasyRead schema-2 state.
//! Persistence uses the original JSON value to retain unknown fields.
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Clone, Debug, Deserialize)]
pub struct ReaderState {
    pub paper: Paper,
    #[serde(default)]
    pub reader: Reader,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Paper {
    pub meta: Meta,
    #[serde(default)]
    pub blocks: Vec<Block>,
    #[serde(default)]
    pub references: Vec<serde_json::Value>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Meta {
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub title_zh: String,
    #[serde(default)]
    pub title_en: String,
    #[serde(default)]
    pub pages: Vec<serde_json::Value>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Block {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub page: usize,
    #[serde(default)]
    pub source_kind: String,
    #[serde(default)]
    pub zh: String,
    #[serde(default)]
    pub en: String,
    #[serde(default)]
    pub tex: String,
    #[serde(default)]
    pub caption_zh: String,
    #[serde(default)]
    pub caption_en: String,
    #[serde(default)]
    pub items: Vec<ListItem>,
    #[serde(default)]
    pub head: serde_json::Value,
    #[serde(default)]
    pub rows: serde_json::Value,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ListItem {
    #[serde(default)]
    pub zh: String,
    #[serde(default)]
    pub en: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Reader {
    #[serde(default)]
    pub edits: HashMap<String, Edit>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Edit {
    pub zh: Option<String>,
    #[serde(default)]
    pub base: String,
}

impl ReaderState {
    pub fn from_value(value: serde_json::Value) -> anyhow::Result<Self> {
        let captions = value["paper"]["blocks"]
            .as_array()
            .map(|blocks| {
                (0..blocks.len())
                    .filter(|&index| crate::formats::epub::is_figure_caption(blocks, index))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let mut state: Self = serde_json::from_value(value)?;
        for index in captions {
            state.paper.blocks[index].kind = "para".into();
        }
        state.validate().map_err(anyhow::Error::msg)?;
        Ok(state)
    }
    pub fn validate(&self) -> Result<(), String> {
        let mut ids = std::collections::HashSet::new();
        for block in &self.paper.blocks {
            if block.id.is_empty() || !ids.insert(&block.id) {
                return Err(format!("empty or duplicate block id: {:?}", block.id));
            }
        }
        Ok(())
    }

    pub fn title(&self) -> &str {
        if !self.paper.meta.display_name.trim().is_empty() {
            &self.paper.meta.display_name
        } else if self.paper.meta.title_zh.is_empty() {
            &self.paper.meta.title_en
        } else {
            &self.paper.meta.title_zh
        }
    }

    pub fn text<'a>(&'a self, key: &str, zh: &'a str, en: &'a str) -> &'a str {
        self.reader
            .edits
            .get(key)
            .and_then(|edit| edit.zh.as_deref())
            .unwrap_or(if zh.is_empty() { en } else { zh })
    }

    pub fn stale(&self, key: &str, zh: &str, en: &str) -> bool {
        self.reader.edits.get(key).is_some_and(|edit| {
            edit.zh.is_some()
                && !edit.base.is_empty()
                && edit.base != text_hash(if zh.is_empty() { en } else { zh })
        })
    }

    pub fn caption<'a>(&'a self, block: &'a Block) -> &'a str {
        let key = format!("{}#caption", block.id);
        // Older native imports used an extraction status as a synthetic caption.
        // Hide only that default; keep source fields and user edits for their hashes.
        if block.kind == "figure"
            && block.source_kind == "pdf-page"
            && block.caption_zh == "原页（未识别文字）"
            && block.caption_en.is_empty()
            && self
                .reader
                .edits
                .get(&key)
                .is_none_or(|edit| edit.zh.is_none())
        {
            ""
        } else {
            self.text(&key, &block.caption_zh, &block.caption_en)
        }
    }

    pub fn markdown(&self, block: &Block, bilingual: bool) -> String {
        self.block_markdown(block, |key: &str, zh: &str, en: &str| {
            let text = if matches!(block.kind.as_str(), "figure" | "table") {
                self.caption(block)
            } else {
                self.text(key, zh, en)
            };
            let original = || {
                if block.source_kind == "pdf-text" {
                    pdf_prose(en)
                } else {
                    en.to_owned()
                }
            };
            if bilingual && !en.is_empty() && text != en {
                let original = original();
                let original = if block.source_kind == "pdf-text" {
                    original.replace('\n', "\n> ")
                } else {
                    original
                };
                format!("{text}\n\n> {original}")
            } else {
                if block.source_kind == "pdf-text" && text == en {
                    original()
                } else {
                    text.to_owned()
                }
            }
        })
    }

    pub fn original_markdown(&self, block: &Block) -> String {
        // Table cells are already shown in the primary view. The companion
        // original is its English caption, not a second copy of the table.
        if block.kind == "table" {
            return block.caption_en.clone();
        }
        self.block_markdown(block, |_, _, en| {
            if block.source_kind == "pdf-text" {
                pdf_prose(en)
            } else {
                en.to_owned()
            }
        })
    }

    fn block_markdown(&self, block: &Block, pair: impl Fn(&str, &str, &str) -> String) -> String {
        match block.kind.as_str() {
            "heading" => format!("## {}", pair(&block.id, &block.zh, &block.en)),
            "para" | "note" => pair(&block.id, &block.zh, &block.en),
            "list" => block
                .items
                .iter()
                .enumerate()
                .map(|(i, item)| {
                    format!(
                        "- {}",
                        pair(&format!("{}#{i}", block.id), &item.zh, &item.en)
                    )
                })
                .collect::<Vec<_>>()
                .join("\n"),
            // Markdown exports preserve TeX; the desktop renders it using embedded Typst.
            "math" => format!("公式（TeX 源码）\n\n```latex\n{}\n```", block.tex),
            "table" => {
                let rows = normalized_rows(&block.rows);
                let heads = normalized_rows(&block.head);
                let width = heads.iter().chain(&rows).map(Vec::len).max().unwrap_or(0);
                let row = |cells: &[String]| {
                    format!(
                        "| {} |",
                        (0..width)
                            .map(|i| cells
                                .get(i)
                                .map(String::as_str)
                                .unwrap_or("")
                                .replace('|', "\\|")
                                .replace('\n', " "))
                            .collect::<Vec<_>>()
                            .join(" | ")
                    )
                };
                let mut table = pair(
                    &format!("{}#caption", block.id),
                    &block.caption_zh,
                    &block.caption_en,
                );
                if width > 0 {
                    table.push_str(&format!(
                        "\n\n{}\n{}",
                        row(heads.first().map(Vec::as_slice).unwrap_or(&[])),
                        row(&vec!["---".into(); width])
                    ));
                    for cells in heads.iter().skip(1).chain(&rows) {
                        table.push_str(&format!("\n{}", row(cells)));
                    }
                }
                table
            }
            "figure" => pair(
                &format!("{}#caption", block.id),
                &block.caption_zh,
                &block.caption_en,
            )
            .to_string(),
            "references" => format!(
                "## 参考文献\n\n{}",
                self.paper
                    .references
                    .iter()
                    .map(|r| format!(
                        "[{}] {}",
                        r["id"].as_str().unwrap_or(""),
                        r["text"].as_str().unwrap_or("")
                    ))
                    .collect::<Vec<_>>()
                    .join("\n\n")
            ),
            other => format!("暂未实现的块类型：{other}（{}）", block.id),
        }
    }
}

/// Reading presentation only; extracted text and user edit hashes stay intact.
/// Join physical PDF line wraps, preserving explicit blank paragraphs and
/// likely short paragraph endings. Do not infer semantic tables or formulas.
pub fn pdf_prose(source: &str) -> String {
    let cleaned: String = source
        .chars()
        .filter(|c| (*c == '\n' || *c == '\t' || !c.is_control()) && *c != '\u{ad}')
        .collect();
    let mut lines: Vec<_> = cleaned.lines().map(str::trim).collect();
    // PDF page blocks put the personalized publisher stamp at the end. Only
    // hide a complete trailing stamp, never a mention inside the body text.
    while lines.last().is_some_and(|line| line.is_empty()) {
        lines.pop();
    }
    if lines.last().is_some_and(|line| is_license_footer(line)) {
        lines.pop();
    }
    let typical = lines.iter().map(|s| s.chars().count()).max().unwrap_or(0);
    let mut result = String::new();
    let mut previous = "";
    let mut in_list = false;
    for (index, line) in lines.iter().copied().enumerate() {
        if line.is_empty() {
            if !result.is_empty() && !result.ends_with("\n\n") {
                result.push_str("\n\n");
            }
            previous = "";
            in_list = false;
            continue;
        }
        // A chapter or numbered section title is its own block, not a physical prose
        // wrap. Preserve both boundaries even when PDF extraction omitted
        // blank lines around it.
        let continued_title = lines
            .get(index + 1)
            .and_then(|next| next.chars().next())
            .is_some_and(char::is_lowercase);
        if let Some(level) = pdf_heading_level(line).filter(|_| !continued_title) {
            if !result.is_empty() && !result.ends_with("\n\n") {
                result.push_str("\n\n");
            }
            result.push_str(&"#".repeat(level));
            result.push(' ');
            result.push_str(line);
            result.push_str("\n\n");
            previous = "";
            in_list = false;
            continue;
        }
        if let Some(item) = pdf_unordered_item(line) {
            if !result.is_empty() && !result.ends_with("\n\n") {
                result.push_str(if in_list { "\n" } else { "\n\n" });
            }
            result.push_str("- ");
            result.push_str(item.trim_start());
            previous = line;
            in_list = true;
            continue;
        }
        // Numbered PDF items start new blocks even when the preceding item
        // fills a physical line. Keep wrapped text within the same item.
        if pdf_ordered_item(line) {
            if !result.is_empty() && !result.ends_with("\n\n") {
                result.push_str("\n\n");
            }
            result.push_str(line);
            previous = line;
            in_list = false;
            continue;
        }
        if !result.is_empty() && !result.ends_with('\n') {
            let previous = pdf_unordered_item(previous).unwrap_or(previous);
            let paragraph_end = previous.chars().count() < typical * 3 / 4
                && previous.ends_with(['.', '?', '!', '。', '？', '！']);
            if paragraph_end {
                result.push_str("\n\n");
                in_list = false;
            } else if in_list {
                result.push_str("\n  ");
            } else {
                result.push(' ');
            }
        }
        result.push_str(line);
        previous = line;
    }
    result.trim_end().to_owned()
}

/// Text-encoded PDF bullets. Vector markers are recovered from page objects
/// by the reading adapter, since they are absent from the extracted string.
pub(crate) fn pdf_unordered_item(line: &str) -> Option<&str> {
    // This publisher's symbol font maps its square bullet to ¡. A tab is
    // required so actual Spanish punctuation remains ordinary prose.
    if let Some(item) = line.strip_prefix("¡\t") {
        return (!item.trim().is_empty()).then_some(item.trim_start());
    }
    let mut chars = line.char_indices();
    let (_, marker) = chars.next()?;
    if !matches!(
        marker,
        '•' | '‣' | '◦' | '▪' | '▫' | '●' | '○' | '■' | '□' | '-' | '*' | '+'
    ) {
        return None;
    }
    let (offset, space) = chars.next()?;
    let item = &line[offset..];
    (space.is_whitespace() && !item.trim().is_empty()).then_some(item.trim_start())
}

pub(crate) fn pdf_ordered_item(line: &str) -> bool {
    let digits = line.bytes().take_while(u8::is_ascii_digit).count();
    (1..=9).contains(&digits)
        && matches!(line.as_bytes().get(digits), Some(b'.' | b')'))
        && line
            .as_bytes()
            .get(digits + 1)
            .is_some_and(u8::is_ascii_whitespace)
        && !line[digits + 1..].trim().is_empty()
}

fn pdf_heading_level(line: &str) -> Option<usize> {
    let (number, title) = line.split_once(char::is_whitespace)?;
    let (level, title) = if number.eq_ignore_ascii_case("chapter") {
        let (number, title) = title.trim_start().split_once(char::is_whitespace)?;
        let number = number.strip_suffix('.').unwrap_or(number);
        if number.is_empty()
            || number.len() > 3
            || !number.bytes().all(|c| c.is_ascii_digit())
            || number.starts_with('0')
        {
            return None;
        }
        (1, title)
    } else {
        let parts: Vec<_> = number.split('.').collect();
        if !(2..=6).contains(&parts.len())
            || parts.iter().any(|part| {
                part.is_empty()
                    || part.len() > 2
                    || !part.bytes().all(|c| c.is_ascii_digit())
                    || part.starts_with('0')
            })
        {
            return None;
        }
        (parts.len(), title)
    };
    let title = title.trim();
    // Require chapter/dotted section numbering, a short title starting with a
    // capital/uncased letter, and no sentence ending or TOC page suffix.
    // Ordinary numbered lists, decimal values and version references stay
    // in prose. Deliberately leave ambiguous long/wrapped titles alone.
    if title.chars().count() > 100
        || !title
            .chars()
            .next()
            .is_some_and(|c| c.is_alphabetic() && !c.is_lowercase())
        || title.ends_with(['.', '!', ';', ':', ',', '。', '！', '；', '：', '，', '-'])
        || title.chars().last().is_some_and(|c| c.is_ascii_digit())
        || title.chars().filter(|c| c.is_alphabetic()).count() < 3
    {
        return None;
    }
    Some(level)
}

fn is_license_footer(line: &str) -> bool {
    let Some(stamp) = line.strip_prefix("Licensed to ") else {
        return false;
    };
    let Some((name, email)) = stamp.rsplit_once(' ') else {
        return false;
    };
    let email = email
        .strip_prefix('<')
        .and_then(|email| email.strip_suffix('>'))
        .unwrap_or(email);
    !name.trim().is_empty()
        && email.split_once('@').is_some_and(|(local, domain)| {
            !local.is_empty() && domain.contains('.') && !domain.ends_with('.')
        })
}

fn normalized_rows(value: &serde_json::Value) -> Vec<Vec<String>> {
    let Some(values) = value.as_array() else {
        return vec![];
    };
    let cell = |v: &serde_json::Value| {
        v.as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| v.to_string())
    };
    if values.iter().all(|v| !v.is_array()) {
        if values.is_empty() {
            vec![]
        } else {
            vec![values.iter().map(cell).collect()]
        }
    } else {
        values
            .iter()
            .map(|v| match v.as_array() {
                Some(row) => row.iter().map(cell).collect(),
                None => vec![cell(v)],
            })
            .collect()
    }
}

/// Same FNV-1a over UTF-16 code units as easyread/store.py and util.js.
pub fn text_hash(text: &str) -> String {
    let mut hash = 0x811c9dc5u32;
    for unit in text.encode_utf16() {
        hash = (hash ^ u32::from(unit)).wrapping_mul(0x01000193);
    }
    format!("{hash:08x}")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn image_only_pdf_pages_hide_legacy_status_without_hiding_captions_or_edits() {
        let mut raw = serde_json::json!({"paper":{"meta":{},"blocks":[
            {"id":"image","type":"figure","page":1,"source_kind":"pdf-page","caption_zh":"原页（未识别文字）"},
            {"id":"text","type":"para","page":2,"source_kind":"pdf-text","en":"Figure 9-50. A decoder block of Flamingo\nFollowing body."}
        ]},"reader":{}});
        let state = ReaderState::from_value(raw.clone()).unwrap();
        let image = &state.paper.blocks[0];
        assert_eq!(state.caption(image), "");
        assert_eq!(state.markdown(image, false), "");
        assert_eq!(state.markdown(image, true), "");
        assert!(
            state
                .markdown(&state.paper.blocks[1], false)
                .contains("Figure 9-50.")
        );
        assert_eq!(image.caption_zh, "原页（未识别文字）");

        for edited in ["用户图注", "原页（未识别文字）", ""] {
            raw["reader"]["edits"] = serde_json::json!({"image#caption":{
                "zh":edited,"base":text_hash("原页（未识别文字）")
            }});
            let state = ReaderState::from_value(raw.clone()).unwrap();
            let image = &state.paper.blocks[0];
            assert_eq!(state.caption(image), edited);
            assert_eq!(state.markdown(image, false), edited);
            assert!(!state.stale("image#caption", &image.caption_zh, &image.caption_en));
        }
        raw["reader"]["edits"]["image#caption"]["zh"] = serde_json::Value::Null;
        let state = ReaderState::from_value(raw.clone()).unwrap();
        assert_eq!(state.caption(&state.paper.blocks[0]), "");

        raw["paper"]["blocks"][0]["caption_zh"] = serde_json::json!("实际图注");
        let state = ReaderState::from_value(raw.clone()).unwrap();
        assert_eq!(state.caption(&state.paper.blocks[0]), "实际图注");
        raw["paper"]["blocks"][0]["caption_zh"] = serde_json::json!("原页（未识别文字）");
        raw["paper"]["blocks"][0]["source_kind"] = serde_json::json!("pdf-image");
        let state = ReaderState::from_value(raw.clone()).unwrap();
        assert_eq!(state.caption(&state.paper.blocks[0]), "原页（未识别文字）");
        raw["paper"]["blocks"][0]["source_kind"] = serde_json::json!("pdf-page");
        raw["paper"]["blocks"][0]["caption_en"] = serde_json::json!("Real source caption");
        let state = ReaderState::from_value(raw).unwrap();
        assert_eq!(state.caption(&state.paper.blocks[0]), "原页（未识别文字）");
    }

    #[test]
    fn pdf_chapter_headings_are_separate_from_body_and_chapter_references() {
        let source = "Previous paragraph\nChapter 4. Memory\nAmong all the added modules to the augmented LLM, memory is a key component needed to go\nfrom an LLM to an agent.";
        let rendered = pdf_prose(source);
        assert_eq!(
            rendered,
            "Previous paragraph\n\n# Chapter 4. Memory\n\nAmong all the added modules to the augmented LLM, memory is a key component needed to go from an LLM to an agent."
        );
        let html = crate::annotations::html(&rendered);
        assert!(html.contains("<h1>Chapter 4. Memory</h1>\n<p><span>Among all"));
        assert_eq!(pdf_heading_level("Chapter 10. Code Agents"), Some(1));
        assert_eq!(pdf_heading_level("Chapter 4 Memory"), Some(1));
        for line in [
            "Chapter 6 adds the final ingredient, planning and reflection",
            "Chapter 4, requires optimizing both inputs and outputs.",
            "Chapter 4. Memory 187",
            "Chapter 4. Memory is a key component.",
            "Chapter 4.",
            "Chapter 04. Memory",
            "Chapter 4.. Memory",
        ] {
            assert_eq!(pdf_heading_level(line), None, "{line}");
        }
        let mut raw = serde_json::json!({"paper":{"meta":{},"blocks":[{"id":"p","type":"para","en":source,"source_kind":"pdf-text"}]},"reader":{}});
        let state = ReaderState::from_value(raw.clone()).unwrap();
        assert_eq!(state.markdown(&state.paper.blocks[0], false), rendered);
        assert_eq!(state.original_markdown(&state.paper.blocks[0]), rendered);
        assert_eq!(state.paper.blocks[0].en, source);
        raw["reader"]["edits"] =
            serde_json::json!({"p":{"zh":"用户修订","base":text_hash(source)}});
        let state = ReaderState::from_value(raw).unwrap();
        assert_eq!(state.markdown(&state.paper.blocks[0], false), "用户修订");
        assert!(
            state
                .markdown(&state.paper.blocks[0], true)
                .contains("> # Chapter 4. Memory\n> \n> Among all")
        );
        assert!(!state.stale("p", "", source));
    }

    #[test]
    fn pdf_numbered_headings_have_boundaries_before_and_after() {
        let source = "Previous paragraph wraps\nonto this line\n1.5.4 Strengthening security and compliance\nSecurity and compliance were part of the same business story.\n1.5.5 Competitive differentiation\nDynamic authorization also created opportunities for competitive differentiation.\nACME’s customers increasingly wanted software that could adapt to their structures\nand constraints.";
        let rendered = pdf_prose(source);
        let html = crate::annotations::html(&rendered);
        assert!(rendered.starts_with("Previous paragraph wraps onto this line\n\n### 1.5.4"));
        assert!(html.contains("<h3>1.5.5 Competitive differentiation</h3>\n<p>"));
        assert!(html.contains("software that could adapt to their structures and constraints."));
        assert!(!html.contains("differentiation Dynamic authorization"));
        assert_eq!(
            pdf_prose("2.1 What is digital identity?\nDigital identity connects people."),
            "## 2.1 What is digital identity?\n\nDigital identity connects people."
        );
    }

    #[test]
    fn pdf_heading_detection_preserves_numbers_and_user_edits() {
        for line in [
            "1. First numbered instruction",
            "1.5 GB available.",
            "1.5.5 is the current version",
            "1.5.5 A complete sentence.",
            "1.5.5 Competitive differentiation 18",
            "1..5 Malformed numbering",
            "10.0.1 Release notes",
        ] {
            assert_eq!(pdf_heading_level(line), None, "{line}");
        }
        assert!(!pdf_prose("2.4 Policy life cycle and\nmanagement\nBody text.").contains("##"));
        let source =
            "1.5.5 Competitive differentiation\nDynamic authorization also created opportunities.";
        let mut raw = serde_json::json!({"paper":{"meta":{},"blocks":[{"id":"p","type":"para","en":source,"source_kind":"pdf-text"}]},"reader":{}});
        let state = ReaderState::from_value(raw.clone()).unwrap();
        assert_eq!(
            state.original_markdown(&state.paper.blocks[0]),
            state.markdown(&state.paper.blocks[0], false)
        );
        assert_eq!(state.paper.blocks[0].en, source);
        raw["reader"]["edits"] =
            serde_json::json!({"p":{"zh":"用户标题\n用户正文","base":text_hash(source)}});
        let state = ReaderState::from_value(raw).unwrap();
        assert_eq!(
            state.markdown(&state.paper.blocks[0], false),
            "用户标题\n用户正文"
        );
        assert!(
            state
                .markdown(&state.paper.blocks[0], true)
                .contains("> ### 1.5.5 Competitive differentiation\n> \n> Dynamic authorization")
        );
        assert!(!state.stale("p", "", source));
    }

    #[test]
    fn pdf_ordered_lists_keep_each_item_and_wrapped_text_separate() {
        let source = "The following steps:\n1. Embed all external documents the agent has no direct access to.\n2. Embed the user’s query.\n3. Compare the embeddings and create\na similarity matrix.\n4. Return the documents with the highest similarity.\n5. Add those documents to the prompt.\nThis gives us the following class:";
        let markdown = pdf_prose(source);
        assert!(markdown.contains("\n\n2. Embed"), "{markdown}");
        assert!(markdown.contains("3. Compare the embeddings and create a similarity matrix."));
        let html = crate::annotations::html(&markdown);
        assert_eq!(html.matches("<ol>").count(), 1, "{html}");
        assert_eq!(html.matches("<li>").count(), 5, "{html}");
        assert!(html.contains("</ol>\n<p><span>This gives"), "{html}");
        for line in [
            "1.5 GB available.",
            "2.1 Section title",
            "3)example",
            "1234567890. Year",
            "4.",
        ] {
            assert!(!pdf_ordered_item(line), "{line}");
        }
        assert!(pdf_ordered_item("10) Another item."));
    }

    #[test]
    fn pdf_lists_reflow_wrapped_items_and_hide_trailing_license_stamp() {
        let source = "Introduction to the following authorization claims:\n¡\tAuthentication is not enough.\n¡\tDynamic authorization supports zero trust, compliance, delegation,\nmulti-tenancy, and AI governance.\n¡\tAuthorization is infrastructure for digital trust.\nTaken together, these claims express relationships and enforce\npolicy across applications.\nLicensed to Example Reader <reader@example.com>\n\n";
        let rendered = pdf_prose(source);
        assert_eq!(
            rendered,
            "Introduction to the following authorization claims:\n\n- Authentication is not enough.\n- Dynamic authorization supports zero trust, compliance, delegation,\n  multi-tenancy, and AI governance.\n- Authorization is infrastructure for digital trust.\n\nTaken together, these claims express relationships and enforce policy across applications."
        );
        let html = crate::annotations::html(&rendered);
        assert!(html.contains("<ul>"));
        assert_eq!(html.matches("<li>").count(), 3);
        assert!(html.contains("multi-tenancy, and AI governance.</li>"));
        assert!(html.contains("</ul>\n<p>"));
        let note = serde_json::json!({"id":"note", "kind":"highlight", "quote":"Dynamic authorization supports zero trust, compliance, delegation, multi-tenancy, and AI governance.", "color":"yellow"});
        let decorated = crate::annotations::decorate_interactive(&html, &[note]);
        assert_ne!(decorated, html);
        assert_eq!(
            crate::annotations::plain_text(&decorated),
            crate::annotations::plain_text(&html)
        );
    }

    #[test]
    fn unicode_bullets_share_the_list_path_without_reinterpreting_punctuation() {
        for marker in ["•", "‣", "◦", "▪", "▫", "●", "○", "■", "□", "-", "*", "+"]
        {
            let source = format!(
                "Introduction:\n{marker} First item\nwraps onto another line.\n{marker} Second item.\nFollowing body."
            );
            let html = crate::annotations::html(&pdf_prose(&source));
            assert_eq!(html.matches("<li>").count(), 2, "{marker}: {html}");
            assert!(
                html.contains("First item\nwraps onto another line.</li>"),
                "{html}"
            );
            assert!(
                html.contains("</ul>\n<p><span>Following body.</span></p>"),
                "{html}"
            );
        }
        for source in [
            "¡ Hola!",
            "•without separator",
            "x • y",
            "-1.5",
            "*expression",
            "+2.5",
            "▪",
        ] {
            assert!(pdf_unordered_item(source).is_none(), "{source}");
        }
    }

    #[test]
    fn pdf_cleanup_keeps_real_punctuation_and_body_license_mentions() {
        for text in [
            "¡Hola! ¡Bienvenido!",
            "¡ An actual Spanish sentence!",
            "Licensed to a company for internal use.",
            "Licensed to Example Reader <reader@example.com>\nThis is body text.",
        ] {
            assert!(!pdf_prose(text).contains("- "));
            assert!(pdf_prose(text).contains(text.lines().next().unwrap()));
        }
        assert_eq!(
            pdf_prose("Body.\nLicensed to Example Reader reader@example.com"),
            "Body."
        );
    }

    #[test]
    fn pdf_cleanup_applies_to_existing_primary_and_original_views() {
        let source = "Claims:\n¡\tFirst claim.\n¡\tSecond claim.\nLicensed to Example Reader <reader@example.com>";
        let mut raw = serde_json::json!({"paper":{"meta":{},"blocks":[{"id":"p","type":"para","en":source,"source_kind":"pdf-text"}]},"reader":{}});
        let state = ReaderState::from_value(raw.clone()).unwrap();
        let block = &state.paper.blocks[0];
        assert_eq!(
            state.markdown(block, false),
            "Claims:\n\n- First claim.\n- Second claim."
        );
        assert_eq!(state.original_markdown(block), state.markdown(block, false));
        assert_eq!(block.en, source);
        raw["reader"]["edits"] =
            serde_json::json!({"p":{"zh":"用户文本：¡\t原样保留","base":text_hash(source)}});
        let state = ReaderState::from_value(raw).unwrap();
        let block = &state.paper.blocks[0];
        assert_eq!(state.markdown(block, false), "用户文本：¡\t原样保留");
        assert!(!state.markdown(block, true).contains("Licensed to"));
        assert!(state.markdown(block, true).contains("> - First claim."));
        assert!(!state.stale("p", "", source));
    }

    #[test]
    fn pdf_reading_cleanup_preserves_source_and_user_edits() {
        let original = "Internet Iden\u{2}tity Workshop\ncontinues here.\n\nSecond paragraph.";
        let mut raw = serde_json::json!({"paper":{"meta":{},"blocks":[{"id":"p","type":"para","en":original,"source_kind":"pdf-text"}]},"reader":{}});
        let state = ReaderState::from_value(raw.clone()).unwrap();
        assert!(
            state
                .markdown(&state.paper.blocks[0], false)
                .contains("Identity Workshop continues")
        );
        assert_eq!(state.paper.blocks[0].en, original);
        raw["reader"]["edits"] =
            serde_json::json!({"p":{"zh":"用户\n\n明确分段","base":text_hash(original)}});
        let state = ReaderState::from_value(raw).unwrap();
        assert_eq!(
            state.markdown(&state.paper.blocks[0], false),
            "用户\n\n明确分段"
        );
        assert!(!state.stale("p", "", original));
    }

    #[test]
    fn preserves_reader_edit_and_detects_updated_translation() {
        let state: ReaderState =
            serde_json::from_str(include_str!("../fixtures/state.json")).unwrap();
        state.validate().unwrap();
        let block = &state.paper.blocks[1];
        assert_eq!(
            state.text(&block.id, &block.zh, &block.en),
            "这是我的修订；译者更新后仍应保留。"
        );
        assert!(state.stale(&block.id, &block.zh, &block.en));
        assert!(state.markdown(block, true).contains("> This paragraph"));
        assert_eq!(state.original_markdown(block), block.en);
    }

    #[test]
    fn utf16_hash_matches_python_golden_values() {
        // Generated independently by easyread.store.text_hash.
        for (source, expected) in [
            ("", "811c9dc5"),
            ("hello", "4f9f2cab"),
            ("中文🦀", "834807cb"),
        ] {
            assert_eq!(text_hash(source), expected);
        }
    }

    #[test]
    fn table_accepts_legacy_flat_header_and_ragged_rows() {
        let state: ReaderState =
            serde_json::from_str(include_str!("../fixtures/state.json")).unwrap();
        let table = state
            .paper
            .blocks
            .iter()
            .find(|b| b.kind == "table")
            .unwrap();
        let rendered = state.markdown(table, false);
        assert!(rendered.contains("| 方法 | 分数 |"));
        assert!(rendered.contains("| A | 0.9 |"));
        assert!(rendered.contains("| B |  |"));
        assert_eq!(state.original_markdown(table), table.caption_en);
    }
}
