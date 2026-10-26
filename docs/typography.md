# EasyRead typography alignment

Reference: sibling `easyread/easyread/web/css/base.css`, `library.css`,
`reader.css`, `reading-language.css`, and `settings.css`.

`src/desktop/typography.rs` resolves the reference font stacks against installed
font families. Bundled Noto Serif SC remains the default Chinese reading face.
English serif prose uses Source Serif 4 → Source Serif Pro → Charter → Iowan Old
Style → Georgia → Noto Serif SC. The system-serif and sans-serif user choices
remain available; existing saved font sizes, line widths and line spacing are
not reset.

| Role | Size | Line height |
| --- | --- | --- |
| Library/settings UI | 14px | 1.6 |
| Reader UI | 15px | 1.6 |
| Library sidebar | 13.5px; counts 12px | inherited |
| Library heading | 20px | 1.3 |
| Library paper title | 16px, semibold | 1.5 |
| Library metadata | 12.5px | inherited |
| Reader toolbar title | 14px, medium | 1.2 |
| Reading body | user size; default 21px | user spacing; default 1.9 |
| Secondary language | body × .84 | 1.66 |
| Figure caption | body × .86 | 1.75 |
| Table cell | body × .86 | 1.5 |
| Section heading level 1 / 2 / deeper | body × 1.28 / 1.08 / 1 | 1.5, semibold |
| Inline code / fenced code | body × .86 | inherited |

PDF font-run recovery keeps its deliberate inline-code-as-bold behavior (1.0
scale), so publisher emphasis does not turn into small monospace code.

Theme typography is reapplied after light/dark changes and library language
changes, including the Base theme projection used by rich-text code. Settings
use the English UI font stack, matching their current English labels.

Bilingual paragraph companions carry the display-only
`data-easyread-companion="true"` marker. The vendored rich-text renderer applies
`TextViewStyle::with_companion` only to these blocks, choosing the opposite
language's face and the secondary size/line height. Publisher blockquotes are
not restyled. The marker does not change persisted paper content or annotations.
The upstream level-aware `with_heading` callback supplies the reference heading
size, weight and line height. Theme edits use `Theme::update`, keeping Base's
projection in sync without manually mutating two theme representations.

## Ink and macOS smoothing

TextView's own foreground overrides the reader container's inherited color.
The reading style therefore explicitly sets body ink to `#23211d`, secondary ink
to `#5a554c`, and body font weight to normal (400). Theme changes cannot reset
these to GPUI's near-black foreground. Headings and source emphasis retain
their intentional stronger weights.

On macOS the pinned `gpui-pre-apple` 0.3.9 backend has an opt-in
`web-font-smoothing` feature, enabled only by this app's desktop build. GPUI Kit
0.8.0 moved the shared text rasterizer here from the older macOS backend. It keeps antialiasing enabled
while explicitly disabling CoreGraphics font-smoothing stroke thickening and
color-dependent glyph dilation, matching the intent of EasyRead's
`-webkit-font-smoothing: antialiased`. Emoji, font metrics and selection layout
are unaffected. No system or application preferences are changed. Details:
`vendor/gpui-pre-apple/EASYREAD-PATCH.md`.

Native GPUI and browser text rasterization/fallback can still differ; matching
families and logical pixel sizes does not imply pixel-identical screenshots.
CSS features such as reader letter spacing are not emulated by this change.

## Verification

```sh
cargo fmt --check
cargo check --locked --features desktop
cargo test --locked
RUST_MIN_STACK=16777216 cargo test --locked --features desktop-ui-tests --bin easyread-gpui
cargo test --manifest-path vendor/gpui-base/Cargo.toml --features test-support bilingual_companions_are_distinct_from_publisher_quotes
cargo test --manifest-path vendor/gpui-base/Cargo.toml --features test-support selection_layout_fingerprint_covers_callback_table_and_theme_fields
cargo test --manifest-path vendor/gpui-pre-apple/Cargo.toml --features web-font-smoothing,font-kit,test-support web_antialiasing
```
