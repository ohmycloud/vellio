use anyhow::{Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::{Arc, LazyLock, Mutex},
};
use typst::layout::{Abs, Frame, FrameItem, Point, Transform};
use typst::visualize::{CurveItem, Geometry};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FormulaLayout {
    pub width: f32,
    pub height: f32,
    pub baseline: f32,
    pub paths: Vec<FormulaPath>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FormulaPath {
    pub commands: Vec<PathCommand>,
    pub stroke: Option<f32>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum PathCommand {
    Move([f32; 2]),
    Line([f32; 2]),
    Quad([f32; 2], [f32; 2]),
    Cubic([f32; 2], [f32; 2], [f32; 2]),
    Close,
}
type Key = (String, bool);
static CACHE: LazyLock<Mutex<HashMap<Key, Arc<FormulaLayout>>>> = LazyLock::new(Mutex::default);
pub fn cached_layout(tex: &str, display: bool) -> Option<Arc<FormulaLayout>> {
    CACHE.lock().ok()?.get(&(tex.to_owned(), display)).cloned()
}
pub fn publish_layout(tex: &str, display: bool, layout: FormulaLayout) {
    if !layout.is_valid() {
        return;
    }
    if let Ok(mut cache) = CACHE.lock() {
        cache.insert((tex.to_owned(), display), Arc::new(layout));
    }
}
pub fn layout(tex: &str, display: bool) -> Result<Arc<FormulaLayout>> {
    if let Some(layout) = cached_layout(tex, display) {
        return Ok(layout);
    }
    let doc = super::compile(tex, display, 0)?;
    let frame = &doc.pages()[0].frame;
    let mut result = FormulaLayout {
        width: frame.width().to_pt() as f32,
        height: frame.height().to_pt() as f32,
        baseline: frame.height().to_pt() as f32,
        paths: vec![],
    };
    let mut baseline = None;
    walk(frame, Transform::identity(), &mut result, &mut baseline)?;
    if let Some(baseline) = baseline {
        result.baseline = baseline.clamp(0., result.height);
    }
    ensure!(
        result.width.is_finite()
            && result.height.is_finite()
            && result.width > 0.
            && result.height > 0.
            && !result.paths.is_empty(),
        "Invalid or empty Typst math frame"
    );
    // Include overhanging serifs and stretched delimiter outlines in the inline
    // object's bounds, without adding arbitrary image-like page margins.
    let (mut min, mut max) = ([0_f32, 0_f32], [result.width, result.height]);
    for path in &mut result.paths {
        for command in &mut path.commands {
            let pad = path.stroke.unwrap_or(0.) * 0.5;
            command.points_mut(&mut |p| {
                for i in 0..2 {
                    min[i] = min[i].min(p[i] - pad);
                    max[i] = max[i].max(p[i] + pad);
                }
            });
        }
    }
    ensure!(
        min.iter().chain(&max).all(|v| v.is_finite()),
        "Non-finite math outline"
    );
    for path in &mut result.paths {
        for command in &mut path.commands {
            command.points_mut(&mut |p| {
                p[0] -= min[0];
                p[1] -= min[1];
            });
        }
    }
    result.width = max[0] - min[0];
    result.height = max[1] - min[1];
    result.baseline -= min[1];
    ensure!(result.is_valid(), "Invalid native math geometry");
    let result = Arc::new(result);
    CACHE
        .lock()
        .map_err(|_| anyhow::anyhow!("Math layout cache poisoned"))?
        .insert((tex.to_owned(), display), result.clone());
    Ok(result)
}
pub(super) fn document_layout(doc: &typst_layout::PagedDocument) -> Result<FormulaLayout> {
    let frame = &doc.pages()[0].frame;
    let mut out = FormulaLayout {
        width: frame.width().to_pt() as f32,
        height: frame.height().to_pt() as f32,
        baseline: frame.height().to_pt() as f32,
        paths: vec![],
    };
    let mut baseline = None;
    walk(frame, Transform::identity(), &mut out, &mut baseline)?;
    ensure!(out.is_valid(), "Invalid native Typst table frame");
    Ok(out)
}

fn xy(point: Point, transform: Transform) -> [f32; 2] {
    let p = point.transform(transform);
    [p.x.to_pt() as f32, p.y.to_pt() as f32]
}
fn walk(
    frame: &Frame,
    transform: Transform,
    out: &mut FormulaLayout,
    baseline: &mut Option<f32>,
) -> Result<()> {
    if baseline.is_none() && frame.has_baseline() {
        *baseline = Some(xy(Point::new(Abs::zero(), frame.baseline()), transform)[1]);
    }
    for (position, item) in frame.items() {
        let transform = transform.pre_concat(Transform::translate(position.x, position.y));
        match item {
            FrameItem::Group(group) => {
                ensure!(group.clip.is_none(), "Unsupported clipped math frame");
                walk(
                    &group.frame,
                    transform.pre_concat(group.transform),
                    out,
                    baseline,
                )?;
            }
            FrameItem::Text(text) => {
                let mut cursor = Point::zero();
                for glyph in &text.glyphs {
                    let point = cursor
                        + Point::new(glyph.x_offset.at(text.size), -glyph.y_offset.at(text.size));
                    let mut pen = Outline {
                        commands: vec![],
                        transform: transform.pre_concat(Transform::translate(point.x, point.y)),
                        scale: text.size.to_pt() as f32 / text.font.units_per_em() as f32,
                    };
                    let outline = text
                        .font
                        .ttf()
                        .outline_glyph(ttf_parser::GlyphId(glyph.id), &mut pen);
                    ensure!(
                        outline.is_some()
                            || text.text.get(glyph.range()).is_some_and(|s| s
                                .chars()
                                .all(|c| c.is_whitespace() || c.is_control())),
                        "Math glyph has no native outline"
                    );
                    if !pen.commands.is_empty() {
                        out.paths.push(FormulaPath {
                            commands: pen.commands,
                            stroke: None,
                        });
                    }
                    cursor += Point::new(
                        glyph.x_advance.at(text.size),
                        -glyph.y_advance.at(text.size),
                    );
                }
            }
            FrameItem::Shape(shape, _) => {
                let mut commands = vec![];
                match &shape.geometry {
                    Geometry::Line(end) => {
                        commands.push(PathCommand::Move(xy(Point::zero(), transform)));
                        commands.push(PathCommand::Line(xy(*end, transform)));
                    }
                    Geometry::Rect(size) => {
                        for (i, p) in [
                            Point::zero(),
                            Point::new(size.x, Abs::zero()),
                            size.to_point(),
                            Point::new(Abs::zero(), size.y),
                        ]
                        .into_iter()
                        .enumerate()
                        {
                            commands.push(if i == 0 {
                                PathCommand::Move(xy(p, transform))
                            } else {
                                PathCommand::Line(xy(p, transform))
                            });
                        }
                        commands.push(PathCommand::Close);
                    }
                    Geometry::Curve(curve) => {
                        for item in &curve.0 {
                            commands.push(match item {
                                CurveItem::Move(p) => PathCommand::Move(xy(*p, transform)),
                                CurveItem::Line(p) => PathCommand::Line(xy(*p, transform)),
                                CurveItem::Cubic(a, b, c) => PathCommand::Cubic(
                                    xy(*a, transform),
                                    xy(*b, transform),
                                    xy(*c, transform),
                                ),
                                CurveItem::Close => PathCommand::Close,
                            });
                        }
                    }
                }
                if shape.fill.is_some() {
                    out.paths.push(FormulaPath {
                        commands: commands.clone(),
                        stroke: None,
                    });
                }
                if let Some(stroke) = &shape.stroke {
                    out.paths.push(FormulaPath {
                        commands,
                        stroke: Some(stroke.thickness.to_pt() as f32),
                    });
                }
            }
            FrameItem::Image(..) => bail!("Images are not supported inside native math"),
            FrameItem::Tag(..) | FrameItem::Link(..) => {}
        }
    }
    Ok(())
}
impl FormulaLayout {
    pub fn is_valid(&self) -> bool {
        self.width.is_finite()
            && self.height.is_finite()
            && self.width > 0.
            && self.height > 0.
            && self.baseline.is_finite()
            && (0. ..=self.height).contains(&self.baseline)
            && !self.paths.is_empty()
            && self.paths.iter().all(|p| {
                p.stroke.is_none_or(|s| s.is_finite() && s > 0.)
                    && !p.commands.is_empty()
                    && p.commands.iter().all(PathCommand::is_finite)
            })
    }
}
impl PathCommand {
    fn is_finite(&self) -> bool {
        match self {
            Self::Move(a) | Self::Line(a) => a.iter().all(|v| v.is_finite()),
            Self::Quad(a, b) => a.iter().chain(b).all(|v| v.is_finite()),
            Self::Cubic(a, b, c) => a.iter().chain(b).chain(c).all(|v| v.is_finite()),
            Self::Close => true,
        }
    }
    fn points_mut(&mut self, visit: &mut impl FnMut(&mut [f32; 2])) {
        match self {
            Self::Move(a) | Self::Line(a) => visit(a),
            Self::Quad(a, b) => {
                visit(a);
                visit(b);
            }
            Self::Cubic(a, b, c) => {
                visit(a);
                visit(b);
                visit(c);
            }
            Self::Close => {}
        }
    }
}
struct Outline {
    commands: Vec<PathCommand>,
    transform: Transform,
    scale: f32,
}
impl Outline {
    fn p(&self, x: f32, y: f32) -> [f32; 2] {
        xy(
            Point::new(
                Abs::pt((x * self.scale) as f64),
                Abs::pt((-y * self.scale) as f64),
            ),
            self.transform,
        )
    }
}
impl ttf_parser::OutlineBuilder for Outline {
    fn move_to(&mut self, x: f32, y: f32) {
        self.commands.push(PathCommand::Move(self.p(x, y)));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.commands.push(PathCommand::Line(self.p(x, y)));
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.commands
            .push(PathCommand::Quad(self.p(x1, y1), self.p(x, y)));
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.commands.push(PathCommand::Cubic(
            self.p(x1, y1),
            self.p(x2, y2),
            self.p(x, y),
        ));
    }
    fn close(&mut self) {
        self.commands.push(PathCommand::Close);
    }
}
