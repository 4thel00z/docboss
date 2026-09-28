//! Documents at `http(s)://` URLs, read with range requests through
//! docboss-aio. When stderr is a terminal, a two-line coverage minimap
//! shows which stretches of the file have been fetched, erased once the
//! read is done; a server that ignores `Range` gets a notice and a
//! download bar instead.

use std::io::{IsTerminal as _, Write as _};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use docboss_aio::{AsyncDocument, FetchObserver, HttpBackend, ReadOptions};
use docboss_model::Document;

const BAR_WIDTH: usize = 30;
const MAP_WIDTH: usize = 40;

/// The URL a command-line path names, when it is one.
pub fn url_of(path: &Path) -> Option<&str> {
    let text = path.to_str()?;
    let lower = text.get(..8)?.to_ascii_lowercase();
    (lower.starts_with("http://") || lower.starts_with("https://")).then_some(text)
}

fn human_size(bytes: u64) -> String {
    const UNITS: [(&str, u64); 3] = [("GiB", 1 << 30), ("MiB", 1 << 20), ("KiB", 1 << 10)];
    UNITS
        .iter()
        .find(|(_, scale)| bytes >= *scale)
        .map_or(format!("{bytes} B"), |(unit, scale)| {
            format!("{:.1} {unit}", bytes as f64 / *scale as f64)
        })
}

fn cell_of(offset: u64, total: u64, width: usize) -> usize {
    if total == 0 {
        return 0;
    }
    (offset.min(total - 1) * width as u64 / total) as usize
}

/// The coverage minimap: a caret over the cell being fetched and a map of
/// the fetched cells with the running byte count.
struct Minimap {
    total: u64,
    state: Mutex<MinimapState>,
}

struct MinimapState {
    cells: Vec<bool>,
    fetched: u64,
    started: bool,
    finished: bool,
}

impl Minimap {
    fn new(total: u64) -> Self {
        Self {
            total,
            state: Mutex::new(MinimapState {
                cells: vec![false; MAP_WIDTH],
                fetched: 0,
                started: false,
                finished: false,
            }),
        }
    }

    fn frame(&self, offset: u64, len: u64) -> Option<String> {
        let mut state = self.state.lock().ok()?;
        if state.finished || len == 0 {
            return None;
        }
        let first = cell_of(offset, self.total, MAP_WIDTH);
        let last = cell_of(offset + len - 1, self.total, MAP_WIDTH);
        state.cells[first..=last]
            .iter_mut()
            .for_each(|cell| *cell = true);
        state.fetched += len;
        let caret = format!("{}▼", " ".repeat(2 + first));
        let map: String = state
            .cells
            .iter()
            .map(|&on| if on { '█' } else { '░' })
            .collect();
        let map = format!(
            "  {map}   {} / {}",
            human_size(state.fetched),
            human_size(self.total)
        );
        if state.started {
            return Some(format!("\r\x1b[1A\x1b[K{caret}\n\x1b[K{map}"));
        }
        state.started = true;
        Some(format!("{caret}\n{map}"))
    }

    fn tick(&self, offset: u64, len: u64) {
        let Some(frame) = self.frame(offset, len) else {
            return;
        };
        eprint!("{frame}");
        let _ = std::io::stderr().flush();
    }

    fn finish(&self) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        let erase = state.started && !state.finished;
        state.finished = true;
        if erase {
            eprint!("\r\x1b[K\x1b[1A\x1b[K");
            let _ = std::io::stderr().flush();
        }
    }
}

/// The notice and bar for the one full download a range-ignoring server
/// forces.
#[derive(Default)]
struct DownloadBar {
    started: AtomicBool,
    last_percent: AtomicU64,
}

impl DownloadBar {
    fn tick(&self, collected: u64, total: u64) {
        if !self.started.swap(true, Ordering::SeqCst) {
            eprintln!(
                "docboss: server ignores Range requests, downloading the whole file ({})",
                human_size(total)
            );
            self.last_percent.store(u64::MAX, Ordering::SeqCst);
        }
        let percent = (collected.min(total) * 100)
            .checked_div(total)
            .unwrap_or(100);
        if self.last_percent.swap(percent, Ordering::SeqCst) == percent {
            return;
        }
        let filled = percent as usize * BAR_WIDTH / 100;
        let bar = format!("{}{}", "=".repeat(filled), " ".repeat(BAR_WIDTH - filled));
        eprint!(
            "\r[{bar}] {percent:>3}%  {}/{}",
            human_size(collected),
            human_size(total)
        );
        let _ = std::io::stderr().flush();
    }

    fn finish(&self) {
        if self.started.load(Ordering::SeqCst) {
            eprint!("\r\x1b[K");
            let _ = std::io::stderr().flush();
        }
    }
}

fn runtime() -> Result<tokio::runtime::Runtime, String> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())
}

/// Opens a URL with progress drawn on a terminal and runs `work` on it.
fn with_document<T>(
    url: &str,
    work: impl AsyncFnOnce(&AsyncDocument) -> docboss_aio::Result<T>,
) -> Result<T, String> {
    runtime()?.block_on(async {
        let terminal = std::io::stderr().is_terminal();
        let bar = Arc::new(DownloadBar::default());
        let mut backend = HttpBackend::new(url).await.map_err(|e| e.to_string())?;
        if terminal {
            let bar = Arc::clone(&bar);
            backend =
                backend.on_fallback_progress(move |collected, total| bar.tick(collected, total));
        }
        let total = docboss_aio::Backend::len(&backend)
            .await
            .map_err(|e| e.to_string())?;
        let minimap = terminal.then(|| Arc::new(Minimap::new(total)));
        let observer = minimap.clone().map(|map| {
            Arc::new(move |offset: u64, len: u64| map.tick(offset, len)) as FetchObserver
        });
        let opened = AsyncDocument::from_backend(Arc::new(backend), observer).await;
        let result = match opened {
            Ok(document) => work(&document).await,
            Err(error) => Err(error),
        };
        minimap.iter().for_each(|map| map.finish());
        bar.finish();
        result.map_err(|e| e.to_string())
    })
}

/// Reads a remote document; without `media` its images are left empty.
pub fn load(url: &str, password: Option<String>, media: bool) -> Result<Document, String> {
    let options = ReadOptions { media, password };
    with_document(url, async |document| document.read(&options).await)
}

/// One ZIP entry or compound file stream of a remote document.
pub fn part(url: &str, name: &str) -> Result<Vec<u8>, String> {
    let name = docboss_tui::container::display_name(name);
    let name = name.trim_start_matches("\\x05").to_string();
    with_document(url, async |document| {
        let exact = document.part(&name).await;
        if exact.is_ok() {
            return exact;
        }
        document.part(&format!("\u{5}{name}")).await
    })
}

/// The whole remote file.
pub fn download(url: &str) -> Result<Vec<u8>, String> {
    runtime()?.block_on(async {
        let backend = HttpBackend::new(url).await.map_err(|e| e.to_string())?;
        let total = docboss_aio::Backend::len(&backend)
            .await
            .map_err(|e| e.to_string())?;
        let mut data = vec![0u8; usize::try_from(total).map_err(|e| e.to_string())?];
        let mut filled = 0;
        while filled < data.len() {
            let count = docboss_aio::Backend::read_at(&backend, filled as u64, &mut data[filled..])
                .await
                .map_err(|e| e.to_string())?;
            if count == 0 {
                break;
            }
            filled += count;
        }
        data.truncate(filled);
        Ok(data)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_are_told_from_paths() {
        assert_eq!(
            url_of(Path::new("https://example.com/a.docx")),
            Some("https://example.com/a.docx")
        );
        assert_eq!(url_of(Path::new("HTTP://x/y.doc")), Some("HTTP://x/y.doc"));
        assert_eq!(url_of(Path::new("report.docx")), None);
        assert_eq!(url_of(Path::new("http.docx")), None);
    }

    #[test]
    fn minimap_marks_fetched_cells_and_erases_once() {
        let map = Minimap::new(4000);
        let first = map.frame(0, 100).unwrap();
        assert!(first.starts_with("  ▼\n  █░"));
        let second = map.frame(3900, 100).unwrap();
        assert!(second.contains("█░░") && second.ends_with("200 B / 3.9 KiB"));
        map.finish();
        assert!(map.frame(0, 10).is_none());
    }
}
