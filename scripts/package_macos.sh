#!/bin/sh
set -eu
# Run from project root. PDFium and all Rust/Typst resources travel with the app.
# Default: size-optimized release build. Use --dev only for a fast local build.
if [ "$#" -gt 1 ]; then
    printf 'Usage: sh scripts/package_macos.sh [--release|--dev]\n' >&2
    exit 2
fi
case "${1---release}" in
    --release) mode=release ;;
    --dev) mode=development ;;
    *)
        printf 'Usage: sh scripts/package_macos.sh [--release|--dev]\n' >&2
        exit 2
        ;;
esac
if [ "$mode" = release ]; then
    target=$(rustc -vV | awk '/^host:/ {print $2}')
    python3 scripts/build_release.py --target "$target"
    binary="target/$target/release/easyread-gpui"
else
    printf 'Development bundle: larger and unoptimized; omit --dev for a smaller release bundle.\n' >&2
    cargo build --locked --features desktop
    binary=target/debug/easyread-gpui
fi
app=target/Vellio.app
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources/licenses"
# Replace the inode so macOS cannot reuse cached code pages from an older build.
cp "$binary" "$app/Contents/MacOS/Vellio.next"
mv "$app/Contents/MacOS/Vellio.next" "$app/Contents/MacOS/Vellio"
cp assets/pdfium/libpdfium.dylib "$app/Contents/MacOS/libpdfium.dylib"
cp -R assets/pdfium/licenses "$app/Contents/Resources/licenses/pdfium"
cp assets/mitex/LICENSE "$app/Contents/Resources/licenses/MiTeX.txt"
cp assets/fonts/OFL.txt "$app/Contents/Resources/licenses/NotoSerifSC-OFL.txt"
cp assets/icons/Vellio.icns "$app/Contents/Resources/Vellio.icns"
cp vendor/gpui-base/LICENSE-APACHE "$app/Contents/Resources/licenses/GPUI-Base.txt"
cat > "$app/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleExecutable</key><string>Vellio</string>
<key>CFBundleIdentifier</key><string>org.easyread.gpui</string>
<key>CFBundleName</key><string>Vellio</string>
<key>CFBundleDisplayName</key><string>Vellio</string>
<key>CFBundleIconFile</key><string>Vellio.icns</string>
<key>CFBundleVersion</key><string>0.1.0</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>NSHighResolutionCapable</key><true/>
</dict></plist>
PLIST
# Keep development symbol names, but remove local symbols in release bundles.
# Never modify the executable after signing, and leave PDFium untouched.
if [ "$mode" = release ]; then
    xcrun strip -x "$app/Contents/MacOS/Vellio"
fi
# Both modes are locally signed, not Developer ID signed or notarized releases.
codesign --force --sign - "$app"
codesign --verify --deep --strict "$app"
printf 'Local macOS %s bundle (ad-hoc signed): %s\n' "$mode" "$app"
du -sh "$app"
