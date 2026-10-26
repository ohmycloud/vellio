//! Render persisted quote annotations without changing the paper's Markdown.
use serde_json::Value;
use std::ops::Range;

/// Shared ink for PDF, Markdown and other document links in the reader.
pub const LINK_COLOR: u32 = 0x8e0012;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum HighlightColor {
    #[default]
    Yellow,
    Green,
    Blue,
    Red,
}

impl HighlightColor {
    pub const ALL: [Self; 4] = [Self::Yellow, Self::Green, Self::Blue, Self::Red];
    pub fn from_name(name: &str) -> Self {
        match name {
            "green" => Self::Green,
            "blue" => Self::Blue,
            "pink" => Self::Red,
            _ => Self::Yellow,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Yellow => "yellow",
            Self::Green => "green",
            Self::Blue => "blue",
            Self::Red => "pink",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Yellow => "黄色",
            Self::Green => "绿色",
            Self::Blue => "蓝色",
            Self::Red => "红色",
        }
    }
    pub fn rgb(self) -> u32 {
        match self {
            Self::Yellow => 0xf2df8a,
            Self::Green => 0xd4e3c9,
            Self::Blue => 0xd2e0ed,
            Self::Red => 0xf0cec7,
        }
    }
    pub fn line_rgb(self) -> u32 {
        match self {
            Self::Yellow => 0xdca722,
            Self::Green => 0x58994c,
            Self::Blue => 0x477fc1,
            Self::Red => 0xcc413a,
        }
    }
}

mod anchors;
mod decoration;
mod markup;
mod source;
use anchors::collapsed_whitespace;
pub use anchors::{
    MARGIN_EDITOR_ANCHOR, context, margin_editor_anchor, navigation_anchor, quote_range,
    quote_range_in_context, rendered_paragraph_quote,
};
pub use decoration::{decorate, decorate_interactive};
use markup::{characters, escape};
pub use markup::{html, plain_text};
pub(crate) use source::{
    source_highlight, source_image, source_link, source_math_edits, source_superscript,
    source_typography,
};
#[cfg(test)]
mod tests;
