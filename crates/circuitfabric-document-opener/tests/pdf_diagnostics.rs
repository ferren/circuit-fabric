use std::process::Command;

// Run extraction in a child process so direct writes to stdout/stderr cannot escape
// the assertion through the test harness's output capture.
#[test]
fn notdef_font_entries_do_not_flood_the_console() {
    const CHILD_FLAG: &str = "CIRCUITFABRIC_PDF_DIAGNOSTICS_CHILD";
    if std::env::var_os(CHILD_FLAG).is_some() {
        let bytes = times_roman_pdf();
        let pages = pdf_extract::extract_text_from_mem_by_pages(&bytes).expect("extract pages");
        assert_eq!(pages.len(), 1);
        assert!(pages[0].contains("LM317 voltage regulator"), "{pages:?}");
        let text = pdf_extract::extract_text_from_mem(&bytes).expect("extract text");
        assert!(text.contains("LM317 voltage regulator"), "{text:?}");
        return;
    }

    let output = Command::new(std::env::current_exe().expect("test executable"))
        .args(["--exact", "notdef_font_entries_do_not_flood_the_console", "--nocapture"])
        .env(CHILD_FLAG, "1")
        .output()
        .expect("run extraction subprocess");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "stdout: {stdout}\nstderr: {stderr}");
    assert!(!stdout.contains("unknown glyph name"), "stdout: {stdout}");
    assert!(!stderr.contains("unknown glyph name"), "stderr: {stderr}");
}

fn times_roman_pdf() -> Vec<u8> {
    use pdf_extract::{Dictionary, Document, Object, Stream};

    let mut pdf = Document::with_version("1.4");
    let pages_id = pdf.new_object_id();
    let mut encoding = Dictionary::new();
    encoding.set("BaseEncoding", Object::Name(b"WinAnsiEncoding".to_vec()));
    let mut differences = vec![Object::Integer(0)];
    differences.extend((0..32).map(|_| Object::Name(b".notdef".to_vec())));
    encoding.set("Differences", Object::Array(differences));
    let mut font = Dictionary::new();
    font.set("Type", Object::Name(b"Font".to_vec()));
    font.set("Subtype", Object::Name(b"Type1".to_vec()));
    font.set("BaseFont", Object::Name(b"Times-Roman".to_vec()));
    font.set("Encoding", encoding);
    let font_id = pdf.add_object(font);
    let mut fonts = Dictionary::new();
    fonts.set("F1", font_id);
    let mut resources = Dictionary::new();
    resources.set("Font", fonts);
    let content_id = pdf.add_object(Stream::new(
        Dictionary::new(),
        b"BT /F1 12 Tf 72 720 Td (LM317 voltage regulator) Tj ET".to_vec(),
    ));
    let mut page = Dictionary::new();
    page.set("Type", Object::Name(b"Page".to_vec()));
    page.set("Parent", pages_id);
    page.set("MediaBox", vec![0.into(), 0.into(), 612.into(), 792.into()]);
    page.set("Resources", resources);
    page.set("Contents", content_id);
    let page_id = pdf.add_object(page);
    let mut pages = Dictionary::new();
    pages.set("Type", Object::Name(b"Pages".to_vec()));
    pages.set("Kids", vec![Object::Reference(page_id)]);
    pages.set("Count", 1);
    pdf.objects.insert(pages_id, Object::Dictionary(pages));
    let mut catalog = Dictionary::new();
    catalog.set("Type", Object::Name(b"Catalog".to_vec()));
    catalog.set("Pages", pages_id);
    let catalog_id = pdf.add_object(catalog);
    pdf.trailer.set("Root", catalog_id);
    let mut bytes = Vec::new();
    pdf.save_to(&mut bytes).expect("serialize fixture");
    bytes
}
