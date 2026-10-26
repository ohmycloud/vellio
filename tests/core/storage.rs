use super::*;

#[test]
fn home_catalog_reads_native_progress_and_preserves_source_files() {
    let temp = tempfile::tempdir().unwrap();
    let lib = Library::open(temp.path().join("library")).unwrap();
    let root = lib.root.join("book-home");
    fs::create_dir_all(&root).unwrap();
    let paper = json!({"schema":2,"meta":{"title_en":"Authorization", "authors":"Ada", "abstract":"Access control"},
        "blocks":[{"id":"a","type":"para","en":"First"},{"id":"b","type":"para","en":"Second"}],"unknown":42});
    write_json(&root.join("paper.json"), &paper).unwrap();
    let reader = json!({"schema":2,"progress":{"block":"a","at":"2026-10-01T00:00:00Z"},"edits":{"a":{"zh":"保留"}},"notes":{"n1":{},"n2":{"deleted":true}}});
    write_json(&root.join("reader.json"), &reader).unwrap();
    write_json(
        &root.join("item.json"),
        &json!({"status":"reading","custom":"keep"}),
    )
    .unwrap();
    write_json(
        &root.join(".native-reader.json"),
        &json!({"progress":{"block":"b","offset":25,"at":"2026-10-07T00:00:00Z"}}),
    )
    .unwrap();
    let before = fs::read(root.join("paper.json")).unwrap();
    let listed = lib.list().unwrap();
    assert_eq!(listed[0]["ratio"], 1.0);
    assert_eq!(listed[0]["notes"], 1);
    assert!(easyread_gpui::library_home::matches(
        &listed[0],
        "Ada access",
        &[]
    ));
    let ws = lib.workspace("book-home").unwrap();
    ws.set_reading_status("unread").unwrap();
    ws.update("item", |item| {
        item["starred"] = json!(true);
        Ok(())
    })
    .unwrap();
    let listed = lib.list().unwrap();
    assert_eq!(listed[0]["status"], "unread");
    assert_eq!(listed[0]["item"]["starred"], true);
    assert_eq!(listed[0]["item"]["custom"], "keep");
    assert_eq!(fs::read(root.join("paper.json")).unwrap(), before);
    assert_eq!(ws.load("reader").unwrap(), reader);
}

#[test]
fn import_render_extract_rotation_deduplicate_and_reject_broken() {
    let temp = tempfile::tempdir().unwrap();
    let lib = Library::open(temp.path().join("library")).unwrap();
    for rotation in [0, 90, 180, 270] {
        let pdf = temp.path().join(format!("test-{rotation}.pdf"));
        pdf_fixture(&pdf, rotation, false);
        let ws = lib.import_pdf(&pdf, None).unwrap();
        let state = ws.state().unwrap();
        assert_eq!(state["paper"]["meta"]["page_count"], 1);
        assert!(
            state["paper"]["blocks"][0]["en"]
                .as_str()
                .unwrap()
                .contains("Rust PDF integration")
        );
        let page = image::open(ws.root.join("pages/page-001.webp")).unwrap();
        assert_eq!(page.width(), 1200);
        let chars: Vec<Value> =
            serde_json::from_slice(&fs::read(ws.root.join("extract/page-001.chars.json")).unwrap())
                .unwrap();
        assert!(chars.len() > 30);
        for c in &chars {
            for coordinate in c.as_array().unwrap().iter().skip(1) {
                assert!((0.0..=1.0).contains(&coordinate.as_f64().unwrap()));
            }
            assert!(c[1].as_f64() <= c[3].as_f64());
            assert!(c[2].as_f64() <= c[4].as_f64());
        }
        let duplicate = lib.import_pdf(&pdf, None).unwrap();
        assert_eq!(duplicate.id(), ws.id());
        let range = chars.iter().find(|c| c[0] == "R").unwrap();
        // First glyph must be inside the raster content area at all rotations.
        let (x0, y0, x1, y1) = (
            range[1].as_f64().unwrap(),
            range[2].as_f64().unwrap(),
            range[3].as_f64().unwrap(),
            range[4].as_f64().unwrap(),
        );
        let rgba = page.to_rgb8();
        let mut dark = 0;
        for y in (y0 * page.height() as f64) as u32
            ..((y1 * page.height() as f64).ceil() as u32).min(page.height())
        {
            for x in (x0 * page.width() as f64) as u32
                ..((x1 * page.width() as f64).ceil() as u32).min(page.width())
            {
                if rgba.get_pixel(x, y).0.iter().all(|v| *v < 100) {
                    dark += 1;
                }
            }
        }
        assert!(
            dark > 5,
            "glyph box does not cover rendered glyph at rotation {rotation}"
        );
    }
    let scan = temp.path().join("scan.pdf");
    pdf_fixture(&scan, 0, true);
    let ws = lib.import_pdf(&scan, None).unwrap();
    let scanned = ws.state().unwrap();
    assert_eq!(scanned["paper"]["blocks"][0]["source_kind"], "pdf-page");
    assert_eq!(scanned["paper"]["blocks"][0]["src"], "pages/page-001.webp");
    assert_eq!(scanned["paper"]["blocks"][0]["caption_zh"], "");
    assert_eq!(scanned["paper"]["blocks"][0]["caption_en"], "");
    let broken = temp.path().join("broken.pdf");
    fs::write(&broken, b"%PDF-not-valid").unwrap();
    assert!(lib.import_pdf(&broken, None).is_err());
    assert_eq!(lib.list().unwrap().len(), 5);
}

#[test]
fn operations_are_transactional_monotonic_and_replay_safe() {
    let mut reader = empty_reader();
    reader["future_field"] = json!({"keep":true});
    let ops = json!([{"op":"edit","block":"b","zh":"用户🦀","base":text_hash("原稿"),"at":"2026-10-03T00:00:00.000Z"},{"op":"note","note":{"id":"n","anchor":"b","quote":"用户","updated":"2026-10-03T00:00:00.000Z","future":5}},{"op":"progress","block":"b","ratio":0.5,"at":"2026-10-03T00:00:00.000Z"}]);
    let ops = ops.as_array().unwrap();
    assert_eq!(apply_ops(&mut reader, ops).unwrap().len(), 3);
    let expected = reader.clone();
    assert!(apply_ops(&mut reader, ops).unwrap().is_empty());
    assert_eq!(reader, expected);
    assert!(
        apply_ops(
            &mut reader,
            &[json!({"op":"edit","block":"b","zh":"older","at":"2020"})]
        )
        .unwrap()
        .is_empty()
    );
    assert!(
        apply_ops(
            &mut reader,
            &[
                json!({"op":"edit","block":"b","zh":"next","at":"2030"}),
                json!({"op":"bad"})
            ]
        )
        .is_err()
    );
    assert_eq!(reader, expected);
    apply_ops(
        &mut reader,
        &[
            json!({"op":"edit","block":"b","zh":null,"at":"2031"}),
            json!({"op":"note_del","id":"n","at":"2031"}),
        ],
    )
    .unwrap();
    assert_eq!(reader["edits"]["b"]["prev"], "用户🦀");
    assert_eq!(reader["notes"]["n"]["deleted"], true);
    assert_eq!(reader["future_field"]["keep"], true);
}

#[test]
fn legacy_copy_preserves_unknown_fields_and_reader_ownership() {
    let temp = tempfile::tempdir().unwrap();
    let old = temp.path().join("old");
    fs::create_dir(&old).unwrap();
    pdf_fixture(&old.join("source.pdf"), 0, false);
    let envelope: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/fixtures/state.json"
    )))
    .unwrap();
    let mut paper = envelope["paper"].clone();
    paper["future"] = json!({"preserve":42});
    write_json(&old.join("paper.json"), &paper).unwrap();
    write_json(&old.join("reader.json"), &envelope["reader"]).unwrap();
    let before = fs::read(old.join("reader.json")).unwrap();
    let lib = Library::open(temp.path().join("new")).unwrap();
    let ws = lib.migrate_copy(&old).unwrap();
    ws.reader_ops(&[json!({"op":"paper_note","body":"Rust note","at":"2030"})])
        .unwrap();
    assert_eq!(fs::read(old.join("reader.json")).unwrap(), before);
    assert_eq!(ws.load("paper").unwrap()["future"], json!({"preserve":42}));
    assert!(lib.migrate_copy(&old).is_err());
    let invalid = temp.path().join("invalid-old");
    fs::create_dir(&invalid).unwrap();
    pdf_fixture(&invalid.join("source.pdf"), 90, false);
    write_json(&invalid.join("paper.json"), &paper).unwrap();
    write_json(&invalid.join("reader.json"), &json!({"edits":[]})).unwrap();
    assert!(lib.migrate_copy(&invalid).is_err());
    assert_eq!(lib.list().unwrap().len(), 1);
    assert!(ws.asset("../old/source.pdf").is_err());
    assert!(lib.workspace("../../old").is_err());
}

#[test]
fn concurrent_reader_updates_do_not_lose_notes() {
    let temp = tempfile::tempdir().unwrap();
    let pdf = temp.path().join("test.pdf");
    pdf_fixture(&pdf, 0, false);
    let ws = Arc::new(
        Library::open(temp.path().join("lib"))
            .unwrap()
            .import_pdf(&pdf, None)
            .unwrap(),
    );
    let writers:Vec<_>=(0..12).map(|i|{let ws=ws.clone();thread::spawn(move||ws.reader_ops(&[json!({"op":"note","note":{"id":format!("n{i}"),"updated":"2030","body":format!("note{i}")}})]).unwrap())}).collect();
    for w in writers {
        w.join().unwrap();
    }
    assert_eq!(
        ws.load("reader").unwrap()["notes"]
            .as_object()
            .unwrap()
            .len(),
        12
    );
}

#[test]
fn cancelled_import_never_publishes_partial_workspace() {
    let temp = tempfile::tempdir().unwrap();
    let pdf = temp.path().join("test.pdf");
    pdf_fixture(&pdf, 0, false);
    let lib = Library::open(temp.path().join("lib")).unwrap();
    assert!(
        lib.import_pdf_cancellable(&pdf, None, &AtomicBool::new(true), |_, _| {})
            .is_err()
    );
    assert!(lib.list().unwrap().is_empty());
}

#[test]
fn timestamp_comparison_handles_legacy_timezone_offsets() {
    let mut reader = empty_reader();
    apply_ops(
        &mut reader,
        &[json!({"op":"edit","block":"b","zh":"old","at":"2026-10-03T10:00:00.000+08:00"})],
    )
    .unwrap();
    apply_ops(
        &mut reader,
        &[json!({"op":"edit","block":"b","zh":"new","at":"2026-10-03T04:00:00.000Z"})],
    )
    .unwrap();
    assert_eq!(reader["edits"]["b"]["zh"], "new");
}
