use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn fixture(crate_name: &str, name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(crate_name)
        .join("tests/fixtures")
        .join(name)
}

fn docx() -> PathBuf {
    fixture("docboss-docx", "libreoffice-rich.docx")
}

fn doc() -> PathBuf {
    fixture("docboss-doc", "lists.doc")
}

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("docboss-cli-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn docboss(args: &[&std::ffi::OsStr]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_docboss"))
        .args(args)
        .output()
        .unwrap()
}

fn run(args: &[&str]) -> Output {
    let args: Vec<&std::ffi::OsStr> = args.iter().map(std::ffi::OsStr::new).collect();
    docboss(&args)
}

fn stdout(output: &Output) -> String {
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout.clone()).unwrap()
}

fn path(p: &Path) -> &str {
    p.to_str().unwrap()
}

#[test]
fn info_reports_format_and_counts_for_both_formats() {
    let info = stdout(&run(&["info", path(&docx())]));
    assert!(info.contains("DOCX"));
    assert!(info.contains("title:           Fixture Document"));
    assert!(info.contains("pages:           1"));
    let info = stdout(&run(&["info", path(&doc())]));
    assert!(info.contains("DOC (Word 97-2003 binary)"));
    assert!(info.contains("author:          Ada Lovelace"));
}

#[test]
fn text_carries_list_labels_and_can_drop_them() {
    let text = stdout(&run(&["text", path(&doc())]));
    assert!(text.contains("1. First item"));
    assert!(text.contains("a) Nested a"));
    let bare = stdout(&run(&["text", path(&doc()), "--no-labels"]));
    assert!(bare.contains("First item") && !bare.contains("1. First item"));
}

#[test]
fn markdown_exports_images_it_links() {
    let dir = scratch("md");
    let media = dir.join("media");
    let md = stdout(&run(&["md", path(&docx()), "--images", path(&media)]));
    assert!(md.contains("# Fixture Heading"));
    assert!(md.contains("**bold words**"));
    assert!(md.contains("[^1]: Footnote text."));
    assert!(md.contains("media/image1.png"));
    assert!(media.join("image1.png").exists());
}

#[test]
fn html_and_json_and_blocks() {
    let html = stdout(&run(&["html", path(&docx()), "--standalone"]));
    assert!(html.contains("<html") && html.contains("<h1"));
    let json = stdout(&run(&["json", path(&docx()), "--compact"]));
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["metadata"]["title"], "Fixture Document");
    let blocks = stdout(&run(&["json", path(&docx()), "--blocks"]));
    let value: serde_json::Value = serde_json::from_str(&blocks).unwrap();
    assert_eq!(value[0]["text"], "Fixture Heading");
}

#[test]
fn q_runs_jq_and_reports_bad_programs_with_exit_two() {
    let title = stdout(&run(&["q", path(&docx()), "-r", ".metadata.title"]));
    assert_eq!(title, "Fixture Document\n");
    let count = stdout(&run(&["q", path(&doc()), "--blocks", "length"]));
    assert_eq!(count.trim(), "6");
    let bad = run(&["q", path(&docx()), ".["]);
    assert_eq!(bad.status.code(), Some(2));
}

#[test]
fn render_writes_every_format_and_numbers_pages() {
    let dir = scratch("render");
    for name in ["p.png", "p.ppm", "p.bmp", "p.jpg"] {
        let out = dir.join(name);
        stdout(&run(&[
            "render",
            path(&docx()),
            "--page",
            "1",
            "-o",
            path(&out),
        ]));
        let bytes = std::fs::read(&out).unwrap();
        let magic: &[u8] = match name {
            "p.png" => b"\x89PNG",
            "p.ppm" => b"P6",
            "p.bmp" => b"BM",
            _ => b"\xff\xd8\xff",
        };
        assert!(bytes.starts_with(magic), "{name}");
    }
    let pattern = dir.join("all-%d.png");
    let listed = stdout(&run(&[
        "render",
        path(&doc()),
        "-o",
        path(&pattern),
        "--scale",
        "0.5",
    ]));
    assert!(listed.contains("all-1.png"));
    assert!(dir.join("all-1.png").exists());
    let missing = run(&[
        "render",
        path(&doc()),
        "--page",
        "99",
        "-o",
        path(&dir.join("x.png")),
    ]);
    assert_eq!(missing.status.code(), Some(1));
    let bad_quality = run(&[
        "render",
        path(&doc()),
        "-o",
        path(&dir.join("x.png")),
        "--jpeg-quality",
        "50",
    ]);
    assert!(!bad_quality.status.success());
}

#[test]
fn convert_doc_to_docx_keeps_the_text() {
    let dir = scratch("convert");
    let out = dir.join("lists.docx");
    stdout(&run(&["convert", path(&doc()), "-o", path(&out)]));
    let original = stdout(&run(&["text", path(&doc())]));
    let converted = stdout(&run(&["text", path(&out)]));
    assert_eq!(original, converted);
    let md = dir.join("lists.md");
    stdout(&run(&["convert", path(&doc()), "-o", path(&md)]));
    assert!(std::fs::read_to_string(md).unwrap().contains("First item"));
    let unknown = run(&["convert", path(&doc()), "-o", path(&dir.join("x.pdf"))]);
    assert_eq!(unknown.status.code(), Some(1));
}

#[test]
fn create_md_composes_a_readable_docx() {
    let dir = scratch("create");
    let source = dir.join("notes.md");
    std::fs::write(
        &source,
        "# Notes\n\nSome **bold** text.\n\n1. one\n2. two\n",
    )
    .unwrap();
    let out = dir.join("notes.docx");
    stdout(&run(&[
        "create",
        "md",
        path(&source),
        "-o",
        path(&out),
        "--size",
        "a4",
    ]));
    let md = stdout(&run(&["md", path(&out)]));
    assert!(md.contains("# Notes"));
    assert!(md.contains("**bold**"));
    assert!(md.contains("1. one"));
}

#[test]
fn explorer_lists_parts_and_dumps_them() {
    let parts = stdout(&run(&["parts", path(&docx())]));
    assert!(parts.contains("word/document.xml"));
    let streams = stdout(&run(&["parts", path(&doc())]));
    assert!(streams.contains("WordDocument") && streams.contains("\\x05SummaryInformation"));
    let xml = stdout(&run(&["xml", path(&docx()), "word/document.xml"]));
    assert!(xml.contains("\n  <w:body>\n"));
    let hex = stdout(&run(&[
        "hex",
        path(&doc()),
        "WordDocument",
        "--length",
        "16",
    ]));
    assert!(hex.starts_with("00000000  ec a5"));
    let summary = stdout(&run(&[
        "hex",
        path(&doc()),
        "SummaryInformation",
        "--length",
        "2",
    ]));
    assert!(summary.starts_with("00000000  fe ff"));
    let missing = run(&["xml", path(&docx()), "word/nothing.xml"]);
    assert_eq!(missing.status.code(), Some(1));
}

#[test]
fn images_fonts_and_diagnostics() {
    let dir = scratch("images");
    let listed = stdout(&run(&["images", path(&docx()), "-o", path(&dir)]));
    assert!(listed.contains("image1.png"));
    assert!(std::fs::read(dir.join("image1.png"))
        .unwrap()
        .starts_with(b"\x89PNG"));
    let fonts = stdout(&run(&["fonts", path(&docx())]));
    assert!(fonts.contains("Liberation Serif -> "));
    let diagnostics = run(&["diagnostics", path(&doc()), "--layout"]);
    assert!(diagnostics.status.success());
}

#[test]
fn encrypted_doc_opens_only_with_its_password() {
    let encrypted = fixture("docboss-doc", "encrypted-rc4.doc");
    let refused = run(&["text", path(&encrypted)]);
    assert_eq!(refused.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&refused.stderr).starts_with("docboss: "));
    let opened = run(&["text", path(&encrypted), "--password", "tika"]);
    assert!(
        opened.status.success(),
        "{}",
        String::from_utf8_lossy(&opened.stderr)
    );
}

#[test]
fn unreadable_input_fails_without_panicking() {
    let dir = scratch("bad");
    let junk = dir.join("junk.docx");
    std::fs::write(&junk, b"PK\x03\x04 this is not really a zip").unwrap();
    for command in ["info", "text", "md", "html", "json", "render"] {
        let output = run(&[command, path(&junk)]);
        assert_eq!(output.status.code(), Some(1), "{command}");
        assert!(!String::from_utf8_lossy(&output.stderr).contains("panicked"));
    }
    let missing = run(&["text", "/no/such/file.docx"]);
    assert_eq!(missing.status.code(), Some(1));
}

#[test]
fn skill_prints_the_bundled_document() {
    let skill = stdout(&run(&["skill", "print"]));
    assert!(skill.starts_with("---\nname: docboss\n"));
    let bundled = include_str!("../skill/SKILL.md");
    let published = include_str!("../../../skills/docboss/SKILL.md");
    assert_eq!(bundled, published);
}
