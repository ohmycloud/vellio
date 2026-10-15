//! Inert Web SVGs use the same ink bounds and baseline as desktop math.
//! Exporting the Typst page directly adds page margins and can clip descenders
//! (notably radicals in inline fractions) outside the paragraph's frame.
use anyhow::Result;
use easyread_gpui::formula::{self, FormulaLayout, PathCommand};
use std::fmt::Write;

pub fn render(tex: &str, display: bool) -> Result<String> {
    let layout = formula::layout(tex, display)?;
    Ok(to_svg(&layout))
}

fn to_svg(layout: &FormulaLayout) -> String {
    let mut svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}" viewBox="0 0 {} {}" data-baseline="{}" data-font-size="16" fill="#23211d">"##,
        layout.width, layout.height, layout.width, layout.height, layout.baseline,
    );
    for path in &layout.paths {
        svg.push_str("<path d=\"");
        for command in &path.commands {
            match command {
                PathCommand::Move(p) => write!(svg, "M{} {}", p[0], p[1]),
                PathCommand::Line(p) => write!(svg, "L{} {}", p[0], p[1]),
                PathCommand::Quad(a, b) => write!(svg, "Q{} {} {} {}", a[0], a[1], b[0], b[1]),
                PathCommand::Cubic(a, b, c) => write!(
                    svg,
                    "C{} {} {} {} {} {}",
                    a[0], a[1], b[0], b[1], c[0], c[1]
                ),
                PathCommand::Close => {
                    svg.push('Z');
                    Ok(())
                }
            }
            .expect("writing to String");
        }
        if let Some(width) = path.stroke {
            write!(
                svg,
                "\" fill=\"none\" stroke=\"#23211d\" stroke-width=\"{width}\"/>"
            )
            .unwrap();
        } else {
            svg.push_str("\"/>");
        }
    }
    svg.push_str("</svg>");
    svg
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn svg_uses_complete_native_bounds_and_baseline_without_page_padding() {
        for tex in [
            r"\frac{1}{\sqrt{d_k}}",
            r"\sqrt{x_i}",
            r"x^2",
            r"\sum_{i=1}^{n} x_i",
        ] {
            for display in [false, true] {
                let layout = formula::layout(tex, display).unwrap();
                let svg = render(tex, display).unwrap();
                assert!(svg.contains(&format!(
                    r#"viewBox="0 0 {} {}""#,
                    layout.width, layout.height
                )));
                assert!(svg.contains(&format!(r#"data-baseline="{}""#, layout.baseline)));
                assert!(svg.contains("<path d="));
                // All outline/control points, including radical descenders and
                // half the stroke, must fit in the exported viewport.
                for path in &layout.paths {
                    let pad = path.stroke.unwrap_or(0.) / 2.;
                    let check = |p: &[f32; 2]| {
                        assert!(p[0] - pad >= -0.001 && p[0] + pad <= layout.width + 0.001);
                        assert!(p[1] - pad >= -0.001 && p[1] + pad <= layout.height + 0.001);
                    };
                    for command in &path.commands {
                        match command {
                            PathCommand::Move(a) | PathCommand::Line(a) => check(a),
                            PathCommand::Quad(a, b) => {
                                check(a);
                                check(b);
                            }
                            PathCommand::Cubic(a, b, c) => {
                                check(a);
                                check(b);
                                check(c);
                            }
                            PathCommand::Close => {}
                        }
                    }
                }
            }
        }
    }
}
