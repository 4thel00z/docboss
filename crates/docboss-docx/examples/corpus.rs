//! Reads every `.docx`, `.docm`, `.dotx` and `.dotm` file under the given
//! paths and reports how many open, how many fail, and any panic.
//!
//! `cargo run --release -p docboss-docx --example corpus -- <dir> [--text] [--mutate N]`
//!
//! `--mutate N` also reads N copies of each file with bytes flipped at
//! pseudo-random offsets, counting only panics.

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

fn next(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}

/// Rewrites the package with bytes of its uncompressed XML parts changed,
/// cut or duplicated, so the XML and WordprocessingML layers see damage
/// the ZIP layer would otherwise catch.
fn mutate_parts(data: &[u8], state: &mut u64) -> Option<Vec<u8>> {
    let archive = docboss_zip::Archive::new(data).ok()?;
    let mut writer = docboss_testkit::ZipWriter::new();
    for entry in archive.entries() {
        let Ok(contents) = archive.read_entry(entry) else {
            continue;
        };
        let mut bytes = contents.data.into_owned();
        let is_xml = entry.name.ends_with(".xml") || entry.name.ends_with(".rels");
        if is_xml && !bytes.is_empty() && next(state) % 2 == 0 {
            let at = (next(state) % bytes.len() as u64) as usize;
            match next(state) % 4 {
                0 => bytes.truncate(at),
                1 => bytes[at] = b"<>/\"&;:x\0"[(next(state) % 9) as usize],
                2 => {
                    let copy = bytes[at..bytes.len().min(at + 64)].to_vec();
                    bytes.splice(at..at, copy);
                }
                _ => {
                    let end = bytes.len().min(at + 32);
                    bytes.drain(at..end);
                }
            }
        }
        writer.stored(&entry.name, &bytes);
    }
    Some(writer.finish())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let show_text = args.iter().any(|a| a == "--text");
    let mutations: usize = args
        .iter()
        .position(|a| a == "--mutate")
        .and_then(|i| args.get(i + 1))
        .and_then(|n| n.parse().ok())
        .unwrap_or(0);
    let mut mutated_panics = 0;
    let mut mutated_opened = 0;
    let mut state: u64 = 0x2545_F491_4F6C_DD1D;
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
        for _ in 0..mutations {
            let mut copy = data.clone();
            for _ in 0..8 {
                let value = next(&mut state);
                let at = (value % copy.len().max(1) as u64) as usize;
                if let Some(byte) = copy.get_mut(at) {
                    *byte ^= (value >> 32) as u8 | 1;
                }
            }
            if std::panic::catch_unwind(|| docboss_docx::read(&copy)).is_err() {
                mutated_panics += 1;
                println!("PANIC (mutated) {}", file.display());
            }
            let Some(repacked) = mutate_parts(&data, &mut state) else {
                continue;
            };
            match std::panic::catch_unwind(|| docboss_docx::read(&repacked)) {
                Ok(Ok(_)) => mutated_opened += 1,
                Ok(Err(_)) => {}
                Err(_) => {
                    mutated_panics += 1;
                    println!("PANIC (mutated xml) {}", file.display());
                }
            }
        }
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
    if mutations > 0 {
        println!(
            "{} mutated copies, {mutated_panics} panicked, {mutated_opened} repacked copies opened",
            files.len() * mutations * 2
        );
    }
}
