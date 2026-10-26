# Vellio / EasyRead Apple text rasterization patch

Source: crates.io `gpui-pre-apple` 0.3.9, Apache-2.0 (see `LICENSE-APACHE`).
Selected through the root `[patch.crates-io]`; the macOS-only optional desktop
dependency enables `font-kit` and `web-font-smoothing`.

GPUI 0.3.9 moved the CoreText rasterizer from `gpui-pre-macos` into this shared
Apple backend. The old 0.3.6 macOS vendor is no longer patched or needed. The
opt-in patch has been ported here; patching the old macOS crate would no longer
control the text rasterizer used by the application.

EasyRead uses `-webkit-font-smoothing: antialiased`. With `web-font-smoothing`:

- `glyph_dilation_for_color` returns zero for dark and light foreground colors.
- Non-emoji glyph bitmaps explicitly disable CoreGraphics font smoothing
  (stroke thickening), while retaining grayscale antialiasing and subpixel
  positioning.
- Font selection, weight, metrics, layout, advances, bitmap bounds and emoji
  rasterization are unchanged. No system/application preferences are written.

Without the feature, upstream rendering behavior is retained. The application
continues to set normal reader weight and its own foreground/secondary colors.

The standalone manifest also explicitly enables CoreVideo `CVBase`, which the
upstream Metal renderer's pixel-buffer locking functions require. The normal
macOS application already receives this through dependency feature unification;
listing it here makes isolated backend tests build as well.

Modified upstream files: `Cargo.toml`, `src/text_system.rs`.

Native tests verify zero dilation and actual Charter glyph masks at 1×/2×:
solid interiors and antialiased edges are both required.

```sh
CARGO_TARGET_DIR="$PWD/target" CARGO_PROFILE_DEV_DEBUG=0 \
  cargo test --manifest-path vendor/gpui-pre-apple/Cargo.toml \
  --features web-font-smoothing,font-kit,test-support web_antialiasing
```

Validated on macOS ARM64. This is not an iOS rendering or other-platform acceptance.
