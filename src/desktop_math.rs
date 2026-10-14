//! Paint Typst's positioned font outlines and rules as native GPUI paths.
//! No image source, SVG decoder, PDF crop or raster texture is involved.
use easyread_gpui::formula::{self, FormulaLayout, PathCommand};
use gpui_kit::base::text::{
    InlineElement, InlineRenderContext, MarkdownNode, MarkdownParseContext, MarkdownPlugin,
    markdown_ast,
};
use gpui_kit::{
    App, InteractiveElement, IntoElement, ParentElement, PathBuilder, Styled, Window, canvas, div,
    point, px,
};
use std::sync::Arc;

pub(super) struct MathPlugin {
    pub max_width: f32,
}
impl MarkdownPlugin for MathPlugin {
    fn name(&self) -> &str {
        "vellio-math"
    }
    fn parse(&self, _: &markdown_ast::Node, _: &MarkdownParseContext<'_>) -> Option<MarkdownNode> {
        None
    }
    fn render_inline(
        &self,
        node: &MarkdownNode,
        context: &InlineRenderContext,
        _: &mut Window,
        _: &mut App,
    ) -> Option<InlineElement> {
        let (tex, display) = node.data::<(String, bool)>()?;
        let layout = formula::cached_layout(tex, *display)?;
        let mut scale = f32::from(context.font_size()) / 16.;
        if *display {
            scale = scale.min(self.max_width / layout.width);
        }
        let baseline = px(layout.baseline * scale);
        Some(
            InlineElement::new(native(
                layout,
                scale,
                context.text_style().color,
                "reader-native-typst-math",
            ))
            .with_baseline(baseline),
        )
    }
}
pub(super) struct TablePlugin {
    pub max_width: f32,
}
impl MarkdownPlugin for TablePlugin {
    fn name(&self) -> &str {
        "vellio-table"
    }
    fn parse(&self, _: &markdown_ast::Node, _: &MarkdownParseContext<'_>) -> Option<MarkdownNode> {
        None
    }
    fn render_inline(
        &self,
        node: &MarkdownNode,
        context: &InlineRenderContext,
        _: &mut Window,
        _: &mut App,
    ) -> Option<InlineElement> {
        let key = node.data::<String>()?;
        let layout = easyread_gpui::typst_table::cached_layout(key)?;
        let scale = (f32::from(context.font_size()) / 16.).min(self.max_width / layout.width);
        let baseline = px(layout.height * scale);
        Some(
            InlineElement::new(
                div()
                    .debug_selector(|| "reader-native-typst-table".to_owned())
                    .child(native(
                        layout,
                        scale,
                        context.text_style().color,
                        "reader-native-typst-table-paths",
                    )),
            )
            .with_baseline(baseline),
        )
    }
}

fn native(
    layout: Arc<FormulaLayout>,
    scale: f32,
    color: gpui_kit::Hsla,
    selector: &'static str,
) -> impl IntoElement {
    div()
        .w(px(layout.width * scale))
        .h(px(layout.height * scale))
        .debug_selector(move || selector.to_owned())
        .child(
            canvas(
                |bounds, _, _| bounds,
                move |bounds, _, window, _| {
                    let p = |v: [f32; 2]| {
                        point(
                            bounds.left() + px(v[0] * scale),
                            bounds.top() + px(v[1] * scale),
                        )
                    };
                    for layer in &layout.paths {
                        let mut builder = layer.stroke.map_or_else(PathBuilder::fill, |width| {
                            PathBuilder::stroke(px(width * scale))
                        });
                        for command in &layer.commands {
                            match command {
                                PathCommand::Move(a) => builder.move_to(p(*a)),
                                PathCommand::Line(a) => builder.line_to(p(*a)),
                                PathCommand::Quad(a, b) => builder.curve_to(p(*b), p(*a)),
                                PathCommand::Cubic(a, b, c) => {
                                    builder.cubic_bezier_to(p(*c), p(*a), p(*b))
                                }
                                PathCommand::Close => builder.close(),
                            }
                        }
                        if let Ok(path) = builder.build() {
                            window.paint_path(path, color);
                        }
                    }
                },
            )
            .size_full(),
        )
}
