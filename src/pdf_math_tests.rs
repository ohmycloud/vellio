use super::*;

fn token(ch: char, x: f32, y: f32, size: f32) -> Token {
    Token {
        index: 0,
        ch,
        x,
        right: x + size * 0.45,
        y,
        size,
        bounds: [x, y - size * 0.2, x + size * 0.45, y + size * 0.8],
        math: !ch.is_ascii_alphabetic(),
        math_roman: false,
        italic: false,
        mono: false,
        bold: false,
        calligraphic: false,
        extension: false,
    }
}
fn numbered(mut tokens: Vec<Token>) -> Vec<Token> {
    for (i, token) in tokens.iter_mut().enumerate() {
        token.index = i;
    }
    tokens
}

#[test]
fn unicode_math_alphabets_greek_and_legacy_styles_keep_meaning() {
    for (ch, tex) in [
        ('𝜔', "\\omega "),
        ('𝜃', "\\theta "),
        ('𝛼', "\\alpha "),
        ('𝑄', "Q"),
        ('𝑘', "k"),
        ('𝐱', "\\mathbf{x}"),
        ('★', "\\star "),
        ('ℎ', "h"),
    ] {
        assert_eq!(symbol(ch).as_deref(), Some(tex));
    }
    let mut t = token('L', 0., 0., 10.);
    t.calligraphic = true;
    t.math = true;
    assert_eq!(parse(&[t], &[], 0).unwrap(), "\\mathcal{L}");
}

#[test]
fn centered_argmin_limit_belongs_to_whole_operator() {
    let mut ts: Vec<_> = "argmin"
        .chars()
        .enumerate()
        .map(|(i, ch)| token(ch, 10. + i as f32 * 5., 100., 10.))
        .collect();
    let mut omega = token('𝜔', 23., 92., 7.);
    omega.math = true;
    ts.push(omega);
    let tex = parse(&numbered(ts), &[], 0).unwrap();
    assert_eq!(tex, r"\operatornamewithlimits{arg\,min}_{\omega}");
    assert!(crate::formula::layout(&tex, true).unwrap().is_valid());
}

#[test]
fn font_backed_underbrace_keeps_label_and_literal_bars_do_not_become_annotations() {
    let mut ts = vec![token('x', 20., 100., 10.), token('3', 25., 98., 7.)];
    ts[0].math = true;
    ts[0].bold = true;
    for (i, ch) in "|{z}".chars().enumerate() {
        let mut t = token(ch, 14. + i as f32 * 5., 90., 10.);
        t.extension = true;
        ts.push(t);
    }
    for (i, ch) in "query".chars().enumerate() {
        ts.push(token(ch, 14. + i as f32 * 3., 80., 7.));
    }
    let ts = numbered(ts);
    assert_eq!(structure::underbraces(&ts).len(), 1);
    let tex = parse(&ts, &[], 0).unwrap();
    assert_eq!(tex, r"\underbrace{\mathbf{x}_{3}}_{\text{query}}");
    let layout = crate::formula::layout(&tex, true).unwrap();
    assert!(
        layout.height
            > crate::formula::layout(r"\mathbf{x}_3", true)
                .unwrap()
                .height
    );
    let mut literals = ts;
    for t in &mut literals {
        t.extension = false;
    }
    assert!(structure::underbraces(&literals).is_empty());
}

#[test]
#[ignore = "requires EASYREAD_QA_MATH_BOOK pointing to Agentic Large Language Models"]
fn agentic_formulas_recover_underbraces_limits_fractions_and_all_original_letters() {
    let root = std::path::PathBuf::from(std::env::var("EASYREAD_QA_MATH_BOOK").unwrap());
    let before = std::fs::read(root.join("paper.json")).unwrap();
    let paper: serde_json::Value = serde_json::from_slice(&before).unwrap();
    let requests: Vec<_> = [49, 50, 183]
        .into_iter()
        .map(|page| {
            let block = paper["blocks"]
                .as_array()
                .unwrap()
                .iter()
                .find(|b| b["page"] == page && b["source_kind"] == "pdf-text")
                .unwrap();
            (page, block["en"].as_str().unwrap().to_owned())
        })
        .collect();
    let presentations =
        crate::pdf::translation_presentations(&root.join("source.pdf"), &requests).unwrap();
    for ((page, _), presentation) in requests.iter().zip(presentations) {
        let presentation = presentation
            .unwrap()
            .expect("Exact old extraction must still match");
        let equation = presentation
            .math
            .iter()
            .find(|m| {
                m.display
                    && match page {
                        49 => m.number.as_deref() == Some("2.10"),
                        50 => m.number.as_deref() == Some("2.11"),
                        183 => m.tex.contains("outer loop"),
                        _ => false,
                    }
            })
            .unwrap_or_else(|| {
                panic!(
                    "Missing whole display equation on page {page}: {:?}",
                    presentation.math.iter().map(|m| &m.tex).collect::<Vec<_>>()
                )
            });
        let expected = match page {
            49 => vec![
                r"\mathbf{a}_{3}",
                r"\sum _{j=1}^{5}",
                r"\alpha _{3j}",
                r"\mathbf{x}_{j}",
                r"\text{query}",
                r"\text{key}",
                r"\text{value}",
            ],
            50 => vec![
                r"\operatorname{Attention}",
                "Q,K,V",
                r"\frac{QK^{T}}{\sqrt{d_{k}}}",
            ],
            183 => vec![
                r"\omega ^{\star}",
                r"\operatornamewithlimits{arg\,min}_{\omega}",
                r"\theta _{i}",
                r"\mathcal{L}_{\omega}",
                "D_{i,train}",
                r"\text{outer loop}",
                r"\text{inner loop}",
            ],
            _ => unreachable!(),
        };
        for expected in expected {
            assert!(
                equation.tex.contains(expected),
                "page {page}: {} missing {expected}",
                equation.tex
            );
        }
        assert_eq!(
            equation.tex.matches("\\underbrace").count(),
            if *page == 49 {
                3
            } else if *page == 183 {
                2
            } else {
                0
            }
        );
        assert!(equation.layout.is_valid() && !equation.layout.paths.is_empty());
        assert!(presentation.html.contains("data-vellio-math="));
        assert!(!presentation.html.contains("|{z}"));
        if *page == 50 {
            assert!(
                presentation.math.iter().all(|m| !m.tex.contains("d_{k}c")),
                "A radical cannot consume the next prose word"
            );
        }
    }
    assert_eq!(std::fs::read(root.join("paper.json")).unwrap(), before);
}
