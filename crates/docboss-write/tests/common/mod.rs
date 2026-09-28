#![allow(dead_code)]

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use docboss_model::{
    Block, Break, Comment, Document, Drawing, DrawingPlacement, Field, Inline, Justification,
    RevisionKind, Run, RunContent, RunProperties, SectionBreak, SectionProperties, TableCell,
    TableCellProperties, TableRow, VerticalMerge,
};
use docboss_write::{DocumentBuilder, ListKind, Para, TableBuilder};

/// A 2x2 RGB PNG written by hand.
pub fn tiny_png() -> Vec<u8> {
    fn chunk(out: &mut Vec<u8>, kind: &[u8], data: &[u8]) {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let mut body = kind.to_vec();
        body.extend_from_slice(data);
        out.extend_from_slice(&body);
        out.extend_from_slice(&docboss_write::zip::crc32(&body).to_be_bytes());
    }
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut header = Vec::new();
    header.extend_from_slice(&2u32.to_be_bytes());
    header.extend_from_slice(&2u32.to_be_bytes());
    header.extend_from_slice(&[8, 2, 0, 0, 0]);
    chunk(&mut png, b"IHDR", &header);
    let raw = [0u8, 255, 0, 0, 0, 255, 0, 0, 0, 0, 255, 255, 255, 255];
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(&raw).unwrap();
    chunk(&mut png, b"IDAT", &encoder.finish().unwrap());
    chunk(&mut png, b"IEND", &[]);
    png
}

/// A document exercising every construct the writer serializes.
pub fn kitchen_sink() -> Document {
    let mut builder = DocumentBuilder::new();
    builder.title("Kitchen sink").author("docboss");
    builder.heading(0, "Kitchen sink title");
    builder.heading(1, "First heading");
    builder.paragraph(
        Para::new()
            .text("Plain ")
            .bold("bold ")
            .italic("italic ")
            .underline("underlined ")
            .link("https://example.com/a?b=1&c=2", "a link")
            .text(" & <escaped> text")
            .tab()
            .text("after tab")
            .line_break()
            .text("second line"),
    );
    let note = builder.footnote(Para::new().text("Footnote body text."));
    builder.paragraph(
        Para::new()
            .text("Has a footnote")
            .footnote_ref(note)
            .align(Justification::Center),
    );
    builder.items(ListKind::Bullet, &["bullet one", "bullet two"]);
    let numbered = builder.list(ListKind::Numbered);
    builder.paragraph(Para::new().text("numbered one").list(numbered, 0));
    builder.paragraph(Para::new().text("nested a").list(numbered, 1));
    builder.paragraph(Para::new().text("numbered two").list(numbered, 0));
    let width = builder.text_width();
    let mut table = TableBuilder::new(3, width)
        .header(&["H1", "H2", "H3"])
        .row(&["a", "b", "c"])
        .build();
    table.rows.push(TableRow {
        cells: vec![
            TableCell {
                properties: TableCellProperties {
                    grid_span: 2,
                    vertical_merge: Some(VerticalMerge::Restart),
                    ..Default::default()
                },
                blocks: vec![Para::new().text("merged").into()],
            },
            TableCell {
                properties: TableCellProperties::default(),
                blocks: vec![],
            },
        ],
        ..Default::default()
    });
    builder.block(Block::Table(table));
    builder.picture(tiny_png());
    let media = builder.image(tiny_png());
    builder.paragraph(Para::new().text("Anchored picture").content(
        RunContent::Drawing(Drawing {
            media: Some(media),
            width: 914400,
            height: 914400,
            placement: DrawingPlacement::Anchored {
                x: 914400,
                y: 0,
                behind_text: false,
                relative_to_page: false,
            },
            name: Some("anchored".into()),
            description: Some("An anchored picture".into()),
            text_box: Vec::new(),
        }),
        RunProperties::default(),
    ));
    builder.paragraph(
        Para::new()
            .text("Page ")
            .inline(Inline::Field(Field {
                instruction: " PAGE ".into(),
                result: vec![Inline::Run(Run {
                    properties: RunProperties::default(),
                    content: vec![RunContent::Text("1".into())],
                })],
            }))
            .inline(Inline::Revision(docboss_model::Revision {
                kind: RevisionKind::Insertion,
                author: Some("Ann".into()),
                date: Some("2024-01-01T00:00:00Z".into()),
                inlines: vec![Inline::Run(Run {
                    properties: RunProperties::default(),
                    content: vec![RunContent::Text(" inserted".into())],
                })],
            }))
            .inline(Inline::Revision(docboss_model::Revision {
                kind: RevisionKind::Deletion,
                author: Some("Ann".into()),
                date: None,
                inlines: vec![Inline::Run(Run {
                    properties: RunProperties::default(),
                    content: vec![RunContent::Text(" deleted".into())],
                })],
            }))
            .inline(Inline::BookmarkStart {
                id: 0,
                name: "mark".into(),
            })
            .text(" bookmarked")
            .inline(Inline::BookmarkEnd { id: 0 })
            .inline(Inline::CommentRangeStart(0))
            .text(" commented")
            .inline(Inline::CommentRangeEnd(0))
            .content(RunContent::CommentReference(0), RunProperties::default())
            .content(
                RunContent::Symbol {
                    font: Some("Symbol".into()),
                    char: 0xF0B7,
                },
                RunProperties::default(),
            ),
    );
    builder
        .header(Para::new().text("Header text"))
        .footer(Para::new().text("Footer text"));
    builder.section(SectionProperties {
        start: SectionBreak::NextPage,
        ..Default::default()
    });
    builder.landscape();
    builder.text("Second section, landscape.");
    builder.paragraph(
        Para::new()
            .text("Page break next")
            .content(RunContent::Break(Break::Page), RunProperties::default()),
    );
    builder.text("After the page break.");
    let mut document = builder.build();
    document.comments.push(Comment {
        id: 0,
        author: Some("Reviewer".into()),
        initials: Some("R".into()),
        date: Some("2024-01-02T00:00:00Z".into()),
        blocks: vec![Para::new().text("A comment.").into()],
    });
    document
}

pub fn soffice() -> Option<&'static str> {
    [
        "soffice",
        "/opt/homebrew/bin/soffice",
        "/usr/bin/soffice",
        "/Applications/LibreOffice.app/Contents/MacOS/soffice",
    ]
    .into_iter()
    .find(|candidate| {
        Command::new(candidate)
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success())
    })
}

pub fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Converts every DOCX in `dir` with LibreOffice and returns the output
/// directory.
pub fn convert(soffice: &str, dir: &Path, format: &str) -> PathBuf {
    let out = dir.join(format);
    let profile = dir.join("profile");
    let inputs: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| path.extension().is_some_and(|e| e == "docx"))
        .collect();
    let status = Command::new(soffice)
        .arg(format!(
            "-env:UserInstallation=file://{}",
            profile.display()
        ))
        .args(["--headless", "--convert-to", format, "--outdir"])
        .arg(&out)
        .args(&inputs)
        .output()
        .unwrap();
    assert!(
        status.status.success(),
        "{}",
        String::from_utf8_lossy(&status.stderr)
    );
    out
}

/// The entries of a ZIP archive by walking its local headers.
pub fn unzip(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
    use std::io::Read;
    let mut entries = Vec::new();
    let mut at = 0;
    let u16_at = |at: usize| u16::from_le_bytes([bytes[at], bytes[at + 1]]) as usize;
    let u32_at = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
    while bytes[at..].starts_with(b"PK\x03\x04") {
        let method = u16_at(at + 8);
        let compressed = u32_at(at + 18);
        let name_len = u16_at(at + 26);
        let extra = u16_at(at + 28);
        let name = String::from_utf8(bytes[at + 30..at + 30 + name_len].to_vec()).unwrap();
        let start = at + 30 + name_len + extra;
        let raw = &bytes[start..start + compressed];
        let data = match method {
            0 => raw.to_vec(),
            _ => {
                let mut out = Vec::new();
                flate2::read::DeflateDecoder::new(raw)
                    .read_to_end(&mut out)
                    .unwrap();
                out
            }
        };
        entries.push((name, data));
        at = start + compressed;
    }
    entries
}

pub fn part(bytes: &[u8], name: &str) -> String {
    let entries = unzip(bytes);
    let (_, data) = entries
        .iter()
        .find(|(n, _)| n == name)
        .unwrap_or_else(|| panic!("no part {name}"));
    String::from_utf8(data.clone()).unwrap()
}

/// The text of a PDF through the pdfboss CLI, when it is installed.
pub fn pdf_text(path: &Path) -> Option<String> {
    let output = Command::new("pdfboss")
        .arg("text")
        .arg(path)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}
