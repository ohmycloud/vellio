use super::source::math_quote_range;
use super::*;

#[test]
fn native_paragraph_scope_keeps_repeated_unicode_paragraphs_and_partial_selections_local() {
    let html = "<p>开头。</p><p>重复 <strong>段落</strong>。</p><blockquote><p>English companion.</p></blockquote><p>重复 <em>段落</em>。</p><p>结尾。</p>";
    let text = plain_text(html);
    let native = "开头。\n重复 段落。\nEnglish companion.\n重复 段落。\n结尾。\n";
    let start = native.match_indices("重复 段落。").nth(1).unwrap().0;
    let paragraph = start..start + "重复 段落。".len();
    let (quote, range) = rendered_paragraph_quote(&text, native, paragraph.clone(), None).unwrap();
    assert_eq!(quote, "重复 段落。");
    assert_eq!(
        range.start,
        text.match_indices("重复 段落。").nth(1).unwrap().0
    );
    let (quote, range) = rendered_paragraph_quote(&text, native, paragraph, Some("段落")).unwrap();
    assert_eq!(quote, "段落");
    assert_eq!(range.start, text.match_indices("段落").nth(1).unwrap().0);
}

#[test]
fn unproven_native_scope_does_not_fall_back_to_the_page_or_first_duplicate() {
    assert!(
        rendered_paragraph_quote(
            "Same.\nSame.",
            "Missing lead.\nSame.\nMissing tail.",
            14..19,
            None
        )
        .is_none()
    );
    assert!(rendered_paragraph_quote("Page text", "", 0..0, None).is_none());
    assert!(rendered_paragraph_quote("Page text", "Native", 0..99, None).is_none());
    assert!(quote_range_in_context("Same. Same.", "Same.", "Missing", "").is_none());
    // Existing saved-note re-anchoring remains backward compatible.
    assert_eq!(
        quote_range("Same. Same.", "Same.", "Missing", ""),
        Some(0..5)
    );
}

#[test]
fn typography_matches_wrapped_identifiers_and_repeated_unicode_with_context() {
    let html =
        "<p>Tools first; 更新 Tools second.</p><p>Reference https://example.org/longpath.</p>";
    let styled = source_typography(html, "Tools", "更新 ", " second.", (true, false, true));
    assert!(styled.contains("Tools first; 更新 <strong><code>Tools</code></strong> second."));
    let styled = source_typography(
        &styled,
        "https://example.org/long\u{2}\npath",
        "Reference ",
        ".",
        (false, true, false),
    );
    assert!(styled.contains("<em>https://example.org/longpath</em>"));
    assert_eq!(plain_text(html), plain_text(&styled));
}
use serde_json::json;

#[test]
fn repeated_reference_numbers_use_reflowed_context_before_first_match() {
    let html = "<p>18 Wu et al. 2025. First paper.</p><p>20 Zhang et al. 2022. OPT.</p><p>22 Alayrac et al. 2022. Flamingo.</p>";
    let linked = source_link(
        html,
        "20",
        "First paper.\n",
        " Zhang et al. 2022.",
        "easyread-pdf:1:100",
    );
    let marked = source_superscript(&linked, "20", "First paper.\n", " Zhang et al. 2022.");
    let marked = source_superscript(&marked, "22", "OPT.\n", " Alayrac et al. 2022.");
    assert!(
        marked.contains("<sup>20</sup></span></a> Zhang"),
        "{marked}"
    );
    assert!(marked.contains("<p><sup>22</sup> Alayrac"), "{marked}");
    assert!(marked.contains("2025. First"), "{marked}");
    assert!(marked.contains("2022. OPT"), "{marked}");
    assert_eq!(plain_text(&marked), plain_text(html));
}

#[test]
fn source_link_keeps_boundary_spaces_outside_anchors() {
    for (source, quote, expected) in [
        (
            "<p>the website next</p>",
            " website ",
            ">website</span></a> next",
        ),
        (
            "<p>the <em> website </em> next</p>",
            " website ",
            ">website</span></a> </em>",
        ),
        (
            "<p>the&#32;web &amp; docs&#32;next</p>",
            " web & docs ",
            ">web &amp; docs</span></a>&#32;next",
        ),
        (
            "<p>the\u{2003}网址\u{2003}next</p>",
            "\u{2003}网址\u{2003}",
            ">网址</span></a>\u{2003}next",
        ),
    ] {
        let linked = source_link(source, quote, "", "", "https://example.org");
        assert!(linked.contains(expected), "{linked}");
        assert_eq!(plain_text(&linked), plain_text(source));
    }
}

#[test]
fn pdf_links_cross_markup_and_use_repeated_unicode_quote_context() {
    let source = "<p><span>First 图 4-1, then <em>图 4</em>-1.</span></p>";
    let linked = source_link(source, "图 4-1", "then ", ".", "easyread-pdf:187:511");
    assert!(linked.contains("First 图 4-1, then <em><a href=\"easyread-pdf:187:511\""));
    assert!(linked.contains("</span></a></em><a href="));
    assert_eq!(plain_text(source), plain_text(&linked));
    let marked = decorate_interactive(
        &linked,
        &[
            serde_json::json!({"id":"note", "quote":"图 4-1", "prefix":"then ", "suffix":".", "kind":"highlight"}),
        ],
    );
    assert_eq!(plain_text(&marked), plain_text(source));
    assert!(marked.contains("easyread-pdf:187:511"));
}

#[test]
fn code_annotations_preserve_newlines_indentation_and_source_colors() {
    let source = "<pre><code><span style=\"color:#000088\">messages</span> = [\n    <span style=\"color:#ff3300\">&quot;中文&quot;</span>,\n]\n</code></pre><p>After</p>";
    let text = plain_text(source);
    assert!(text.contains("messages = [\n    \"中文\",\n]"));
    let quote = "messages = [\n    \"中文\",";
    let note = json!({"id":"code-note", "quote":quote,"color":"green","style":"underline"});
    let result = decorate_interactive(source, &[note]);
    assert_eq!(plain_text(&result), text);
    assert!(result.contains("color:#000088"));
    assert!(result.contains("color:#ff3300"));
    assert!(result.contains("data-easyread-note=\"code-note\""));
    assert!(result.contains("text-decoration-color:#58994c"));
}

#[test]
fn margin_editor_locates_unsaved_quote_and_keeps_anchor_after_save() {
    let source = html("Early quote.\n\nLater **quote & 中文** at the end.");
    let draft = json!({"id":"new-note", "quote":"quote & 中文",
        "prefix":"Later ", "suffix":" at the end.", "body":"", "style":"underline"});
    let anchored = margin_editor_anchor(&source, &draft);
    let marker = "<span data-easyread-note=\"reader-margin-editor-target\">q</span>";
    assert!(anchored.contains(&format!("Later <strong>{marker}uote &amp; 中文</strong>")));
    assert_eq!(plain_text(&anchored), plain_text(&source));
    assert!(!anchored.contains("<u"));
    assert!(!anchored.contains("<mark"));

    let mut saved = draft.clone();
    saved["body"] = json!("Saved note");
    let decorated = decorate_interactive(&source, &[saved.clone()]);
    let editing_saved = margin_editor_anchor(&decorated, &saved);
    assert!(editing_saved.contains(marker));
    assert_eq!(plain_text(&editing_saved), plain_text(&source));
    assert!(editing_saved.contains("new-note"));

    let attached = json!({"id":"attached", "quote":"quote & 中文",
        "prefix":"Later ", "decoration":"none", "source_annotation":"new-note"});
    assert!(margin_editor_anchor(&decorated, &attached).contains(marker));
    assert_eq!(
        margin_editor_anchor(&source, &json!({"quote":"missing"})),
        source
    );
    assert_eq!(margin_editor_anchor(&source, &json!({"quote":""})), source);
}

#[test]
fn navigation_anchor_preserves_markup_and_uses_repeated_quote_context() {
    let source = html("first & then **& 中文** end");
    let note = json!({"quote":"& 中文", "prefix":"then ", "suffix":" end", "decoration":"none"});
    let anchored = navigation_anchor(&source, &note);
    assert_eq!(plain_text(&anchored), plain_text(&source));
    assert!(anchored.contains(
        "<strong><span data-easyread-note=\"reader-navigation-target\">&amp;</span> 中文</strong>"
    ));
    assert!(!anchored.contains("<mark"));
    assert_eq!(
        navigation_anchor(&source, &json!({"quote":"missing"})),
        source
    );
    assert_eq!(navigation_anchor(&source, &json!({"quote":""})), source);
}

#[test]
fn legacy_colors_restore_and_latest_overlap_wins_without_losing_underline() {
    let notes = [
        json!({"id":"red","quote":"bc","color":"pink","updated":"2026-10-03T00:00:02Z"}),
        json!({"id":"green","quote":"abc","color":"green","updated":"2026-10-03T00:00:01Z"}),
        json!({"id":"underline","quote":"b","style":"underline"}),
    ];
    let result = decorate(&html("abc"), &notes);
    assert!(result.contains("#d4e3c9\">a</mark>"));
    assert!(result.contains("#f0cec7\"><u>b</u>"));
    assert_eq!(plain_text(&result), "abc");
    let reversed: Vec<_> = notes.into_iter().rev().collect();
    assert_eq!(result, decorate(&html("abc"), &reversed));
    assert!(
        decorate(&html("word"), &[json!({"quote":"word","color":"unknown"})]).contains("#f2df8a")
    );
}

#[test]
fn interactive_marks_keep_identity_across_links_emphasis_and_overlap() {
    let source = html("前 **选中** [文字](https://example.org) 后");
    let notes = [
        json!({"id":"old", "kind":"highlight", "quote":"选中 文字", "color":"green", "updated":"2026-10-03T00:00:01Z"}),
        json!({"id":"new", "kind":"highlight", "quote":"文字", "style":"underline", "color":"blue", "updated":"2026-10-03T00:00:02Z"}),
        json!({"id":"deleted", "quote":"前", "deleted":true}),
        json!({"id":"question", "kind":"question", "quote":"后", "color":"yellow"}),
    ];
    let decorated = decorate_interactive(&source, &notes);
    assert!(decorated.contains("data-easyread-note=\"old\""));
    assert!(decorated.contains("data-easyread-note=\"new\""));
    assert!(!decorated.contains("data-easyread-note=\"deleted\""));
    assert!(decorated.contains("data-easyread-note=\"question\""));
    assert!(decorated.contains("href=\"https://example.org\""));
    assert_eq!(plain_text(&decorated), plain_text(&source));
    assert!(!decorate(&source, &notes).contains("data-easyread-note"));
}

#[test]
fn colored_underlines_keep_independent_highlights_and_latest_line_color() {
    let source = html("前文 **选中文字** & 后文");
    let mut notes = vec![
        json!({"quote":"选中文字 & 后文", "color":"green"}),
        json!({"id":"old", "quote":"选中文字 & 后文", "style":"underline", "color":"yellow", "updated":"2026-10-03T00:00:01Z"}),
        json!({"id":"new", "quote":"中文字", "style":"underline", "color":"blue", "updated":"2026-10-03T00:00:02Z"}),
    ];
    let result = decorate(&source, &notes);
    assert!(result.contains("text-decoration-color:#477fc1\">中文字</u>"));
    assert!(result.contains("text-decoration-color:#dca722\">选</u>"));
    assert!(result.contains("<mark color=\"#d4e3c9\">"));
    assert_eq!(plain_text(&result), plain_text(&source));
    notes.reverse();
    assert_eq!(decorate(&source, &notes), result);
    for color in HighlightColor::ALL {
        let result = decorate(
            &source,
            &[json!({"quote":"选中文字", "style":"underline", "color":color.name()})],
        );
        assert!(result.contains(&format!("text-decoration-color:#{:06x}", color.line_rgb())));
        assert!(!result.contains("<mark"));
    }
}

#[test]
fn visible_selection_matches_pdf_line_breaks_and_keeps_source_offsets() {
    let source = html("开头\nPDF   text\nwith **emphasis** 结束");
    let text = plain_text(&source);
    let range = quote_range(&text, "开头 PDF text with emphasis", "", "").unwrap();
    assert_eq!(&text[range.clone()], "开头\nPDF   text\nwith emphasis");
    assert_eq!(context(&text, range), (String::new(), " 结束".into()));
    let marked = decorate(&source, &[json!({"quote":"开头 PDF text with emphasis"})]);
    assert!(marked.contains("<strong><mark"));
    assert_eq!(plain_text(&marked), text);
    let repeated = "same\nquote then same\nquote";
    assert_eq!(
        quote_range(repeated, "same quote", "then ", ""),
        Some(16..26)
    );
    assert_eq!(quote_range("无关正文", "未选中", "", ""), None);
    let marked = decorate(
        &html("Native Markdown supports prose"),
        &[json!({"quote":"Markdown "})],
    );
    assert!(marked.contains("Markdown</mark> supports"));
}

#[test]
fn recovered_math_uses_surviving_context_when_page_footer_is_removed() {
    let html = "<p>Small <em>dk</em> values. Scale by <em>√</em> <sup>1</sup> dk .</p><p>Mean and variance <em>dk</em>.</p>";
    let edits = vec![
        (
            "dk".into(),
            "Small ".into(),
            " values. Scale".into(),
            crate::formula::native_html("d_{k}", false),
        ),
        (
            "√\r\n1\r\ndk".into(),
            "Scale by ".into(),
            "\r\n.\r\nMean and variance".into(),
            crate::formula::native_html(r"\frac{1}{\sqrt{d_{k}}}", false),
        ),
        (
            "dk".into(),
            "Mean and variance ".into(),
            ".\r\n4".into(),
            crate::formula::native_html("d_{k}", false),
        ),
    ];
    let result = source_math_edits(html, &edits);
    assert_eq!(result.matches("data-vellio-math=").count(), 3, "{result}");
    assert!(result.contains(r"\frac{1}{\sqrt{d_{k}}}"));
    assert!(!result.contains("<sup>"));
    assert!(result.contains("Mean and variance <em><span data-vellio-math="));
    // Reordering the input edits must not change their source locations.
    assert_eq!(
        source_math_edits(html, &edits.into_iter().rev().collect::<Vec<_>>()),
        result
    );
}

#[test]
fn recovered_math_drops_pdf_script_shift_but_retains_adjacent_footnotes() {
    let html = "<p>Use <em><sup>dk</sup></em> and <sup>4</sup> footnote.</p>";
    let object = crate::formula::native_html("d_{k}", false);
    let result = source_math_edits(
        html,
        &[("dk".into(), "Use ".into(), " and ".into(), object.clone())],
    );
    assert_eq!(
        result,
        format!("<p>Use <em>{object}</em> and <sup>4</sup> footnote.</p>")
    );
    assert_eq!(math_quote_range("dk and dk", "dk", "", ""), None);
    assert_eq!(math_quote_range("dk and dk", "", "", ""), None);
    assert_eq!(math_quote_range("dk和dk", "dk", "", "和dk"), Some(0..2));
    assert_eq!(math_quote_range("dk和dk", "dk", "dk和", ""), Some(5..7));
}

#[test]
fn native_formula_objects_survive_html_conversion_and_decoration() {
    let source = crate::formula::native_markdown_html(
        r"行内公式 $x_k$ 与中文混排。 [link](javascript:alert(1))",
    );
    let marked = decorate(
        &source,
        &[json!({"quote":"与中文混排", "style":"underline"})],
    );
    assert!(marked.contains("data-vellio-math=\"x_k\""));
    assert!(!marked.contains("<img") && !marked.contains("data:image"));
    assert_eq!(plain_text(&marked), plain_text(&source));
    assert!(marked.contains("<u>与中文混排</u>"));
    assert!(marked.contains("href=\"\""));
}

#[test]
fn annotations_cross_emphasis_and_entities_without_changing_text() {
    let source = html("前文 **selected** & 后文 🦀");
    let marked = decorate(
        &source,
        &[json!({"quote":"selected & 后文", "style":"underline"})],
    );
    assert!(marked.contains("<strong><u>selected</u></strong>"));
    assert!(marked.contains("<u> &amp; 后文</u>"));
    assert_eq!(plain_text(&marked), plain_text(&source));
}

#[test]
fn attached_margin_note_does_not_cover_the_source_underline() {
    let source = html("words");
    let marked = decorate_interactive(
        &source,
        &[
            json!({"id":"mark","quote":"words","style":"underline","kind":"highlight","color":"blue"}),
            json!({"id":"note","quote":"words","kind":"note","color":"yellow","decoration":"none"}),
        ],
    );
    assert!(!marked.contains("<mark"));
    assert!(marked.contains("#477fc1"));
    assert!(marked.contains("data-easyread-note=\"mark\""));
}

#[test]
fn repeated_quote_context_and_overlapping_marks_survive() {
    let source = html("first word then word end");
    let marked = decorate(
        &source,
        &[
            json!({"quote":"word", "prefix":"then ", "suffix":" end"}),
            json!({"quote":"word end", "style":"underline", "prefix":"then "}),
            json!({"quote":"first", "deleted":true}),
        ],
    );
    assert!(marked.contains("first word then <mark"));
    assert!(marked.contains("<u>word</u></mark><u> end</u>"));
    assert_eq!(plain_text(&marked), "first word then word end");
}
