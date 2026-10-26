//! Native assets, platform integration and window startup.
use super::*;

gpui_kit::assets::icon_assets!(
    SelectionIcons,
    [
        Highlighter,
        Underline,
        MessageSquare,
        CircleQuestionMark,
        BookOpen,
        List,
        File,
        Asterisk,
        Search,
        Plus,
        Moon,
        Sun,
        Settings,
        Star,
        Check,
        ChevronDown,
        Folder,
        Pin,
        Copy,
        Pencil,
        Trash
    ]
);

struct ReaderAssets;
impl AssetSource for ReaderAssets {
    fn load(&self, path: &str) -> anyhow::Result<Option<std::borrow::Cow<'static, [u8]>>> {
        if let Some(bytes) = SelectionIcons.load(path)? {
            return Ok(Some(bytes));
        }
        gpui_kit::assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> anyhow::Result<Vec<SharedString>> {
        let mut paths = gpui_kit::assets::Assets.list(path)?;
        paths.extend(SelectionIcons.list(path)?);
        paths.sort();
        paths.dedup();
        Ok(paths)
    }
}

pub fn run(raw: Value, library: Option<Library>, workspace: Option<Workspace>) {
    gpui_kit::application()
        .with_assets(ReaderAssets)
        .run(move |cx| {
            #[cfg(target_os = "macos")]
            set_dock_icon();
            cx.text_system()
                .add_fonts(vec![
                    std::borrow::Cow::Borrowed(easyread_gpui::fonts::NOTO_SERIF_SC_REGULAR),
                    std::borrow::Cow::Borrowed(easyread_gpui::fonts::NOTO_SERIF_SC_SEMIBOLD),
                ])
                .expect("Cannot load bundled Noto Serif SC fonts");
            gpui_kit::init(cx);
            typography::apply_theme(false, cx);
            let text_style =
                gpui_kit::base::TextViewStyle::from_theme(&gpui_kit::base::Theme::global(cx))
                    .with_link(rgb(annotations::LINK_COLOR).into())
                    .with_link_underline(false);
            cx.set_global(
                gpui_kit::base::TextViewDefaults::new()
                    .with_style(text_style)
                    .with_code_block_highlighter(code::highlight),
            );
            cx.on_window_closed(|cx, _| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                        None,
                        size(px(1400.), px(900.)),
                        cx,
                    ))),
                    ..Default::default()
                },
                cx,
                |window, cx| cx.new(|cx| ReaderView::new(raw, library, workspace, window, cx)),
            )
            .expect("Cannot open Vellio window");
            cx.activate(true);
        });
}

#[cfg(target_os = "macos")]
fn set_dock_icon() {
    use objc2::{AnyThread, MainThreadMarker};
    use objc2_app_kit::{NSApplication, NSImage};
    use objc2_foundation::NSData;

    // Direct executable launches and stale LaunchServices entries can otherwise
    // leave the running app with macOS's generic placeholder icon.
    let main_thread = MainThreadMarker::new().expect("App startup must run on the main thread");
    let data = NSData::with_bytes(include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/assets/icons/Vellio.icns"
    )));
    let Some(icon) = NSImage::initWithData(NSImage::alloc(), &data).filter(|icon| icon.isValid())
    else {
        eprintln!("Cannot decode the embedded Vellio Dock icon");
        return;
    };
    // SAFETY: A valid, non-null image is supplied on the application main thread.
    unsafe {
        NSApplication::sharedApplication(main_thread).setApplicationIconImage(Some(&icon));
    }
}
