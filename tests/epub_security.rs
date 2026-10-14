use easyread_gpui::store::Library;
use std::{fs, io::Read};
#[path = "support/epub.rs"]
mod fixtures;

#[test]
fn encrypted_content_fixed_layout_and_oversized_entries_are_rejected_transactionally() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("fixture.epub");
    fixtures::fixture(&source, false);
    let mut zip = zip::ZipArchive::new(fs::File::open(&source).unwrap()).unwrap();
    let entries: Vec<(String, Vec<u8>)> = (0..zip.len())
        .map(|i| {
            let mut entry = zip.by_index(i).unwrap();
            let name = entry.name().to_owned();
            let mut bytes = vec![];
            entry.read_to_end(&mut bytes).unwrap();
            (name, bytes)
        })
        .collect();
    drop(zip);
    let lib = Library::open(temp.path().join("library")).unwrap();
    for fixed in [false, true] {
        let mut modified = entries.clone();
        if fixed {
            let opf = modified
                .iter_mut()
                .find(|(name, _)| name.ends_with("content.opf"))
                .unwrap();
            opf.1 = String::from_utf8(opf.1.clone())
                .unwrap()
                .replace(
                    "</metadata>",
                    r#"<meta property="rendition:layout">pre-paginated</meta></metadata>"#,
                )
                .into_bytes();
        } else {
            modified.push(("META-INF/encryption.xml".into(), br#"<encryption xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><EncryptedData xmlns="http://www.w3.org/2001/04/xmlenc#"><CipherData><CipherReference URI="OEBPS/Text/first.xhtml"/></CipherData></EncryptedData></encryption>"#.to_vec()));
        }
        let borrowed: Vec<_> = modified
            .iter()
            .map(|(name, bytes)| (name.as_str(), bytes.as_slice()))
            .collect();
        fixtures::archive(&source, &borrowed);
        let error = lib.import_file(&source, None).unwrap_err().to_string();
        assert!(
            error.contains(if fixed { "Fixed-layout" } else { "Encrypted" }),
            "{error}"
        );
        assert_eq!(fs::read_dir(&lib.root).unwrap().count(), 0);
    }
    fixtures::fixture(&source, false);
    let mut bytes = fs::read(&source).unwrap();
    let central = bytes.windows(4).position(|w| w == b"PK\x01\x02").unwrap();
    // A huge declared uncompressed size is rejected before decompression.
    bytes[central + 24..central + 28].copy_from_slice(&(65u32 * 1024 * 1024).to_le_bytes());
    fs::write(&source, &bytes).unwrap();
    assert!(
        lib.import_file(&source, None)
            .unwrap_err()
            .to_string()
            .contains("size limit")
    );
    assert_eq!(fs::read_dir(&lib.root).unwrap().count(), 0);
}
