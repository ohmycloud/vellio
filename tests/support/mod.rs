//! Shared synthetic PDF fixtures; all remain available through `support::*`.
use std::fs;

mod text;
#[allow(unused_imports)]
pub use text::*;
mod images;
#[allow(unused_imports)]
pub use images::*;
mod tables;
#[allow(unused_imports)]
pub use tables::*;
mod document;
#[allow(unused_imports)]
pub use document::*;
