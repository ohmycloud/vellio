# Vellio / EasyRead macOS text rasterization patch

Source: crates.io `gpui-pre-macos` 0.3.6, Apache-2.0 (see `LICENSE-APACHE`).
The application selects this copy through `[patch.crates-io]` and enables the
opt-in `web-font-smoothing` feature in its macOS desktop dependency.

EasyRead uses `-webkit-font-smoothing: antialiased`. The corresponding native
policy keeps grayscale antialiasing enabled but explicitly disables CoreGraphics
font smoothing (stroke thickening). Merely selecting GPUI's `Grayscale` rendering
mode, or requesting dilation 0 for dark text, does not explicitly disable the
fresh CGContext's font-smoothing policy.

With `web-font-smoothing` enabled:

- `glyph_dilation_for_color` returns 0 for all foreground colors.
- Non-emoji glyph bitmaps set both `allowsFontSmoothing` and `shouldSmoothFonts`
  to false, retaining antialiasing and subpixel positioning.
- Font selection, weight, metrics, layout, advances, bitmap bounds and emoji
  rasterization are unchanged.
- No `AppleFontSmoothing` system/application preferences are written.

Without the feature the original rendering policy is retained. The application
also explicitly sets EasyRead's reader foreground `#23211d`, secondary ink
`#5a554c`, and normal body weight rather than inheriting GPUI theme foreground.

The vendored manifest allows deprecation warnings from the pinned upstream
cocoa backend, analogous to the warning cap for registry dependencies. Other
warnings are not suppressed.

Modified upstream files: `Cargo.toml`, `src/text_system.rs`.

Native raster tests check zero dilation for dark/light ink and real Charter
glyph bitmaps at 1×/2× scale: interiors remain solid and edges remain antialiased.

```sh
cargo test --manifest-path vendor/gpui-pre-macos/Cargo.toml \
  --features web-font-smoothing,font-kit,test-support web_antialiasing
```
