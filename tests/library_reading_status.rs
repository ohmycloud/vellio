use easyread_gpui::store::{Library, write_json};
use serde_json::json;

#[test]
fn library_status_tracks_progress_and_preserves_deliberate_status_changes() {
    let temp = tempfile::tempdir().unwrap();
    let library = Library::open(temp.path().join("library")).unwrap();
    let root = library.root.join("book");
    std::fs::create_dir_all(&root).unwrap();
    write_json(
        &root.join("paper.json"),
        &json!({"schema":2,"meta":{"title_en":"Reading status regression"},
            "blocks":[{"id":"a","type":"para","en":"First"},
                {"id":"b","type":"para","en":"Second"},
                {"id":"c","type":"para","en":"Third"},
                {"id":"d","type":"para","en":"Last"}]}),
    )
    .unwrap();
    write_json(
        &root.join("item.json"),
        &json!({"status":"unread","custom":"keep"}),
    )
    .unwrap();
    let book = library.workspace("book").unwrap();
    let paper_before = std::fs::read(root.join("paper.json")).unwrap();
    assert_eq!(library.list().unwrap()[0]["status"], "unread");

    // Imported books retain unread in their metadata; saved progress drives
    // both the home badge and status filters without a data migration.
    book.reader_ops(&[json!({"op":"progress","block":"a","ratio":0.25,
        "at":"2000-01-01T00:00:00Z"})])
        .unwrap();
    let reader_before = book.load("reader").unwrap();
    let item_before = book.load("item").unwrap();
    let listed = library.list().unwrap();
    assert_eq!(listed[0]["status"], "reading");
    assert_eq!(listed[0]["ratio"], 0.25);
    assert_eq!(book.load("item").unwrap(), item_before);

    // Resetting to unread must work even when the stored status was already
    // unread, and must preserve the reading position for a later return.
    let reset = book.set_reading_status("unread").unwrap();
    assert_eq!(library.list().unwrap()[0]["status"], "unread");
    assert_eq!(book.load("reader").unwrap(), reader_before);
    let reset_at =
        chrono::DateTime::parse_from_rfc3339(reset["status_at"].as_str().unwrap()).unwrap();
    let resumed_at = (reset_at + chrono::Duration::seconds(1)).to_rfc3339();
    book.reader_ops(&[json!({"op":"progress","block":"b","ratio":0.5,"at":resumed_at})])
        .unwrap();
    assert_eq!(library.list().unwrap()[0]["status"], "reading");

    book.set_reading_status("done").unwrap();
    let later = (reset_at + chrono::Duration::seconds(2)).to_rfc3339();
    book.reader_ops(&[json!({"op":"progress","block":"c","ratio":0.75,"at":later})])
        .unwrap();
    assert_eq!(library.list().unwrap()[0]["status"], "done");
    let saved_item = book.load("item").unwrap();
    assert!(book.set_reading_status("invalid").is_err());
    assert_eq!(book.load("item").unwrap(), saved_item);
    assert_eq!(saved_item["custom"], "keep");
    assert_eq!(
        std::fs::read(root.join("paper.json")).unwrap(),
        paper_before
    );
}
