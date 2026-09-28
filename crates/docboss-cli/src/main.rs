//! The `docboss` command-line tool: document info, text, Markdown, HTML and
//! JSON extraction, page rendering, conversion to DOCX and container
//! inspection.

mod container;
mod fonts;
mod hexdump;
mod q;
mod render;
mod skill;
mod xmlpretty;

use std::io::{IsTerminal, Write as _};
use std::path::{Path, PathBuf};

use clap::{Args, Parser, Subcommand, ValueEnum};
use docboss_core::{Block, Document, Options, PageSize, Severity, SourceFormat};
use docboss_output::{HtmlOptions, ImageMode, MarkdownOptions, TextOptions};

/// A fatal CLI failure: message for stderr plus the process exit code.
/// Document and I/O problems exit 1; invalid jq programs exit 2, the code
/// clap uses for usage errors.
pub struct Failure {
    pub message: String,
    pub code: i32,
}

impl Failure {
    pub fn new(message: impl Into<String>) -> Failure {
        Failure {
            message: message.into(),
            code: 1,
        }
    }

    pub fn program(message: impl Into<String>) -> Failure {
        Failure {
            message: message.into(),
            code: 2,
        }
    }
}

impl From<String> for Failure {
    fn from(message: String) -> Failure {
        Failure::new(message)
    }
}

#[derive(Parser)]
#[command(
    name = "docboss",
    version,
    about = "DOCX and DOC parsing, extraction, rendering and conversion"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

/// A document to read.
#[derive(Args)]
struct Source {
    /// Path to the .docx or .doc file; the format is detected from its bytes.
    file: PathBuf,
    /// Password for an encrypted DOC.
    #[arg(long)]
    password: Option<String>,
}

/// Which parts besides the main text an extraction includes.
#[derive(Args)]
struct Stories {
    /// Include each section's headers and footers.
    #[arg(long)]
    headers_footers: bool,
    /// Leave out footnotes and endnotes.
    #[arg(long)]
    no_notes: bool,
    /// Include comments.
    #[arg(long)]
    comments: bool,
}

#[derive(Clone, Copy, ValueEnum)]
enum Paper {
    Letter,
    A4,
}

impl Paper {
    fn size(self) -> PageSize {
        match self {
            Paper::Letter => PageSize::default(),
            Paper::A4 => PageSize {
                width: 11906,
                height: 16838,
                ..PageSize::default()
            },
        }
    }
}

#[derive(Subcommand)]
enum CreateCommand {
    /// Compose CommonMark + GFM Markdown into a DOCX.
    Md {
        /// The Markdown file; relative image paths resolve against its directory.
        file: PathBuf,
        /// Output DOCX path.
        #[arg(short, long)]
        out: PathBuf,
        /// Page size.
        #[arg(long, value_enum, default_value = "letter")]
        size: Paper,
        /// Document title; the first heading's text by default.
        #[arg(long)]
        title: Option<String>,
    },
}

#[derive(Subcommand)]
enum Command {
    /// Format, metadata, page setup, page count and content counts.
    Info {
        #[command(flatten)]
        source: Source,
    },
    /// Extract plain text.
    Text {
        #[command(flatten)]
        source: Source,
        #[command(flatten)]
        stories: Stories,
        /// Leave list labels (1., a), bullets) out.
        #[arg(long)]
        no_labels: bool,
        /// Write to a file instead of stdout.
        #[arg(short, long)]
        out: Option<PathBuf>,
    },
    /// Convert to GitHub-flavored Markdown.
    Md {
        #[command(flatten)]
        source: Source,
        #[command(flatten)]
        stories: Stories,
        /// Export images into this directory and link them from the Markdown.
        #[arg(long, conflicts_with = "embed_images")]
        images: Option<PathBuf>,
        /// Embed images as data: URIs.
        #[arg(long)]
        embed_images: bool,
        /// Start with the metadata title as a level-one heading.
        #[arg(long)]
        title: bool,
        /// Write page breaks as thematic breaks (---).
        #[arg(long)]
        page_breaks: bool,
        /// Write to a file instead of stdout.
        #[arg(short, long)]
        out: Option<PathBuf>,
    },
    /// Convert to semantic HTML; images are embedded unless --images is given.
    Html {
        #[command(flatten)]
        source: Source,
        #[command(flatten)]
        stories: Stories,
        /// Write a whole HTML document instead of a body fragment.
        #[arg(long)]
        standalone: bool,
        /// Export images into this directory and link them instead of embedding.
        #[arg(long)]
        images: Option<PathBuf>,
        /// Write to a file instead of stdout.
        #[arg(short, long)]
        out: Option<PathBuf>,
    },
    /// Dump the document model as JSON.
    Json {
        #[command(flatten)]
        source: Source,
        /// The compact per-block view: type, style, heading level, list label, text.
        #[arg(long)]
        blocks: bool,
        /// One line instead of indented output.
        #[arg(long)]
        compact: bool,
        /// Write to a file instead of stdout.
        #[arg(short, long)]
        out: Option<PathBuf>,
    },
    /// Run a jq program over the JSON form of the document.
    Q {
        #[command(flatten)]
        source: Source,
        /// The jq program, e.g. '.metadata.title' or '.sections[].blocks | length'.
        program: String,
        /// Print strings without quotes.
        #[arg(short, long)]
        raw: bool,
        /// Query the compact per-block view instead of the full model.
        #[arg(long)]
        blocks: bool,
    },
    /// Lay out and render pages to PNG, PPM, BMP or JPEG (picked by the -o extension).
    Render {
        #[command(flatten)]
        source: Source,
        /// Page number (1-based); every page when omitted.
        #[arg(long)]
        page: Option<usize>,
        /// Pixels per point: 1.0 is 72 dpi.
        #[arg(long, default_value_t = 1.0)]
        scale: f32,
        /// Output path; %d is replaced by the page number.
        #[arg(short, long, default_value = "page-%d.png")]
        out: String,
        /// JPEG quality, 1 to 100.
        #[arg(long, value_parser = clap::value_parser!(u8).range(1..=100))]
        jpeg_quality: Option<u8>,
    },
    /// Export the document's images at their stored size and format.
    Images {
        #[command(flatten)]
        source: Source,
        /// Output directory.
        #[arg(short, long, default_value = ".")]
        out: PathBuf,
    },
    /// List the ZIP entries or compound file streams of the container.
    Parts {
        /// Path to the .docx or .doc file.
        file: PathBuf,
    },
    /// Hexdump the whole file or one part of it.
    Hex {
        /// Path to the .docx or .doc file.
        file: PathBuf,
        /// A ZIP entry (word/document.xml) or compound file stream (1Table).
        part: Option<String>,
        /// First byte to dump.
        #[arg(long, default_value_t = 0)]
        offset: u64,
        /// Number of bytes to dump; to the end when omitted.
        #[arg(long)]
        length: Option<u64>,
        /// Bytes per row.
        #[arg(long, default_value_t = 16)]
        width: usize,
    },
    /// Print an XML part of a DOCX, indented.
    Xml {
        /// Path to the .docx file.
        file: PathBuf,
        /// The part, e.g. word/document.xml.
        #[arg(default_value = "word/document.xml")]
        part: String,
        /// Print the part as stored, without re-indenting.
        #[arg(long)]
        raw: bool,
    },
    /// Convert to a fresh DOCX, or to .md, .html, .txt or .json, by the -o extension.
    Convert {
        #[command(flatten)]
        source: Source,
        /// Output path.
        #[arg(short, long)]
        out: PathBuf,
    },
    /// Create a new DOCX.
    Create {
        #[command(subcommand)]
        command: CreateCommand,
    },
    /// The fonts the document names and the faces they resolve to here.
    Fonts {
        #[command(flatten)]
        source: Source,
    },
    /// Everything the lenient reader approximated or dropped.
    Diagnostics {
        #[command(flatten)]
        source: Source,
        /// Also lay the document out and report what layout approximated.
        #[arg(long)]
        layout: bool,
    },
    /// Install or print the skill document for coding agents.
    Skill {
        #[command(subcommand)]
        command: skill::SkillCommand,
    },
    /// Explore the document interactively in the terminal.
    Tui {
        #[command(flatten)]
        source: Source,
    },
}

fn main() {
    let cli = Cli::parse();
    let Err(failure) = run(cli.command) else {
        return;
    };
    eprintln!("docboss: {}", failure.message);
    std::process::exit(failure.code);
}

fn run(command: Command) -> Result<(), Failure> {
    match command {
        Command::Info { source } => cmd_info(&source),
        Command::Text {
            source,
            stories,
            no_labels,
            out,
        } => {
            let document = load(&source)?;
            let options = TextOptions {
                list_labels: !no_labels,
                headers_footers: stories.headers_footers,
                notes: !stories.no_notes,
                comments: stories.comments,
            };
            emit(
                out.as_deref(),
                &docboss_output::to_text(&document, &options),
            )
        }
        Command::Md {
            source,
            stories,
            images,
            embed_images,
            title,
            page_breaks,
            out,
        } => {
            let document = load(&source)?;
            let mode = image_mode(&document, images.as_deref(), embed_images, ImageMode::Omit)?;
            let options = MarkdownOptions {
                title,
                page_breaks,
                images: mode,
                headers_footers: stories.headers_footers,
                notes: !stories.no_notes,
                comments: stories.comments,
            };
            emit(
                out.as_deref(),
                &docboss_output::to_markdown(&document, &options),
            )
        }
        Command::Html {
            source,
            stories,
            standalone,
            images,
            out,
        } => {
            let document = load(&source)?;
            let options = HtmlOptions {
                standalone,
                images: image_mode(&document, images.as_deref(), false, ImageMode::Embed)?,
                headers_footers: stories.headers_footers,
                notes: !stories.no_notes,
                comments: stories.comments,
            };
            emit(
                out.as_deref(),
                &docboss_output::to_html(&document, &options),
            )
        }
        Command::Json {
            source,
            blocks,
            compact,
            out,
        } => {
            let document = load(&source)?;
            let mut json = json_of(&document, blocks, !compact)?;
            json.push('\n');
            emit(out.as_deref(), &json)
        }
        Command::Q {
            source,
            program,
            raw,
            blocks,
        } => cmd_q(&source, &program, raw, blocks),
        Command::Render {
            source,
            page,
            scale,
            out,
            jpeg_quality,
        } => {
            let document = load(&source)?;
            render::cmd_render(&document, page, scale, &out, jpeg_quality)
        }
        Command::Images { source, out } => {
            let document = load(&source)?;
            let written = export_media(&document, &out)?;
            written
                .iter()
                .for_each(|path| println!("{}", path.display()));
            Ok(())
        }
        Command::Parts { file } => cmd_parts(&file),
        Command::Hex {
            file,
            part,
            offset,
            length,
            width,
        } => cmd_hex(&file, part.as_deref(), offset, length, width),
        Command::Xml { file, part, raw } => {
            let bytes = read_file(&file)?;
            let data = container::part_bytes(&bytes, &part)?;
            let text = docboss_xml::decode(&data);
            let text = if raw {
                text.into_owned()
            } else {
                xmlpretty::pretty(&text, 2)
            };
            emit(None, &text)
        }
        Command::Convert { source, out } => cmd_convert(&source, &out),
        Command::Create { command } => cmd_create(command),
        Command::Fonts { source } => {
            let document = load(&source)?;
            let database = docboss_layout::fonts_for(&document);
            for resolution in fonts::resolve(&document, &database) {
                let face = resolution.face.as_deref().unwrap_or("(no face)");
                let embedded = if resolution.embedded {
                    " (embedded)"
                } else {
                    ""
                };
                println!("{} -> {face}{embedded}", resolution.requested);
            }
            Ok(())
        }
        Command::Diagnostics { source, layout } => {
            let document = load(&source)?;
            let mut diagnostics = document.diagnostics.clone();
            if layout {
                diagnostics.extend(docboss_layout::layout_document(&document).diagnostics);
            }
            for diagnostic in &diagnostics {
                let severity = match diagnostic.severity {
                    Severity::Approximated => "approximated",
                    Severity::Dropped => "dropped",
                };
                println!("{severity} {}: {}", diagnostic.location, diagnostic.message);
            }
            Ok(())
        }
        Command::Skill { command } => skill::cmd_skill(command).map_err(Failure::new),
        Command::Tui { source } => cmd_tui(&source),
    }
}

fn cmd_tui(source: &Source) -> Result<(), Failure> {
    let bytes = read_file(&source.file)?;
    let options = Options {
        password: source.password.clone(),
    };
    let document = docboss_core::read_with(&bytes, &options)
        .map_err(|e| Failure::new(format!("{}: {e}", source.file.display())))?;
    let target = source.file.display().to_string();
    docboss_tui::run(document, bytes, target).map_err(|e| Failure::new(e.to_string()))
}

fn read_file(path: &Path) -> Result<Vec<u8>, Failure> {
    std::fs::read(path).map_err(|e| Failure::new(format!("{}: {e}", path.display())))
}

fn load(source: &Source) -> Result<Document, Failure> {
    let bytes = read_file(&source.file)?;
    let options = Options {
        password: source.password.clone(),
    };
    docboss_core::read_with(&bytes, &options)
        .map_err(|e| Failure::new(format!("{}: {e}", source.file.display())))
}

/// Writes `text` to `out`, or to stdout when `out` is `None`. A closed
/// stdout pipe (`docboss text x.docx | head`) ends the output quietly.
fn emit(out: Option<&Path>, text: &str) -> Result<(), Failure> {
    if let Some(path) = out {
        return std::fs::write(path, text)
            .map_err(|e| Failure::new(format!("{}: {e}", path.display())));
    }
    let mut stdout = std::io::stdout().lock();
    let written = stdout
        .write_all(text.as_bytes())
        .and_then(|()| stdout.flush());
    match written {
        Err(e) if e.kind() != std::io::ErrorKind::BrokenPipe => {
            Err(Failure::new(format!("stdout: {e}")))
        }
        _ => Ok(()),
    }
}

fn json_of(document: &Document, blocks: bool, pretty: bool) -> Result<String, Failure> {
    let json = match blocks {
        true => docboss_output::blocks_json(document, pretty),
        false => docboss_output::to_json(document, pretty),
    };
    json.map_err(|e| Failure::new(format!("json: {e}")))
}

/// Writes every media part into `dir` under its own file name and returns
/// the paths written.
fn export_media(document: &Document, dir: &Path) -> Result<Vec<PathBuf>, Failure> {
    std::fs::create_dir_all(dir).map_err(|e| Failure::new(format!("{}: {e}", dir.display())))?;
    document
        .media
        .iter()
        .map(|media| {
            let path = dir.join(docboss_output::media_file_name(&media.name));
            std::fs::write(&path, &media.data)
                .map(|()| path.clone())
                .map_err(|e| Failure::new(format!("{}: {e}", path.display())))
        })
        .collect()
}

/// The image mode for Markdown and HTML: exported next to the output under
/// `dir`, embedded, or the format's default.
fn image_mode(
    document: &Document,
    dir: Option<&Path>,
    embed: bool,
    default: ImageMode,
) -> Result<ImageMode, Failure> {
    if embed {
        return Ok(ImageMode::Embed);
    }
    let Some(dir) = dir else {
        return Ok(default);
    };
    export_media(document, dir)?;
    let prefix = format!("{}/", dir.display().to_string().trim_end_matches('/'));
    Ok(ImageMode::Reference { prefix })
}

fn count_blocks(blocks: &[Block], paragraphs: &mut usize, tables: &mut usize) {
    for block in blocks {
        match block {
            Block::Paragraph(_) => *paragraphs += 1,
            Block::Table(table) => {
                *tables += 1;
                table
                    .rows
                    .iter()
                    .flat_map(|row| row.cells.iter())
                    .for_each(|cell| count_blocks(&cell.blocks, paragraphs, tables));
            }
        }
    }
}

fn cmd_info(source: &Source) -> Result<(), Failure> {
    let document = load(source)?;
    let layout = docboss_layout::layout_document(&document);
    let mut out = String::new();
    let mut line = |key: &str, value: &str| {
        out.push_str(&format!("{key:<17}{value}\n"));
    };
    let format = match document.format {
        SourceFormat::Docx => "DOCX (ECMA-376 WordprocessingML)",
        SourceFormat::Doc => "DOC (Word 97-2003 binary)",
    };
    line("format:", format);
    let metadata = &document.metadata;
    let fields = [
        ("title:", &metadata.title),
        ("subject:", &metadata.subject),
        ("author:", &metadata.creator),
        ("keywords:", &metadata.keywords),
        ("description:", &metadata.description),
        ("last author:", &metadata.last_modified_by),
        ("revision:", &metadata.revision),
        ("created:", &metadata.created),
        ("modified:", &metadata.modified),
        ("category:", &metadata.category),
        ("application:", &metadata.application),
        ("company:", &metadata.company),
    ];
    for (key, value) in fields {
        let Some(value) = value.as_deref().filter(|v| !v.trim().is_empty()) else {
            continue;
        };
        line(key, value);
    }
    line("pages:", &layout.pages.len().to_string());
    line("sections:", &document.sections.len().to_string());
    if let Some(first) = document.sections.first() {
        let size = first.properties.page_size;
        let points = |twips: i32| twips as f32 / 20.0;
        line(
            "page size:",
            &format!("{} x {} pt", points(size.width), points(size.height)),
        );
    }
    let (mut paragraphs, mut tables) = (0usize, 0usize);
    document
        .sections
        .iter()
        .for_each(|section| count_blocks(&section.blocks, &mut paragraphs, &mut tables));
    line("paragraphs:", &paragraphs.to_string());
    line("tables:", &tables.to_string());
    line("styles:", &document.styles.styles.len().to_string());
    line(
        "headers/footers:",
        &document.headers_footers.len().to_string(),
    );
    line("footnotes:", &document.footnotes.len().to_string());
    line("endnotes:", &document.endnotes.len().to_string());
    line("comments:", &document.comments.len().to_string());
    line("images:", &document.media.len().to_string());
    line("fonts:", &document.fonts.len().to_string());
    let dropped = document
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Dropped)
        .count();
    let approximated = document.diagnostics.len() - dropped;
    line(
        "diagnostics:",
        &format!("{dropped} dropped, {approximated} approximated"),
    );
    emit(None, &out)
}

fn cmd_q(source: &Source, program: &str, raw: bool, blocks: bool) -> Result<(), Failure> {
    let program = q::compile_program(program).map_err(Failure::program)?;
    let document = load(source)?;
    let json = json_of(&document, blocks, false)?;
    let value: serde_json::Value =
        serde_json::from_str(&json).map_err(|e| Failure::new(format!("json: {e}")))?;
    let mut out = String::new();
    for item in q::run_program(&program, value) {
        let value = item.map_err(Failure::new)?;
        let text = match (&value, raw) {
            (serde_json::Value::String(text), true) => text.clone(),
            _ => serde_json::to_string_pretty(&value)
                .map_err(|e| Failure::new(format!("json: {e}")))?,
        };
        out.push_str(&text);
        out.push('\n');
    }
    emit(None, &out)
}

fn cmd_parts(file: &Path) -> Result<(), Failure> {
    let bytes = read_file(file)?;
    let mut out = String::new();
    for part in container::parts(&bytes)? {
        let stored = part
            .stored
            .map_or(String::new(), |stored| format!("  ({stored} stored)"));
        out.push_str(&format!("{:>10}  {}{stored}\n", part.size, part.name));
    }
    emit(None, &out)
}

fn cmd_hex(
    file: &Path,
    part: Option<&str>,
    offset: u64,
    length: Option<u64>,
    width: usize,
) -> Result<(), Failure> {
    let bytes = read_file(file)?;
    let data = match part {
        Some(name) => container::part_bytes(&bytes, name)?,
        None => bytes,
    };
    let start = usize::try_from(offset)
        .unwrap_or(usize::MAX)
        .min(data.len());
    let end = length.map_or(data.len(), |length| {
        start
            .saturating_add(usize::try_from(length).unwrap_or(usize::MAX))
            .min(data.len())
    });
    let color = std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none();
    let options = hexdump::HexOpts { width, color };
    let mut out = Vec::new();
    hexdump::hexdump(&mut out, &data[start..end], start as u64, &options)
        .map_err(|e| Failure::new(e.to_string()))?;
    emit(None, &String::from_utf8_lossy(&out))
}

fn cmd_convert(source: &Source, out: &Path) -> Result<(), Failure> {
    let document = load(source)?;
    let extension = out
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    let text = match extension.as_str() {
        "docx" => {
            return docboss_write::save(&document, out)
                .map_err(|e| Failure::new(format!("{}: {e}", out.display())));
        }
        "md" | "markdown" => docboss_output::to_markdown(&document, &MarkdownOptions::default()),
        "html" | "htm" => {
            let options = HtmlOptions {
                standalone: true,
                ..HtmlOptions::default()
            };
            docboss_output::to_html(&document, &options)
        }
        "txt" => docboss_output::to_text(&document, &TextOptions::default()),
        "json" => json_of(&document, false, true)?,
        _ => {
            return Err(Failure::new(format!(
                "{}: unknown output extension; use .docx, .md, .html, .txt or .json",
                out.display()
            )));
        }
    };
    emit(Some(out), &text)
}

fn cmd_create(command: CreateCommand) -> Result<(), Failure> {
    let CreateCommand::Md {
        file,
        out,
        size,
        title,
    } = command;
    let markdown = std::fs::read_to_string(&file)
        .map_err(|e| Failure::new(format!("{}: {e}", file.display())))?;
    let base = file.parent().map(Path::to_path_buf).unwrap_or_default();
    let mut options = docboss_write::markdown::Options::local_images(base);
    options.page_size = size.size();
    options.title = title;
    let bytes = docboss_write::markdown::to_docx(&markdown, &options)
        .map_err(|e| Failure::new(e.to_string()))?;
    std::fs::write(&out, bytes).map_err(|e| Failure::new(format!("{}: {e}", out.display())))
}
