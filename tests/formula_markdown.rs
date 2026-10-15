//! Shared Ask AI / reader Markdown math, without rewriting stored responses.
use easyread_gpui::{annotations, formula};

const ATTENTION: &str =
    r"\text{Attention}(Q,K,V)=\text{softmax}\!\left(\frac{QK^{\top}}{\sqrt{d_k}}\right)V";

#[test]
fn screenshot_attention_display_and_inline_formulas_share_native_rendering() {
    let source = format!(
        "原文（第 4 页，式 (1)）写成矩阵形式：\n$${ATTENTION}$$\n逐项对应：\n\n- 输入：$d_k$ 和 $d_v$\n- 每个分数除以 $\\sqrt{{d_k}}$。"
    );
    let spans = formula::math_spans(&source);
    assert_eq!(spans.len(), 4);
    assert_eq!(spans[0].tex, ATTENTION);
    assert!(spans[0].display);
    assert!(spans[1..].iter().all(|m| !m.display));
    for span in &spans {
        formula::layout(&span.tex, span.display).unwrap();
    }
    let html = formula::native_markdown_html(&source);
    assert_eq!(html.matches("data-vellio-math=").count(), 4);
    assert_eq!(html.matches("data-display=\"true\"").count(), 1);
    assert!(html.contains("<p style=\"text-align:center\"><span><span data-vellio-math="));
    assert!(!html.contains("$$") && !html.contains("<img"));
    assert!(html.contains("逐项对应") && html.contains("<li>"));
}

#[test]
fn dollar_and_latex_delimiters_keep_display_semantics_and_utf8_offsets() {
    for (formula, display) in [
        ("$d_k$", false),
        ("$$d_k$$", true),
        ("$$\nd_k\n$$", true),
        (r"\(d_k\)", false),
        (r"\[d_k\]", true),
        ("\\[\nd_k\n\\]", true),
    ] {
        let source = format!("中文：\n\n{formula}\n\n后文。");
        let spans = formula::math_spans(&source);
        assert_eq!(spans.len(), 1, "{source}");
        assert_eq!(spans[0].tex, "d_k");
        assert_eq!(spans[0].display, display);
        assert_eq!(&source[spans[0].range.clone()], formula);
        let html = formula::native_markdown_html(&source);
        assert_eq!(html.matches("data-vellio-math=").count(), 1, "{html}");
        assert!(html.contains("后文。"));
    }
}

#[test]
fn code_examples_escaped_delimiters_and_incomplete_streams_remain_literal() {
    for source in [
        r"`$x$ $$x$$ \(x\) \[x\]`",
        r"``a ` $x$ $$x$$ \[x\]``",
        "```latex\n$$x$$\n\\[y\\]\n```",
        "~~~latex\n$x$\n\\(y\\)\n~~~",
        "    $$x$$\n    \\[y\\]",
        r"\$x\$ \\(x\\) \\[x\\]",
        "$$\nx^2\nnot finished",
        r"\[x^2",
        r"\(x^2",
        r"$x",
        r"$$x",
    ] {
        assert!(formula::math_spans(source).is_empty(), "{source}");
        assert!(
            !formula::native_markdown_html(source).contains("data-vellio-math="),
            "{source}"
        );
    }
    let source = "~~~latex\n$$example$$\n~~~\n\n真正的公式 $x^2$。";
    assert_eq!(formula::math_spans(source).len(), 1);
    assert!(
        formula::native_markdown_html(source)
            .contains("<code class=\"language-latex\">$$example$$")
    );
    assert_eq!(formula::math_spans("$$\nx^2\n$$")[0].tex, "x^2");
}

#[test]
fn display_math_retains_lists_quotes_and_surrounding_paragraphs() {
    for source in [
        "> 前文\n> $$\n> x^2\n> $$\n> 后文",
        "> 前文 $$x^2$$ 后文",
        "- 前文 $$x^2$$ 后文\n- 下一个项目",
        "1. 前文 $$x^2$$ 后文\n2. 下一个项目",
        "> 前文\n> \\[\n> x^2\n> \\]\n> 后文",
        "- 前文\n  \\[\n  x^2\n  \\]\n  后文\n- 下一个项目",
        "- 前文\n  $$\n  x^2\n  $$\n  后文\n- 下一个项目",
    ] {
        assert_eq!(formula::math_spans(source)[0].tex, "x^2");
        let html = formula::native_markdown_html(source);
        let normalized = easyread_gpui::translation_layout::xhtml(&html);
        let wrapped = format!("<root>{normalized}</root>");
        let doc = roxmltree::Document::parse(&wrapped).unwrap();
        let math = doc
            .descendants()
            .find(|n| n.attribute("data-vellio-math").is_some())
            .unwrap();
        let container = if source.starts_with('>') {
            "blockquote"
        } else {
            "li"
        };
        assert!(
            math.ancestors().any(|n| n.has_tag_name(container)),
            "{html}"
        );
        assert!(
            math.ancestors()
                .any(|n| n.attribute("style") == Some("text-align:center")),
            "{html}"
        );
        let text = annotations::plain_text(&html);
        assert!(text.contains("前文") && text.contains("后文"), "{text}");
        assert_eq!(html.matches("data-vellio-math=").count(), 1, "{html}");
    }
}

#[test]
fn tex_is_escaped_not_interpreted_as_markdown_or_source_html() {
    let source = r#"Before $$x<y+\text{"quoted"}_k$$ <script>alert(1)</script> after $d_k$"#;
    let html = formula::native_markdown_html(source);
    assert_eq!(html.matches("data-vellio-math=").count(), 2);
    assert!(!html.contains("<script>"));
    assert!(html.contains("x&lt;y") && html.contains("&quot;quoted&quot;"));
    assert!(!html.contains("<em>k"));
    assert_eq!(formula::math_spans(r"\[x+\text{$y$}\]").len(), 1);
}
