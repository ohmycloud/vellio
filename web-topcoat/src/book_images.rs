//! The same image allowlist is used by rendering and the asset route.
//! Paths on disk stay relative; browser URLs must include the owning book.
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, percent_decode_str, utf8_percent_encode};

const SEGMENT: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'~');

pub fn content_type(path: &str) -> Option<&'static str> {
    let (directory, _) = path.split_once('/')?;
    if !matches!(directory, "images" | "figures" | "pages")
        || path
            .split('/')
            .any(|part| part.is_empty() || matches!(part, "." | ".."))
        || path.chars().any(|c| c == '\\' || c.is_control())
    {
        return None;
    }
    match path.rsplit_once('.')?.1.to_ascii_lowercase().as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "webp" => Some("image/webp"),
        "gif" => Some("image/gif"),
        _ => None,
    }
}

pub fn url(book: &str, path: &str) -> Option<String> {
    content_type(path)?;
    let path = path
        .split('/')
        .map(|part| utf8_percent_encode(part, SEGMENT).to_string())
        .collect::<Vec<_>>()
        .join("/");
    Some(format!(
        "/read/{}/{path}",
        utf8_percent_encode(book, SEGMENT)
    ))
}

/// Rebase relative images in legacy HTML too, but do not rewrite links/anchors
/// or permit an HTML fragment to address another book or the server filesystem.
pub fn html_url(book: &str, source: &str) -> Option<String> {
    let prefix = format!("/read/{}/", utf8_percent_encode(book, SEGMENT));
    let path = source.strip_prefix(&prefix).unwrap_or(source);
    let path = percent_decode_str(path).decode_utf8().ok()?;
    url(book, &path)
}

/// Native PDF layout includes PNG crops. Permit only this raster data URL form,
/// not arbitrary data: URLs (especially SVG/HTML or links). Keep a size bound.
pub fn inline_png(source: &str) -> bool {
    source
        .strip_prefix("data:image/png;base64,")
        .is_some_and(|data| {
            data.starts_with("iVBORw0KGgo")
                && data.len() <= 32 * 1024 * 1024
                && data
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/' | b'='))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_urls_include_book_and_encode_each_path_segment() {
        assert_eq!(
            url("book-one", "figures/native-1.webp").as_deref(),
            Some("/read/book-one/figures/native-1.webp")
        );
        assert_eq!(
            url("book-one", "images/图 1#?.PNG").as_deref(),
            Some("/read/book-one/images/%E5%9B%BE%201%23%3F.PNG")
        );
        assert_eq!(
            html_url("book-one", "/read/book-one/images/a%20b.png"),
            url("book-one", "images/a b.png")
        );
        assert_eq!(
            html_url("book-one", "images/a%20b.png"),
            url("book-one", "images/a b.png")
        );
        for path in [
            "../images/x.png",
            "images/../paper.json",
            "images//x.png",
            "images/./x.png",
            "images/evil.svg",
            "source.pdf",
            "/etc/passwd",
            "images\\x.png",
            "images/\nx.png",
        ] {
            assert_eq!(url("book-one", path), None, "{path}");
        }
        for source in [
            "/read/other/images/x.png",
            "images/%2e%2e/paper.json",
            "https://example.com/x.png",
            "//host/x.png",
        ] {
            assert_eq!(html_url("book-one", source), None, "{source}");
        }
    }

    #[test]
    fn only_bounded_inline_pngs_are_allowed() {
        assert!(inline_png("data:image/png;base64,iVBORw0KGgoAAA=="));
        for source in [
            "data:image/svg+xml,<svg/>",
            "data:text/html,<script/>",
            "data:image/png;base64,bad!",
            "https://host/x.png",
        ] {
            assert!(!inline_png(source));
        }
    }
}
