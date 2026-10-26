//! Presentation-only heading hierarchy; original titles and reader data stay intact.

pub fn split_heading_number(title: &str) -> (Option<&str>, &str) {
    let title = title.trim();
    let end = title
        .find(|c: char| !c.is_ascii_digit() && c != '.')
        .unwrap_or(title.len());
    let number = title[..end].trim_end_matches('.');
    if number.is_empty() || number.split('.').any(|part| part.is_empty()) {
        return (None, title);
    }
    let body = title[end..].trim_start();
    if body.is_empty() {
        return (None, title);
    }
    (Some(number), body)
}

pub fn heading_depth(title: &str, level: Option<u64>) -> usize {
    if let Some(level) = level.filter(|level| *level > 0) {
        return level.saturating_sub(1).min(6) as usize;
    }
    split_heading_number(title)
        .0
        .map(|number| number.split('.').count().saturating_sub(1).min(6))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn heading_hierarchy_handles_explicit_levels_numbered_and_unnumbered_titles() {
        assert_eq!(heading_depth("1 Chapter", None), 0);
        assert_eq!(heading_depth("1.3 Authorization matters", None), 1);
        assert_eq!(heading_depth("1.3.2Zero-trust security models", None), 2);
        assert_eq!(
            split_heading_number("1.3.2Zero-trust security models"),
            (Some("1.3.2"), "Zero-trust security models")
        );
        assert_eq!(heading_depth("Unnumbered subsection", Some(3)), 2);
        assert_eq!(heading_depth("1.3.2 Title", Some(1)), 0);
        assert_eq!(heading_depth("Unnumbered", None), 0);
        assert_eq!(heading_depth("1..3 Broken", None), 0);
        assert_eq!(heading_depth("Title", Some(u64::MAX)), 6);
        assert_eq!(split_heading_number("1984"), (None, "1984"));
    }
}
