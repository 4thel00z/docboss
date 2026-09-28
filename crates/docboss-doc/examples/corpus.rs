//! Reads every `.doc` file under a directory, catching panics, and prints
//! the open rate, errors, panics and throughput.

use std::path::{Path, PathBuf};
use std::time::Instant;

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, out);
            continue;
        }
        if path
            .extension()
            .is_some_and(|x| x.eq_ignore_ascii_case("doc"))
        {
            out.push(path);
        }
    }
}

fn main() {
    let dir = std::env::args().nth(1).expect("usage: corpus <dir>");
    let verbose = std::env::args().any(|a| a == "-v");
    let mut files = Vec::new();
    walk(Path::new(&dir), &mut files);
    files.sort();
    std::panic::set_hook(Box::new(|_| {}));
    let (mut ok, mut errors, mut panics, mut bytes, mut characters) = (0, 0, 0, 0usize, 0usize);
    let started = Instant::now();
    for path in &files {
        let Ok(data) = std::fs::read(path) else {
            continue;
        };
        bytes += data.len();
        let begin = Instant::now();
        let result = std::panic::catch_unwind(|| {
            docboss_doc::read(&data).map(|d| {
                (
                    docboss_model::plain_text(&d),
                    d.diagnostics
                        .iter()
                        .map(|x| format!("{:?} {}: {}", x.severity, x.location, x.message))
                        .collect::<Vec<_>>(),
                )
            })
        });
        let elapsed = begin.elapsed();
        match result {
            Ok(Ok((text, diagnostics))) => {
                ok += 1;
                characters += text.chars().count();
                if verbose {
                    println!(
                        "ok     {:>8.2}ms {:>7} chars {:>3} diag {}",
                        elapsed.as_secs_f64() * 1e3,
                        text.chars().count(),
                        diagnostics.len(),
                        path.display()
                    );
                    for diagnostic in &diagnostics {
                        println!("         {diagnostic}");
                    }
                }
            }
            Ok(Err(error)) => {
                errors += 1;
                println!("error  {}: {error}", path.display());
            }
            Err(_) => {
                panics += 1;
                println!("PANIC  {}", path.display());
            }
        }
    }
    let seconds = started.elapsed().as_secs_f64();
    println!(
        "{} files: {ok} read, {errors} errors, {panics} panics; {:.1} MB in {:.3}s ({:.0} files/s, {:.1} MB/s), {characters} characters",
        files.len(),
        bytes as f64 / 1e6,
        seconds,
        files.len() as f64 / seconds,
        bytes as f64 / 1e6 / seconds
    );
}
