use easyread_gpui::{
    library_home,
    model::ReaderState,
    store::{Library, empty_reader, write_json},
};
use serde_json::json;
use std::fs;

#[test]
fn rename_persists_display_name_and_preserves_document_and_reader_data() {
    let temp = tempfile::tempdir().unwrap();
    let lib = Library::open(temp.path()).unwrap();
    let root = lib.root.join("book-1234");
    fs::create_dir(&root).unwrap();
    let paper = json!({"schema":2,"meta":{"title_en":"Original title","title_zh":"原始标题","source":"original.epub","unknown":42},"blocks":[{"id":"p1","type":"para","en":"Body"}],"unknown":{"kept":true}});
    write_json(&root.join("paper.json"), &paper).unwrap();
    let reader = empty_reader();
    write_json(&root.join("reader.json"), &reader).unwrap();
    write_json(
        &root.join("item.json"),
        &json!({"status":"done","starred":true}),
    )
    .unwrap();
    fs::write(root.join("source.epub"), b"unchanged source").unwrap();
    let before: Vec<_> = ["reader.json", "item.json", "source.epub"]
        .iter()
        .map(|name| fs::read(root.join(name)).unwrap())
        .collect();
    let ws = lib.workspace("book-1234").unwrap();
    ws.rename("  我的阅读文件  ").unwrap();
    let mut expected = paper;
    expected["meta"]["display_name"] = json!("我的阅读文件");
    assert_eq!(ws.load("paper").unwrap(), expected);
    for (name, bytes) in ["reader.json", "item.json", "source.epub"]
        .iter()
        .zip(before)
    {
        assert_eq!(fs::read(root.join(name)).unwrap(), bytes);
    }
    let reopened = Library::open(temp.path()).unwrap();
    let item = reopened.list().unwrap().remove(0);
    assert_eq!(library_home::title(&item), "我的阅读文件");
    assert_eq!(
        ReaderState::from_value(ws.state().unwrap())
            .unwrap()
            .title(),
        "我的阅读文件"
    );
    assert!(library_home::matches(&item, "我的阅读", &[]));
    assert!(library_home::matches(&item, "Original title", &[]));
    assert!(library_home::matches(&item, "原始标题", &[]));
    for invalid in ["", "   ", "two\nlines", "bad\0name"] {
        assert!(ws.rename(invalid).is_err());
        assert_eq!(ws.load("paper").unwrap(), expected);
    }
}

#[test]
fn renamed_entries_sort_by_display_name() {
    let mut items = [
        json!({"meta":{"title_en":"A original", "display_name":"Z renamed"}}),
        json!({"meta":{"title_en":"B original"}}),
    ];
    items.sort_by(|a, b| library_home::compare_for_sort(a, b, 3));
    assert_eq!(library_home::title(&items[0]), "B original");
    assert_eq!(
        library_home::title(&json!({"meta":{"display_name":"  ","title_zh":"原名"}})),
        "原名"
    );
}
