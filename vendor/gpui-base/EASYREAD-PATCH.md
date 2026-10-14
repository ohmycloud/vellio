# EasyRead local patch

Source: crates.io `gpui-base` 0.6.6, Apache-2.0 (see `LICENSE-APACHE`).
The application selects this copy through `[patch.crates-io]`.

The upstream HTML renderer recognizes `<u>` but discards its decoration color.
The local change carries `text-decoration-color` into `TextMark`, through nested
mark merging and selected-source serialization, and into GPUI's `UnderlineStyle`.
It leaves glyph foreground and highlight background unchanged. EasyRead underlines
use 2px thickness. A generated `span[data-easyread-note]` retains a separate
annotation id through nested marks; the existing native inline hit-testing and
drag guard route clicks as `easyread-annotation:<id>` to the application handler.
Document links retain their own marks and URLs.

The inline painter also records each internal annotation's first painted line
relative to its TextView. `TextViewState::annotation_offset(id)` exposes that
position to the application for margin-note placement. Each paint rebuilds the
map and notifies observers only when geometry changes, including width reflow.
This does not alter document text, native selection, or ordinary link handling.
An actual-window test verifies distinct quote positions and width reflow.

Selection geometry uses downstream affinity for characters and range starts at
soft-wrap boundaries. GPUI 0.3.6's `position_for_index` places the shared caret
on the previous row; using it for a glyph omitted the next row's first character
from drag selection. The shared correction covers TextView and plain text-run
projection, highlight starts, touch-handle starts and annotation offsets. Range
ends keep upstream affinity so a selection ending at a wrap stays on its row.
A window-event regression checks first-character and whole-row copying in both
drag directions, at two widths, with English and Unicode text.

Reading indicators can opt into paragraph geometry with
`TextViewState::set_track_paragraphs`. The renderer reports each paragraph's
window bounds, excluding its trailing gap; `paragraph_at` and `paragraph_bounds`
identify a clicked paragraph within a single PDF-page TextView. Geometry refreshes
on reflow and retains render-order identity when annotation spans are added.
Nested HTML roots correctly apply the final-paragraph gap rule to their final
child rather than every child. A window test checks three paragraphs in one view,
paragraph gaps, width reflow and annotation decoration; selection tests run with
paragraph tracking enabled as well.

Semantic mark/annotation fills are distinguished from inline-code badge colors.
In mixed inline flow, annotation rectangles use each fragment's shared line-cell
top and height, not its font's compact cap-height badge or shifted glyph bounds.
Backgrounds are painted in badge → annotation → glyph order; annotation fills
have square corners and preserve per-range colors, while unannotated code keeps
its compact rounded badge. Nested styles and stream fades retain the fill role.
Colored annotation underlines are painted at the shared row's body-font underline
position instead of each bold/code fragment's own font metrics. Adjacent
same-color spans are joined before GPUI's independent device-pixel rounding,
and fully underlined inline-code fragments cover their horizontal padding, so
no seam remains at font boundaries. Their 2px stroke, color, text shaping and
hit-testing remain unchanged; native uncolored and wavy underlines retain
GPUI's normal paint path.
This is display-only: text bytes, native selections, link handlers, note IDs and
saved annotations are unchanged. Actual painted-quad tests cover mixed bold,
italic, code, superscript, links, adjacent colors, two widths and code scales;
the application adds an actual-reader width-reflow regression.

Atomic inline objects (including native Typst formulas) paint drag/select-all
backgrounds using the shared row selection cell rather than their tight element
bounds. This fills the leading around subscripts and fractions without changing
formula size, baseline, layout, hit-testing or TeX copying. The application tests
actual painted quads and mouse-release toolbar capture at multiple widths/font
sizes, with forward and reverse drags through repeated subscripts and a fraction.

Persisted marks inside a native math HTML span also survive conversion to its
atomic object: highlight colors, colored underlines and annotation identity are
retained instead of dropping the decorated children. InlineFlow carries the
semantic fill separately from glyph styling, paints it on the shared row cell,
and includes objects in the common underline spans. Tall fractions lower that
row's annotation underline below the denominator rather than striking through it.
Annotated objects route clicks to the existing annotation handler without
recoloring the formula and report their first row for margin-note placement.
Tests cover paragraph-wide and formula-only saved annotations, reload-style
initialization, overlapping fill/underline colors, width/font reflow, clicks,
live recoloring/removal and unchanged source/TeX selection.

Typography alignment adds `TextViewStyle::with_heading` and `with_companion`.
Heading refinement is applied after the level-specific defaults; companion
refinement applies only to display-only HTML blockquotes marked
`data-easyread-companion="true"`. Ordinary HTML and Markdown blockquotes keep
their existing appearance. Both refinements participate in style equality so
selection/layout caches invalidate when typography changes. Parser and style
fingerprint tests cover the distinction. See `../../docs/typography.md`.

Modified source files: `src/text/style.rs`, `src/text/format/markdown.rs`,
`src/text/format/html.rs`, `src/text/node.rs`,
`src/text/state.rs`, `src/text/inline.rs`, `src/text/inline_flow.rs`, `src/text/text_view.rs`,
`src/text_selection.rs`, `src/text/inline_object.rs`.
Remove this patch when an upstream version supports colored underlines and has
passed EasyRead's annotation and native-window checks.
