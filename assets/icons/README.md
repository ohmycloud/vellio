# Vellio application icon

Selected artwork: **A — orange-billed pelican on a mist-blue-grey rounded tile**. The cream bird, long orange bill and pouch replace the previous open-book icon.

- Background: `#516D86`; bird: `#FFF8E8`; bill: `#FF853D`.
- `Vellio.svg`: editable master, matching the approved A concept.
- `Vellio.png`: 1024 px transparent-canvas preview.
- `Vellio-small.svg`: same silhouette and palette, with a larger eye and mouth opening for small displays.
- `Vellio-small.png`: 64 px colour image embedded by the library header, displayed at 28 logical px without monochrome tinting.
- `Vellio.icns`: macOS standard and Retina sizes from 16 to 1024 px. Logical 16 px entries use the small-size cut.

The previous book artwork is archived in `assets/branding/vellio/archive/book-icon/`. Functional book icons (Reading / Unread / navigation) are not rebranded.

To regenerate on macOS with librsvg installed:

```sh
sh scripts/build_macos_icon.sh
```

The normal macOS packaging script copies the prebuilt ICNS and declares it through `CFBundleIconFile`; packaging does not require librsvg.

The desktop executable also embeds this ICNS and sets `NSApplication.applicationIconImage` at startup. This supplies the Dock icon when launching the executable directly or when LaunchServices still has a placeholder cached for the development bundle. The header caches its full-colour PNG once per reader view, preserving the logo palette in both light and dark themes.

Rebuild the development bundle after changing the artwork:

```sh
sh scripts/build_macos_icon.sh
sh scripts/package_macos.sh
```

Quit and restart the app to refresh both embedded images; packaging does not forcibly close an active reader. The development bundle is `target/Vellio.app`.
