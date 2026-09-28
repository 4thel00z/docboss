//! Compares range reads with the synchronous reader over every .docx and
//! .doc file under the given directories: `cargo run --release -p
//! docboss-aio --example parity -- <dir>...`.

use std::path::{Path, PathBuf};

use docboss_aio::{AsyncDocument, ReadOptions};

fn files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for path in entries.flatten().map(|e| e.path()) {
        if path.is_dir() {
            files(&path, out);
            continue;
        }
        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase);
        if matches!(
            extension.as_deref(),
            Some("docx" | "doc" | "docm" | "dotx" | "dot")
        ) {
            out.push(path);
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let mut paths = Vec::new();
    std::env::args()
        .skip(1)
        .for_each(|dir| files(Path::new(&dir), &mut paths));
    paths.sort();
    let (mut same, mut both_fail, mut differ) = (0, 0, 0);
    let mut bytes_by_format = std::collections::BTreeMap::<String, (u64, u64, u32)>::new();
    for path in &paths {
        let bytes = std::fs::read(path).unwrap_or_default();
        let sync = docboss_core::read(&bytes);
        let full = async {
            let document = AsyncDocument::open(path).await?;
            let read = document
                .read(&ReadOptions {
                    media: true,
                    ..ReadOptions::default()
                })
                .await?;
            Ok::<_, docboss_aio::Error>(read)
        }
        .await;
        let text = async {
            let document = AsyncDocument::open(path).await?;
            let read = document.read(&ReadOptions::default()).await?;
            Ok::<_, docboss_aio::Error>((read, document.bytes_fetched(), document.len()))
        }
        .await;
        match (sync, full, text) {
            (Ok(sync), Ok(full), Ok((text, got, len))) => {
                let text_same =
                    docboss_model::plain_text(&text) == docboss_model::plain_text(&sync);
                if full.sections == sync.sections && full.media == sync.media && text_same {
                    same += 1;
                    let slot = bytes_by_format
                        .entry(format!("{:?}", sync.format))
                        .or_default();
                    *slot = (slot.0 + got, slot.1 + len, slot.2 + 1);
                    continue;
                }
                differ += 1;
                println!("differs: {}", path.display());
            }
            (Err(_), Err(_), Err(_)) => both_fail += 1,
            (sync, full, _) => {
                differ += 1;
                println!(
                    "outcome differs: {} sync ok {} async ok {}",
                    path.display(),
                    sync.is_ok(),
                    full.is_ok()
                );
            }
        }
    }
    println!(
        "{} files: {same} identical, {both_fail} refused by both, {differ} different",
        paths.len()
    );
    for (format, (got, len, count)) in bytes_by_format {
        println!(
            "{format}: {count} files, text reads fetched {got} of {len} bytes ({:.1}%)",
            100.0 * got as f64 / len.max(1) as f64
        );
    }
}
