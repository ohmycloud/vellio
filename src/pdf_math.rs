//! Reconstruct math structure from PDF glyph positions and mathematical fonts.
//! Fractions, radicals and scripts become TeX, then Typst's native vector frame.
//! No page rasterization, bitmap crops or SVG image elements.
use super::Glyph;
use pdfium_render::prelude::*;
use std::{
    collections::{HashMap, HashSet},
    ops::Range,
};

#[derive(Clone)]
struct Token {
    index: usize,
    ch: char,
    x: f32,
    right: f32,
    y: f32,
    size: f32,
    bounds: [f32; 4],
    math: bool,
    math_roman: bool,
    italic: bool,
    mono: bool,
}
#[derive(Clone, Copy)]
struct Rule([f32; 4]);
impl Rule {
    fn y(self) -> f32 {
        (self.0[1] + self.0[3]) * 0.5
    }
}
#[derive(Clone)]
struct Atom {
    x: f32,
    right: f32,
    y: f32,
    size: f32,
    tex: String,
    word: bool,
}

pub(super) struct ReadingMath {
    pub range: Range<usize>,
    pub lines: Vec<usize>,
    pub presentation: crate::pdf::MathPresentation,
}

fn key(x: f32, y: f32, ch: char) -> (i32, i32, char) {
    ((x * 100.).round() as i32, (y * 100.).round() as i32, ch)
}
fn tokens(page: &PdfPage<'_>, glyphs: &[Glyph]) -> anyhow::Result<Vec<Token>> {
    let text = page.text()?;
    let mut metadata = HashMap::new();
    for g in text.chars().iter() {
        let Some(ch) = g
            .unicode_char()
            .filter(|ch| !ch.is_whitespace() && !ch.is_control())
        else {
            continue;
        };
        let Ok((x, y)) = g.origin() else { continue };
        let font = g.font_name().to_ascii_lowercase();
        let Ok(b) = g.tight_bounds() else { continue };
        let math = ["cmmi", "cmsy", "cmex", "cmmib", "math", "msam", "msbm"]
            .iter()
            .any(|s| font.contains(s));
        // Known Computer Modern extension encodings sometimes expose legacy
        // Latin code points instead of Unicode mathematical operators.
        let mapped = if font.contains("cmex") {
            match ch {
                'P' => '∑',
                'Q' => '∏',
                'R' => '∫',
                _ => ch,
            }
        } else {
            ch
        };
        metadata.insert(
            key(x.value, y.value, ch),
            (
                mapped,
                math,
                font.contains("cmr"),
                [
                    b.left().value,
                    b.bottom().value,
                    b.right().value,
                    b.top().value,
                ],
            ),
        );
    }
    Ok(glyphs
        .iter()
        .enumerate()
        .filter_map(|(index, g)| {
            let &(ch, math, math_roman, bounds) = metadata.get(&key(g.x, g.y, g.ch))?;
            Some(Token {
                index,
                ch,
                x: g.x,
                right: g.right,
                y: g.y,
                size: g.size,
                bounds,
                math,
                math_roman,
                italic: g.italic,
                mono: g.mono,
            })
        })
        .collect())
}
fn rules(page: &PdfPage<'_>) -> Vec<Rule> {
    page.objects()
        .iter()
        .filter_map(|o| {
            o.as_path_object()?;
            let b = o.bounds().ok()?;
            (b.width().value >= 2. && b.height().value <= 1.5).then_some(Rule([
                b.left().value,
                b.bottom().value,
                b.right().value,
                b.top().value,
            ]))
        })
        .collect()
}
fn root_roof(root: &Token, r: Rule) -> bool {
    root.ch == '√'
        && r.0[0] >= root.x + root.size * 0.5
        && r.0[0] <= root.bounds[2] + root.size * 0.5
        && (r.y() - root.bounds[3]).abs() <= root.size * 0.4
}
fn fraction_parts(ts: &[Token], r: Rule) -> Option<(Vec<Token>, Vec<Token>)> {
    let local_size = ts
        .iter()
        .filter(|t| {
            t.x >= r.0[0] - 4.
                && t.x <= r.0[2] + 4.
                && (t.y - r.y()).abs() < 32.
                && (t.math || t.italic || t.ch == '√' || t.ch.is_ascii_digit())
        })
        .map(|t| t.size)
        .fold(0., f32::max);
    let size = if local_size > 0. {
        local_size
    } else {
        ts.iter().map(|t| t.size).fold(0., f32::max)
    };
    if size <= 0. || r.0[2] - r.0[0] < size * 0.45 || ts.iter().any(|t| root_roof(t, r)) {
        return None;
    }
    let eligible = |t: &&Token| {
        t.x >= r.0[0] - size * 0.25
            && t.x <= r.0[2] + size * if t.size < size * 0.9 { 0.5 } else { 0.1 }
            && t.right <= r.0[2] + size * 0.6
            && (t.y - r.y()).abs() <= size * 1.8
    };
    let up: Vec<_> = ts
        .iter()
        .filter(eligible)
        .filter(|t| t.y > r.y() + size * 0.12)
        .cloned()
        .collect();
    let down: Vec<_> = ts
        .iter()
        .filter(eligible)
        .filter(|t| {
            let y = if t.ch == '√' {
                t.bounds[1] + t.size * 0.25
            } else {
                t.y
            };
            y < r.y() - size * if "()".contains(t.ch) { 0.6 } else { 0.3 }
        })
        .cloned()
        .collect();
    (!up.is_empty() && !down.is_empty()).then_some((up, down))
}
fn dominant_y(atoms: &[Atom], size: f32) -> f32 {
    let mut bins: Vec<(f32, usize)> = vec![];
    for a in atoms.iter().filter(|a| a.size >= size * 0.9) {
        if let Some(b) = bins.iter_mut().find(|b| (b.0 - a.y).abs() <= size * 0.12) {
            b.1 += 1;
        } else {
            bins.push((a.y, 1));
        }
    }
    bins.into_iter().max_by_key(|b| b.1).map_or(0., |b| b.0)
}
fn symbol(ch: char) -> Option<String> {
    let named = match ch {
        'α' => "alpha",
        'β' => "beta",
        'γ' => "gamma",
        'δ' => "delta",
        'ε' | 'ϵ' => "epsilon",
        'ζ' => "zeta",
        'η' => "eta",
        'θ' => "theta",
        'ι' => "iota",
        'κ' => "kappa",
        'λ' => "lambda",
        'μ' => "mu",
        'ν' => "nu",
        'ξ' => "xi",
        'π' => "pi",
        'ρ' => "rho",
        'σ' => "sigma",
        'τ' => "tau",
        'υ' => "upsilon",
        'φ' => "phi",
        'χ' => "chi",
        'ψ' => "psi",
        'ω' => "omega",
        'Γ' => "Gamma",
        'Δ' => "Delta",
        'Θ' => "Theta",
        'Λ' => "Lambda",
        'Ξ' => "Xi",
        'Π' => "Pi",
        'Σ' => "Sigma",
        'Φ' => "Phi",
        'Ψ' => "Psi",
        'Ω' => "Omega",
        '∑' => "sum",
        '∏' => "prod",
        '∫' => "int",
        '∞' => "infty",
        '∂' => "partial",
        '∇' => "nabla",
        '±' => "pm",
        '≤' => "leq",
        '≥' => "geq",
        '≠' => "neq",
        '≈' => "approx",
        '∈' => "in",
        '∉' => "notin",
        '⊂' => "subset",
        '⊆' => "subseteq",
        '×' => "times",
        '÷' => "div",
        '·' => "cdot",
        '→' => "to",
        'ℓ' => "ell",
        'ℝ' => "mathbb{R}",
        _ => "",
    };
    if !named.is_empty() {
        return Some(format!("\\{named} "));
    }
    match ch {
        '−' | '–' => Some("-".into()),
        ch if ch.is_ascii_alphanumeric() || "=+-*/(),.[]|:!".contains(ch) => Some(ch.to_string()),
        '{' => Some("\\{".into()),
        '}' => Some("\\}".into()),
        _ => None,
    }
}
fn parse(ts: &[Token], rs: &[Rule], depth: usize) -> Option<String> {
    if ts.is_empty() || depth > 12 {
        return None;
    }
    let size = ts.iter().map(|t| t.size).fold(0., f32::max);
    let mut used = HashSet::new();
    let mut atoms = vec![];
    let mut bars = rs.to_vec();
    bars.sort_by(|a, b| (b.0[2] - b.0[0]).total_cmp(&(a.0[2] - a.0[0])));
    for r in bars {
        let available: Vec<_> = ts
            .iter()
            .filter(|t| !used.contains(&t.index))
            .cloned()
            .collect();
        let Some((up, down)) = fraction_parts(&available, r) else {
            continue;
        };
        let numerator = parse(&up, rs, depth + 1)?;
        let denominator = parse(&down, rs, depth + 1)?;
        for t in up.iter().chain(&down) {
            used.insert(t.index);
        }
        atoms.push(Atom {
            x: r.0[0],
            right: r.0[2],
            y: r.y() - size * 0.25,
            size,
            tex: format!("\\frac{{{numerator}}}{{{denominator}}}"),
            word: false,
        });
    }
    for root in ts.iter().filter(|t| t.ch == '√') {
        if used.contains(&root.index) {
            continue;
        }
        let roof = rs
            .iter()
            .copied()
            .filter(|r| root_roof(root, *r))
            .min_by(|a, b| (a.y() - root.y).total_cmp(&(b.y() - root.y)))?;
        let inside: Vec<_> = ts
            .iter()
            .filter(|t| {
                !used.contains(&t.index)
                    && t.index != root.index
                    && t.x >= roof.0[0] - size * 0.25
                    && t.x < roof.0[2] + size * 0.5
                    && t.right <= roof.0[2] + size * 0.75
                    && !",.;".contains(t.ch)
                    && t.y < roof.y() - size * 0.35
                    && roof.y() - t.y <= size * 1.8
            })
            .cloned()
            .collect();
        let tex = parse(&inside, rs, depth + 1)?;
        let y = inside
            .iter()
            .filter(|t| t.size >= size * 0.9)
            .map(|t| t.y)
            .next()?;
        used.insert(root.index);
        for t in &inside {
            used.insert(t.index);
        }
        atoms.push(Atom {
            x: root.x,
            right: roof.0[2],
            y,
            size,
            tex: format!("\\sqrt{{{tex}}}"),
            word: false,
        });
    }
    for t in ts.iter().filter(|t| !used.contains(&t.index)) {
        atoms.push(Atom {
            x: t.x,
            right: t.right,
            y: t.y,
            size: t.size,
            tex: symbol(t.ch)?,
            word: !t.math && !t.italic && t.ch.is_ascii_alphabetic(),
        });
    }
    let mut out = render_atoms(atoms, depth)?;
    if out.contains("\\frac") || out.contains("\\sqrt") {
        let mut stack = vec![];
        let mut pairs = vec![];
        for (i, ch) in out.char_indices() {
            if out[..i].ends_with("\\left") || out[..i].ends_with("\\right") {
                continue;
            }
            if ch == '(' {
                stack.push(i);
            }
            if ch == ')'
                && let Some(left) = stack.pop()
            {
                pairs.extend([(left, "\\left("), (i, "\\right)")]);
            }
        }
        pairs.sort_by_key(|p| p.0);
        for (i, delimiter) in pairs.into_iter().rev() {
            out.replace_range(i..i + 1, delimiter);
        }
    }
    Some(out)
}
fn render_atoms(mut atoms: Vec<Atom>, depth: usize) -> Option<String> {
    if atoms.is_empty() || depth > 12 {
        return None;
    }
    let size = atoms.iter().map(|a| a.size).fold(0., f32::max);
    let baseline = dominant_y(&atoms, size);
    for a in &mut atoms {
        if ["\\sum ", "\\prod ", "\\int "].contains(&a.tex.as_str()) {
            a.y = baseline;
        }
    }
    atoms.sort_by(|a, b| a.x.total_cmp(&b.x));
    let scripted = |a: &Atom| a.size < size * 0.95 && (a.y - baseline).abs() > size * 0.10;
    let bases: Vec<_> = atoms.iter().filter(|a| !scripted(a)).cloned().collect();
    let mut scripts = vec![(Vec::<Atom>::new(), Vec::<Atom>::new()); bases.len()];
    for a in atoms.iter().filter(|a| scripted(a)) {
        let owner = bases
            .iter()
            .enumerate()
            .filter(|(_, b)| b.x <= a.x + size * 0.25)
            .max_by(|(_, a), (_, b)| a.x.total_cmp(&b.x))
            .map(|(i, _)| i)?;
        if a.y > baseline {
            scripts[owner].1.push(a.clone());
        } else {
            scripts[owner].0.push(a.clone());
        }
    }
    let mut out = String::new();
    let mut i = 0;
    while i < bases.len() {
        let a = &bases[i];
        let mut j = i + 1;
        if a.word && scripts[i].0.is_empty() && scripts[i].1.is_empty() {
            let mut word = a.tex.clone();
            while j < bases.len()
                && bases[j].word
                && scripts[j].0.is_empty()
                && scripts[j].1.is_empty()
                && (bases[j].y - a.y).abs() < size * 0.1
                && bases[j].x - bases[j - 1].right < size * 0.35
            {
                word.push_str(&bases[j].tex);
                j += 1;
            }
            if word.len() > 1 {
                out.push_str(&format!(
                    "\\{}{{{word}}}",
                    if depth > 0 { "mathrm" } else { "operatorname" }
                ));
            } else {
                out.push_str(&word);
            }
        } else {
            out.push_str(&a.tex);
        }
        if !scripts[i].0.is_empty() {
            out.push_str(&format!(
                "_{{{}}}",
                render_atoms(scripts[i].0.clone(), depth + 1)?
            ));
        }
        if !scripts[i].1.is_empty() {
            out.push_str(&format!(
                "^{{{}}}",
                render_atoms(scripts[i].1.clone(), depth + 1)?
            ));
        }
        i = j;
    }
    Some(out.trim().to_owned())
}
fn range(ts: &[Token]) -> Range<usize> {
    ts.iter().map(|t| t.index).min().unwrap()..ts.iter().map(|t| t.index + 1).max().unwrap()
}
fn numbered(ts: &[Token]) -> Option<(String, HashSet<usize>)> {
    let mut ordered = ts.to_vec();
    ordered.sort_by(|a, b| a.x.total_cmp(&b.x));
    if ordered.last()?.ch != ')' {
        return None;
    }
    let start = ordered.iter().rposition(|t| t.ch == '(')?;
    let digits = &ordered[start + 1..ordered.len() - 1];
    if digits.is_empty()
        || !digits
            .iter()
            .all(|t| t.ch.is_ascii_digit() || ".-".contains(t.ch))
    {
        return None;
    }
    Some((
        digits.iter().map(|t| t.ch).collect(),
        ordered[start..].iter().map(|t| t.index).collect(),
    ))
}
fn prose_words(ts: &[Token]) -> usize {
    // Three ordinary multi-letter words are prose, not a mathematical row.
    let mut ts = ts.to_vec();
    ts.sort_by(|a, b| a.x.total_cmp(&b.x));
    let mut count = 0;
    let mut len = 0;
    let mut right = -f32::INFINITY;
    for t in ts {
        if !t.math && !t.math_roman && !t.italic && t.ch.is_ascii_alphabetic() {
            if t.x - right > t.size * 0.3 {
                count += usize::from(len >= 2);
                len = 0;
            }
            len += 1;
            right = t.right;
        } else {
            count += usize::from(len >= 2);
            len = 0;
            right = -f32::INFINITY;
        }
    }
    count + usize::from(len >= 2)
}
fn prose(ts: &[Token]) -> bool {
    prose_words(ts) >= 3
}

pub(super) fn recover(
    page: &PdfPage<'_>,
    glyphs: &[Glyph],
    omitted: &HashSet<usize>,
    blocked: &HashSet<usize>,
    inline_rows: &HashSet<usize>,
) -> anyhow::Result<Vec<ReadingMath>> {
    let ts: Vec<_> = tokens(page, glyphs)?
        .into_iter()
        .filter(|t| !blocked.contains(&t.index))
        .collect();
    let rs = rules(page);
    let mut line_indices = vec![0; glyphs.len()];
    let mut line = 0;
    for (i, g) in glyphs.iter().enumerate() {
        line_indices[i] = line;
        if g.ch == '\n' {
            line += 1;
        }
    }
    let left = ts.iter().map(|t| t.x).fold(f32::INFINITY, f32::min);
    let mut used = HashSet::new();
    let mut found = vec![];
    // Display rows must be isolated and consist only of the formula/number.
    for equal in ts
        .iter()
        .filter(|t| t.ch == '=' && !t.mono && !inline_rows.contains(&line_indices[t.index]))
    {
        let base: Vec<_> = ts
            .iter()
            .filter(|t| (t.y - equal.y).abs() <= equal.size * 0.12)
            .cloned()
            .collect();
        let min_x = base.iter().map(|t| t.x).fold(f32::INFINITY, f32::min);
        let max_x = base.iter().map(|t| t.right).fold(0., f32::max);
        let row: Vec<_> = ts
            .iter()
            .filter(|t| {
                (t.y - equal.y).abs() <= equal.size * 1.8
                    && t.x >= min_x - equal.size * 0.3
                    && t.right <= max_x + equal.size * 0.3
                    && ((t.y - equal.y).abs() <= equal.size * 0.12
                        || t.math
                        || t.italic
                        || t.size < equal.size * 0.9
                        || t.ch == '√'
                        || t.ch.is_ascii_digit())
            })
            .cloned()
            .collect();
        if row.is_empty()
            || row.len() > 180
            || row
                .iter()
                .any(|t| t.mono || omitted.contains(&line_indices[t.index]))
            || prose(&row)
        {
            continue;
        }
        let margin = if row.len() == ts.len() {
            page.width().value * 0.1
        } else {
            left
        };
        if row.iter().map(|t| t.x).fold(f32::INFINITY, f32::min) < margin + equal.size * 1.5 {
            continue;
        }
        let r = range(&row);
        if used.contains(&equal.index) {
            continue;
        }
        let (number, labels) = numbered(&row).map_or_else(
            || (None, HashSet::new()),
            |(number, labels)| (Some(number), labels),
        );
        if number.is_none() && prose_words(&base) > 0 {
            continue;
        }
        let body: Vec<_> = row
            .iter()
            .filter(|t| !labels.contains(&t.index))
            .cloned()
            .collect();
        if !rs.iter().any(|r| fraction_parts(&body, *r).is_some())
            && body.iter().filter(|t| t.math).count() < 2
        {
            continue;
        }
        let Some(tex) = parse(&body, &rs, 0) else {
            continue;
        };
        let Ok(layout) = crate::formula::layout(&tex, true) else {
            continue;
        };
        let mut lines: Vec<_> = row.iter().map(|t| line_indices[t.index]).collect();
        lines.sort_unstable();
        lines.dedup();
        if glyphs.iter().enumerate().any(|(i, g)| {
            lines.contains(&line_indices[i])
                && !g.ch.is_whitespace()
                && !g.ch.is_control()
                && !row.iter().any(|t| t.index == i)
        }) {
            continue;
        }
        for t in &row {
            used.insert(t.index);
        }
        found.push(ReadingMath {
            range: r,
            lines,
            presentation: crate::pdf::MathPresentation {
                tex,
                display: true,
                number,
                layout: (*layout).clone(),
            },
        });
    }
    // Keep mixed-font glyphs in a predominantly monospace code row out of
    // recovery, including small symbols spilling onto adjacent extraction
    // lines. An ordinary prose row with an inline code word is NOT code.
    let code_rows: Vec<_> = glyphs
        .split(|g| g.ch == '\n')
        .filter_map(|row| {
            let visible: Vec<_> = row
                .iter()
                .filter(|g| !g.ch.is_whitespace() && !g.ch.is_control())
                .collect();
            let mono: Vec<_> = visible.iter().filter(|g| g.mono).collect();
            if mono.len() < 3 || mono.len() * 2 < visible.len() {
                return None;
            }
            Some((
                mono.iter().map(|g| g.x).fold(f32::INFINITY, f32::min),
                mono.iter().map(|g| g.right).fold(0., f32::max),
                mono[0].y,
                mono[0].size,
            ))
        })
        .collect();
    // Inline fractions and radicals can cross extraction newlines. Their
    // actual numerator/denominator coordinates determine the atomic span.
    let mut candidates: Vec<Vec<Token>> = vec![];
    for r in &rs {
        let available: Vec<_> = ts
            .iter()
            .filter(|t| {
                !used.contains(&t.index)
                    && !t.mono
                    && !omitted.contains(&line_indices[t.index])
                    && (t.math || t.italic || t.ch.is_ascii_digit() || "√=+-*/()[]|".contains(t.ch))
            })
            .cloned()
            .collect();
        if let Some((up, down)) = fraction_parts(&available, *r) {
            candidates.push(up.into_iter().chain(down).collect());
        }
    }
    for root in ts
        .iter()
        .filter(|t| t.ch == '√' && !used.contains(&t.index))
    {
        if let Some(roof) = rs.iter().copied().find(|r| root_roof(root, *r)) {
            let mut group = vec![root.clone()];
            group.extend(
                ts.iter()
                    .filter(|t| {
                        t.index != root.index
                            && t.x >= roof.0[0] - root.size * 0.25
                            && t.x < roof.0[2] + root.size * 0.5
                            && t.right <= roof.0[2] + root.size * 0.75
                            && !",.;".contains(t.ch)
                            && t.y < roof.y() - root.size * 0.35
                            && roof.y() - t.y <= root.size * 1.8
                    })
                    .cloned(),
            );
            candidates.push(group);
        }
    }
    // Mathematical-font runs and script-bearing italic variables in prose.
    for (i, base) in ts.iter().enumerate() {
        if used.contains(&base.index) || base.mono || omitted.contains(&line_indices[base.index]) {
            continue;
        }
        let word_tail = i > 0 && {
            let prev = &ts[i - 1];
            prev.italic
                && prev.ch.is_alphabetic()
                && base.ch.is_alphabetic()
                && !glyphs[prev.index + 1..base.index]
                    .iter()
                    .any(|g| g.ch.is_whitespace())
                && (prev.size - base.size).abs() < base.size * 0.1
                && (prev.y - base.y).abs() < base.size * 0.1
                && base.x - prev.right < base.size * 0.35
        };
        let scripted = !word_tail
            && ts.get(i + 1).is_some_and(|t| {
                base.italic
                    && t.size < base.size * 0.9
                    && (t.y - base.y).abs() > base.size * 0.1
                    && (t.y - base.y).abs() < base.size * 0.85
                    && t.x >= base.x
                    && t.x - base.right < base.size * 0.5
            });
        if !base.math && !scripted {
            continue;
        }
        // TeX uses a math-italic decimal point between math-roman digits.
        // The point (or division slash) alone is a seed, but the WHOLE numeric
        // term must become one object (0.1 / 28.4 / 1.0 * 10^20 / 1/4).
        let mut start = i;
        if matches!(base.ch, '.' | '/') && ts.get(i + 1).is_some_and(|t| t.ch.is_ascii_digit()) {
            while start > 0 {
                let previous = &ts[start - 1];
                let current = &ts[start];
                if !previous.ch.is_ascii_digit()
                    || previous.mono
                    || used.contains(&previous.index)
                    || omitted.contains(&line_indices[previous.index])
                    || (previous.size - base.size).abs() > base.size * 0.05
                    || (previous.y - base.y).abs() > base.size * 0.05
                    || current.x < previous.x
                    || current.x - previous.right > base.size * 0.35
                    || glyphs[previous.index + 1..current.index]
                        .iter()
                        .any(|g| g.ch.is_whitespace())
                {
                    break;
                }
                start -= 1;
            }
        }
        let mut group = ts[start..=i].to_vec();
        let mut prev = base;
        for next in &ts[i + 1..] {
            if next.mono
                || omitted.contains(&line_indices[next.index])
                || used.contains(&next.index)
                || (next.x < prev.x && next.size >= base.size * 0.9)
                || next.x - prev.right > base.size * 0.5
                || (next.y - base.y).abs() > base.size * 0.85
                || !(next.math
                    || next.size < base.size * 0.9
                    || "=+-*/(),|".contains(next.ch)
                    || next.ch.is_ascii_digit())
            {
                break;
            }
            group.push(next.clone());
            prev = next;
        }
        while group.last().is_some_and(|t| ",.-".contains(t.ch)) {
            group.pop();
        }
        if !group.is_empty() {
            candidates.push(group);
        }
    }
    candidates.sort_by_key(|ts| std::cmp::Reverse(ts.len()));
    for group in candidates {
        if group.iter().any(|t| {
            code_rows.iter().any(|&(left, right, y, size)| {
                t.x >= left - size * 0.3
                    && t.x <= right + size * 4.
                    && (t.y - y).abs() <= size * 1.8
            })
        }) {
            continue;
        }
        if group
            .iter()
            .any(|t| used.contains(&t.index) || omitted.contains(&line_indices[t.index]))
            || prose(&group)
        {
            continue;
        }
        let r = range(&group);
        // Never consume intervening prose when PDF text order is interleaved.
        if ts
            .iter()
            .any(|t| r.contains(&t.index) && !group.iter().any(|g| g.index == t.index))
        {
            continue;
        }
        let Some(tex) = parse(&group, &rs, 0) else {
            continue;
        };
        let Ok(layout) = crate::formula::layout(&tex, false) else {
            continue;
        };
        for t in &group {
            used.insert(t.index);
        }
        found.push(ReadingMath {
            range: r,
            lines: vec![],
            presentation: crate::pdf::MathPresentation {
                tex,
                display: false,
                number: None,
                layout: (*layout).clone(),
            },
        });
    }
    found.sort_by_key(|m| m.range.start);
    Ok(found)
}
