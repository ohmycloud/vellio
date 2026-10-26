//! The process-wide PDFium instance and its serialization lock.
use super::*;

pub(super) static PDF_LOCK: Mutex<()> = Mutex::new(());
static PDFIUM: OnceLock<Pdfium> = OnceLock::new();

pub(super) fn library(explicit: Option<&Path>) -> Result<&'static Pdfium> {
    if let Some(pdfium) = PDFIUM.get() {
        return Ok(pdfium);
    }
    let path = explicit
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("EASYREAD_PDFIUM_LIBRARY").map(PathBuf::from))
        .unwrap_or_else(|| {
            let adjacent = std::env::current_exe()
                .unwrap_or_default()
                .parent()
                .unwrap_or(Path::new("."))
                .join(Pdfium::pdfium_platform_library_name());
            if adjacent.exists() {
                adjacent
            } else {
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("assets/pdfium")
                    .join(Pdfium::pdfium_platform_library_name())
            }
        });
    let bindings = Pdfium::bind_to_library(&path).with_context(|| {
        format!(
            "PDFium unavailable at {}. Set EASYREAD_PDFIUM_LIBRARY to a compatible native library.",
            path.display()
        )
    })?;
    let _ = PDFIUM.set(Pdfium::new(bindings));
    Ok(PDFIUM.get().unwrap())
}
