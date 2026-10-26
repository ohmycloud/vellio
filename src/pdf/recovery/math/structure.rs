//! Geometry-backed annotations and upright operators inside PDF equations.
use super::{Atom, Rule, Token, parse};
use std::collections::HashSet;

pub(super) struct Underbrace {
    pub pieces: Vec<Token>,
    pub body: Vec<Token>,
    pub caption: Vec<Token>,
}

/// Legacy TeX extension fonts encode an extensible underbrace as | { z }.
/// Require that exact font-backed sequence, a formula above, and a small label
/// below. Ordinary literal braces or vertical bars can never trigger this path.
pub(super) fn underbraces(ts: &[Token]) -> Vec<Underbrace> {
    let mut pieces: Vec<_> = ts
        .iter()
        .filter(|t| t.extension && "|{z}".contains(t.ch))
        .cloned()
        .collect();
    pieces.sort_by(|a, b| b.y.total_cmp(&a.y).then(a.x.total_cmp(&b.x)));
    let mut found = vec![];
    let mut i = 0;
    while i + 3 < pieces.len() {
        let group = &pieces[i..i + 4];
        if group.iter().map(|t| t.ch).collect::<String>() != "|{z}"
            || group
                .iter()
                .any(|t| (t.y - group[0].y).abs() > group[0].size * 0.12)
        {
            i += 1;
            continue;
        }
        let left = group[0].x;
        let right = group[3].right;
        let y = group[0].y;
        let size = group[0].size;
        let inside = |t: &&Token| t.x >= left - size * 0.15 && t.right <= right + size * 0.2;
        let body: Vec<_> = ts
            .iter()
            .filter(inside)
            .filter(|t| t.y > y + size * 0.2 && t.y < y + size * 3.)
            .cloned()
            .collect();
        let caption: Vec<_> = ts
            .iter()
            .filter(inside)
            .filter(|t| t.y < y - size * 0.3 && t.y >= y - size * 1.6 && t.size < size * 0.9)
            .cloned()
            .collect();
        if !body.is_empty()
            && !caption.is_empty()
            && caption
                .iter()
                .all(|t| t.ch.is_ascii_alphanumeric() || "-".contains(t.ch))
            && caption
                .iter()
                .all(|t| (t.y - caption[0].y).abs() <= size * 0.12)
        {
            found.push(Underbrace {
                pieces: group.to_vec(),
                body,
                caption,
            });
        }
        i += 4;
    }
    found
}

pub(super) fn annotated_atoms(
    ts: &[Token],
    rules: &[Rule],
    depth: usize,
    used: &mut HashSet<usize>,
) -> Option<Vec<Atom>> {
    let mut atoms = vec![];
    for brace in underbraces(ts) {
        if brace
            .body
            .iter()
            .chain(&brace.pieces)
            .chain(&brace.caption)
            .any(|t| used.contains(&t.index))
        {
            continue;
        }
        let body = parse(&brace.body, rules, depth + 1)?;
        let mut caption = brace.caption.clone();
        caption.sort_by(|a, b| a.x.total_cmp(&b.x));
        let mut label = String::new();
        for (i, t) in caption.iter().enumerate() {
            if i > 0
                && (t.index > caption[i - 1].index + 1
                    || t.x - caption[i - 1].right > t.size * 0.25)
            {
                label.push(' ');
            }
            label.push(t.ch);
        }
        let size = brace.body.iter().map(|t| t.size).fold(0., f32::max);
        let y = brace
            .body
            .iter()
            .filter(|t| t.size >= size * 0.95)
            .map(|t| t.y)
            .max_by(f32::total_cmp)?;
        atoms.push(Atom {
            x: brace.pieces[0].x,
            right: brace.pieces[3].right,
            y,
            size,
            tex: format!("\\underbrace{{{body}}}_{{\\text{{{label}}}}}"),
            word: false,
        });
        used.extend(
            brace
                .body
                .iter()
                .chain(&brace.pieces)
                .chain(&brace.caption)
                .map(|t| t.index),
        );
    }
    Some(atoms)
}

/// Group upright function names before assigning centered subscripts. In
/// arg min_omega, the omega lies below the middle of the operator, not after n.
pub(super) fn operator_atoms(ts: &[Token], used: &mut HashSet<usize>) -> Vec<Atom> {
    let mut letters: Vec<_> = ts
        .iter()
        .filter(|t| {
            !used.contains(&t.index)
                && (!t.math || t.math_roman)
                && !t.italic
                && !t.bold
                && t.ch.is_ascii_alphabetic()
        })
        .collect();
    letters.sort_by(|a, b| b.y.total_cmp(&a.y).then(a.x.total_cmp(&b.x)));
    let mut atoms = vec![];
    let mut i = 0;
    while i < letters.len() {
        let first = letters[i];
        let mut end = i + 1;
        while end < letters.len()
            && (letters[end].y - first.y).abs() <= first.size * 0.1
            && (letters[end].size - first.size).abs() <= first.size * 0.05
            && letters[end].x - letters[end - 1].right >= -first.size * 0.1
            && letters[end].x - letters[end - 1].right <= first.size * 0.75
        {
            end += 1;
        }
        if end - i > 1 {
            let word: String = letters[i..end].iter().map(|t| t.ch).collect();
            let tex = match word.as_str() {
                "argmin" => "\\operatornamewithlimits{arg\\,min}".into(),
                "argmax" => "\\operatornamewithlimits{arg\\,max}".into(),
                _ => format!("\\operatorname{{{word}}}"),
            };
            atoms.push(Atom {
                x: first.x,
                right: letters[end - 1].right,
                y: first.y,
                size: first.size,
                tex,
                word: false,
            });
            used.extend(letters[i..end].iter().map(|t| t.index));
        }
        i = end;
    }
    atoms
}
