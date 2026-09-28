//! Reads every `.docx`, `.docm`, `.dotx` and `.dotm` file under the given
//! paths and reports how many open, how many fail, and any panic.
//!
//! `cargo run --release -p docboss-docx --example corpus -- <dir> [--text]`

use std::path::{Path, PathBuf};
use std::time::Instant;

fn collect(path: &Path, out: &mut Vec<PathBuf>) {
    if path.is_dir() {
        let Ok(entries) = std::fs::read_dir(path) else {
            return;
        };
        let mut entries: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
        entries.sort();
        entries.iter().for_each(|entry| collect(entry, out));
        return;
    }
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase);
    if matches!(
        extension.as_deref(),
        Some("docx" | "docm" | "dotx" | "dotm")
    ) {
        out.push(path.to_path_buf());
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let show_text = args.iter().any(|a| a == "--text");
    let mut files = Vec::new();
    args.iter()
        .filter(|a| !a.starts_with("--"))
        .for_each(|a| collect(Path::new(a), &mut files));
    let (mut opened, mut failed, mut panicked, mut bytes) = (0, 0, 0, 0usize);
    let started = Instant::now();
    for file in &files {
        let Ok(data) = std::fs::read(file) else {
            continue;
        };
        bytes += data.len();
        let outcome = std::panic::catch_unwind(|| docboss_docx::read(&data));
        match outcome {
            Ok(Ok(document)) => {
                opened += 1;
                if show_text {
                    println!(
                        "== {} ({} diagnostics)",
                        file.display(),
                        document.diagnostics.len()
                    );
                    for d in &document.diagnostics {
                        println!("   ! {}: {}", d.location, d.message);
                    }
                    println!("{}", docboss_model::plain_text(&document));
                }
            }
            Ok(Err(error)) => {
                failed += 1;
                println!("FAIL {}: {error}", file.display());
            }
            Err(_) => {
                panicked += 1;
                println!("PANIC {}", file.display());
            }
        }
    }
    let elapsed = started.elapsed();
    println!(
        "{} files, {opened} opened, {failed} failed, {panicked} panicked, {:.1} MB in {:.3}s",
        files.len(),
        bytes as f64 / 1e6,
        elapsed.as_secs_f64()
    );
}
