//! Read-only catalog presentation, including native reading-position recovery.
use serde_json::{Value, json};
use std::collections::BTreeSet;

pub fn title(item: &Value) -> &str {
    item["meta"]["display_name"]
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| item["meta"]["title_zh"].as_str().filter(|s| !s.is_empty()))
        .or_else(|| item["meta"]["title_en"].as_str().filter(|s| !s.is_empty()))
        .unwrap_or("Untitled")
}

pub fn authors(meta: &Value) -> String {
    match &meta["authors"] {
        Value::String(s) => s.clone(),
        Value::Array(authors) => authors
            .iter()
            .filter_map(|a| a.as_str().or_else(|| a["name"].as_str()))
            .collect::<Vec<_>>()
            .join(", "),
        _ => String::new(),
    }
}

/// Plain-text citations use only metadata present in the catalog.
pub fn citation(item: &Value) -> String {
    let mut parts = Vec::new();
    if let Some(authors) = item["authors"].as_str().filter(|s| !s.trim().is_empty()) {
        parts.push(authors.to_owned());
    }
    if let Some(year) = published_year(item) {
        parts.push(format!("({year})"));
    }
    parts.push(title(item).to_owned());
    if let Some(venue) = item["meta"]["venue"]
        .as_str()
        .or_else(|| item["meta"]["publication"].as_str())
        .filter(|s| !s.trim().is_empty())
    {
        parts.push(venue.to_owned());
    }
    parts
        .iter()
        .map(|part| part.trim_end_matches('.'))
        .collect::<Vec<_>>()
        .join(". ")
        + "."
}

pub fn published_year(item: &Value) -> Option<u64> {
    let year = &item["meta"]["year"];
    year.as_u64()
        .or_else(|| year.as_str()?.trim().parse().ok())
        .filter(|year| (1..=9999).contains(year))
}

/// Menu order: recently opened, recently added, year published, title.
pub fn compare_for_sort(a: &Value, b: &Value, sort: usize) -> std::cmp::Ordering {
    let by_title = || title(a).to_lowercase().cmp(&title(b).to_lowercase());
    match sort {
        1 => compare_time(
            b["item"]["added"].as_str().unwrap_or(""),
            a["item"]["added"].as_str().unwrap_or(""),
        )
        .then_with(by_title),
        2 => published_year(b)
            .cmp(&published_year(a))
            .then_with(by_title),
        3 => by_title(),
        _ => compare_time(
            b["opened"].as_str().unwrap_or(""),
            a["opened"].as_str().unwrap_or(""),
        )
        .then_with(by_title),
    }
}

pub fn summary(id: &str, state: &Value, native: &Value) -> Value {
    let blocks = state["paper"]["blocks"].as_array();
    let native_progress = &native["progress"];
    let committed = &state["reader"]["progress"];
    let native_known = blocks.is_some_and(|bs| {
        bs.iter()
            .any(|b| native_progress["block"].is_string() && b["id"] == native_progress["block"])
    });
    let progress = if compare_time(
        native_progress["at"].as_str().unwrap_or(""),
        committed["at"].as_str().unwrap_or(""),
    ) != std::cmp::Ordering::Less
        && native_known
    {
        native_progress
    } else {
        committed
    };
    let ratio = blocks
        .and_then(|bs| {
            bs.iter()
                .position(|b| b["id"] == progress["block"] && progress["block"].is_string())
                .map(|index| (index + 1) as f64 / bs.len().max(1) as f64)
        })
        .or_else(|| progress["ratio"].as_f64())
        .unwrap_or(0.0)
        .clamp(0.0, 1.0);
    let meta = &state["paper"]["meta"];
    let status = match state["item"]["status"].as_str() {
        Some("done") => "done",
        Some("reading") => "reading",
        Some("unread") => "unread",
        _ if progress["block"].is_string() => "reading",
        _ => "unread",
    };
    let notes = state["reader"]["notes"]
        .as_object()
        .map(|notes| {
            notes
                .values()
                .filter(|n| !n["deleted"].as_bool().unwrap_or(false))
                .count()
        })
        .unwrap_or(0);
    let opened = [
        state["item"]["opened"].as_str().unwrap_or(""),
        progress["at"].as_str().unwrap_or(""),
    ]
    .into_iter()
    .max_by(|a, b| compare_time(a, b))
    .unwrap_or("");
    let opened = chrono::DateTime::parse_from_rfc3339(opened)
        .map(|at| {
            at.with_timezone(&chrono::Utc)
                .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
        })
        .unwrap_or_else(|_| opened.to_owned());
    // Derive text-translation coverage from source blocks, not a provider's
    // claim that an entire batch/page was completed. This is read-only: never
    // repair an old paper by rewriting it while listing the library.
    let mut text_pages = BTreeSet::new();
    let mut untranslated_pages = BTreeSet::new();
    for block in blocks.into_iter().flatten() {
        if matches!(block["type"].as_str(), Some("para" | "heading"))
            && !block["en"].as_str().unwrap_or("").trim().is_empty()
            && let Some(page) = block["page"].as_u64().filter(|p| *p > 0)
        {
            text_pages.insert(page);
            if block["zh"].as_str().unwrap_or("").trim().is_empty() {
                untranslated_pages.insert(page);
            }
        }
    }
    let mut translation = state["paper"]["translation"]
        .as_object()
        .cloned()
        .unwrap_or_default();
    let mut done: BTreeSet<_> = translation
        .get("done_pages")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_u64)
        .collect();
    // Retain coverage for kinds not yet handled by the native text translator.
    done.retain(|page| !untranslated_pages.contains(page));
    done.extend(text_pages.difference(&untranslated_pages));
    translation.insert("done_pages".into(), json!(done));
    if state["job"]["type"] == "translate" && state["job"]["failed_pages"].is_array() {
        let mut failed: BTreeSet<_> = translation
            .get("failed_pages")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_u64)
            .collect();
        failed.extend(
            state["job"]["failed_pages"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(Value::as_u64),
        );
        failed.retain(|page| !done.contains(page));
        translation.insert("failed_pages".into(), json!(failed));
    }
    json!({"id":id, "meta":meta, "item":state["item"], "progress":progress,
        "blocks":blocks.map(Vec::len).unwrap_or(0), "status":status,
        "ratio":ratio, "opened":opened, "notes":notes,
        "open_questions":state["reader"]["notes"].as_object().map(|notes|notes.values().filter(|note|crate::annotation_panel::matches(note, crate::annotation_panel::AnnotationFilter::Unanswered)).count()).unwrap_or(0),
        "translating":state["job"]["type"]=="translate" && state["job"]["state"]=="running",
        "translation":translation, "untranslated_pages":untranslated_pages,
        "cover":meta["cover"].as_str().or_else(|| meta["pages"][0]["img"].as_str()),
        "authors":authors(meta)})
}

fn compare_time(a: &str, b: &str) -> std::cmp::Ordering {
    match (
        chrono::DateTime::parse_from_rfc3339(a),
        chrono::DateTime::parse_from_rfc3339(b),
    ) {
        (Ok(a), Ok(b)) => a.cmp(&b),
        _ => a.cmp(b),
    }
}

pub fn matches(item: &Value, query: &str, folders: &[String]) -> bool {
    let haystack = format!(
        "{} {} {} {} {} {} {}",
        title(item),
        item["meta"]["title_en"].as_str().unwrap_or(""),
        item["meta"]["title_zh"].as_str().unwrap_or(""),
        item["authors"].as_str().unwrap_or(""),
        item["meta"]["abstract"].as_str().unwrap_or(""),
        item["meta"]["abstract_zh"].as_str().unwrap_or(""),
        folders.join(" ")
    )
    .to_lowercase();
    query
        .split_whitespace()
        .all(|word| haystack.contains(&word.to_lowercase()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn citations_use_available_metadata_without_inventing_missing_fields() {
        let item = json!({"authors":"Ashish Vaswani et al.", "meta":{"title_en":"Attention Is All You Need", "year":"2017", "venue":"NIPS"}});
        assert_eq!(
            citation(&item),
            "Ashish Vaswani et al. (2017). Attention Is All You Need. NIPS."
        );
        assert_eq!(citation(&json!({"meta":{"title_en":"Book"}})), "Book.");
    }

    #[test]
    fn publication_sort_uses_numeric_years_and_places_missing_years_last() {
        let mut papers = [
            json!({"meta":{"title_en":"Unknown", "year":"not set"}}),
            json!({"meta":{"title_en":"Old", "year":"2017"}}),
            json!({"meta":{"title_en":"New", "year":2026}}),
            json!({"meta":{"title_en":"Earlier", "year":1999}}),
        ];
        papers.sort_by(|a, b| compare_for_sort(a, b, 2));
        assert_eq!(
            papers.iter().map(title).collect::<Vec<_>>(),
            ["New", "Old", "Earlier", "Unknown"]
        );
        papers.sort_by(|a, b| compare_for_sort(a, b, 3));
        assert_eq!(
            papers.iter().map(title).collect::<Vec<_>>(),
            ["Earlier", "New", "Old", "Unknown"]
        );
        assert_eq!(published_year(&json!({"meta":{"year":0}})), None);
    }

    #[test]
    fn catalog_uses_newest_position_and_preserves_explicit_done() {
        let state = json!({"paper":{"meta":{"title_en":"Paper", "authors":["Ada", {"name":"Lin"}],
            "pages":[{"img":"pages/1.webp"}]},"blocks":[{"id":"a"},{"id":"b"}]},
            "item":{"status":"done","extra":42}, "reader":{"progress":{"block":"a","at":"2026-10-01"},
            "notes":{"n1":{},"n2":{"deleted":true}}}});
        let native = json!({"progress":{"block":"b","at":"2026-10-07"}});
        let item = summary("book", &state, &native);
        assert_eq!(item["status"], "done");
        assert_eq!(item["ratio"], 1.0);
        assert_eq!(item["notes"], 1);
        assert_eq!(item["authors"], "Ada, Lin");
        assert_eq!(item["cover"], "pages/1.webp");
        assert_eq!(item["item"]["extra"], 42);
        assert!(matches(&item, "ada paper", &[]));
        assert!(matches(&item, "Security", &["Security".into()]));
        assert!(!matches(&item, "unknown", &[]));
        let stale = json!({"progress":{"block":"b","at":"2026-09-01"}});
        assert_eq!(summary("book", &state, &stale)["ratio"], 0.5);
        let unknown = json!({"progress":{"block":"removed","at":"2027-01-01"}});
        assert_eq!(summary("book", &state, &unknown)["ratio"], 0.5);
        assert!(compare_time("2026-10-07T00:00:00+08:00", "2026-10-06T18:00:00Z").is_lt());
        let empty = summary("book", &json!({}), &json!({}));
        assert_eq!(empty["ratio"], 0.0);
        assert_eq!(empty["status"], "unread");
    }
}
