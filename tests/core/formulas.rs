use super::*;

#[test]
fn native_formula_corpus_and_invalid_input() {
    for tex in [
        r"e^{i\pi}+1=0",
        r"\mathcal{L}(\theta)=\sum_{i=1}^{n}\log p_\theta(x_i)",
        r"\frac{\partial}{\partial x}\sum_{i=1}^n x_i^2",
        r"\begin{pmatrix}a&b\\c&d\end{pmatrix}",
        r"f(x)=\begin{cases}x^2&x\ge 0\\-x&x<0\end{cases}",
        r"\begin{aligned}a&=b+c\\d&=e+f\end{aligned}",
    ] {
        let svg = formula::svg(tex, true).unwrap_or_else(|e| panic!("{tex}: {e:#}"));
        assert!(svg.contains("<svg"));
        assert!(svg.len() > 500);
        for display in [false, true] {
            let layout =
                formula::layout(tex, display).unwrap_or_else(|e| panic!("native {tex}: {e:#}"));
            assert!(layout.is_valid());
            assert!(layout.width > 10. && layout.height > 10.);
            assert!(
                layout
                    .paths
                    .iter()
                    .any(|p| p.commands.iter().any(|c| matches!(
                        c,
                        formula::PathCommand::Quad(..) | formula::PathCommand::Cubic(..)
                    )))
            );
            assert!(std::sync::Arc::ptr_eq(
                &layout,
                &formula::cached_layout(tex, display).unwrap()
            ));
            let json = serde_json::to_string(&*layout).unwrap();
            assert!(
                !json.contains("<svg") && !json.contains("data:image") && !json.contains("png")
            );
            let restored: formula::FormulaLayout = serde_json::from_str(&json).unwrap();
            assert!(restored.is_valid());
            assert_eq!(restored.paths.len(), layout.paths.len());
        }
    }
    assert!(formula::layout("", true).is_err());
    assert!(formula::layout(r"\nonexistentcommand{x}", false).is_err());
    assert!(formula::svg("", true).is_err());
    assert!(formula::svg(r"\nonexistentcommand{x}", true).is_err());
}

#[test]
fn inline_formula_scanner_preserves_code_and_escaped_dollars() {
    let text = r"中文 $e^{i\pi}+1=0$ and `code $x$` and \$cash and $$x^2$$ and $\frac{1}{2}$";
    let ranges = formula::inline_math(text);
    assert_eq!(
        ranges.iter().map(|(_, t)| *t).collect::<Vec<_>>(),
        vec![r"e^{i\pi}+1=0", r"\frac{1}{2}"]
    );
    let transformed = formula::native_markdown_html(text);
    assert_eq!(transformed.matches("data-vellio-math").count(), 3);
    assert!(transformed.contains("data-display=\"true\""));
    assert!(!transformed.contains("<img") && !transformed.contains("data:image"));
    assert!(transformed.contains("<code>code $x$</code>"));
    let plain = easyread_gpui::annotations::plain_text(&transformed);
    assert!(plain.contains(r"$\frac{1}{2}$") && plain.contains("$cash"));
    let malicious = formula::native_markdown_html(
        r#"Before $x<y$ <script>alert(1)</script> $\text{\"quoted\"}_k$"#,
    );
    assert!(!malicious.contains("<script>"));
    assert!(malicious.contains("x&lt;y") && malicious.contains("&quot;quoted"));
    assert!(
        formula::inline_math("```latex\n$x$\n```\n中文 $y$")
            .iter()
            .all(|(_, t)| *t == "y")
    );
}
