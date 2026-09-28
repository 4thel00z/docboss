//! `docboss render`: lays the document out once and paints the selected
//! pages on every core, encoding each page on the worker that painted it.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use docboss_core::Document;
use docboss_layout::Layout;
use docboss_render::{Format, Renderer};

use crate::Failure;

/// The output path for 1-based `page`: `%d` in the template is replaced by
/// the page number.
fn page_path(template: &str, page: usize) -> PathBuf {
    PathBuf::from(template.replace("%d", &page.to_string()))
}

/// A template that names each page differently: one without `%d` gets
/// `-%d` before its extension when several pages are written.
fn template_for(out: &str, pages: usize) -> String {
    if pages < 2 || out.contains("%d") {
        return out.to_string();
    }
    let path = std::path::Path::new(out);
    let Some(extension) = path.extension().and_then(|e| e.to_str()) else {
        return format!("{out}-%d");
    };
    let stem = &out[..out.len() - extension.len() - 1];
    format!("{stem}-%d.{extension}")
}

fn format_for(template: &str, jpeg_quality: Option<u8>) -> Result<Format, Failure> {
    let extension = std::path::Path::new(template)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("png");
    let format = Format::from_extension(extension).ok_or_else(|| {
        Failure::new(format!(
            "{template}: unknown image extension; use .png, .ppm, .bmp or .jpg"
        ))
    })?;
    let Some(quality) = jpeg_quality else {
        return Ok(format);
    };
    match format {
        Format::Jpeg { .. } => Ok(Format::Jpeg { quality }),
        _ => Err(Failure::new("--jpeg-quality needs a .jpg or .jpeg output")),
    }
}

fn render_one(
    renderer: &mut Renderer,
    layout: &Layout,
    index: usize,
    scale: f32,
    template: &str,
    format: Format,
) -> Result<PathBuf, String> {
    let pixmap = renderer
        .render(layout, index, scale)
        .map_err(|e| format!("page {}: {e}", index + 1))?;
    let bytes = pixmap
        .encode(format)
        .map_err(|e| format!("page {}: {e}", index + 1))?;
    let path = page_path(template, index + 1);
    std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path)
}

pub fn cmd_render(
    document: &Document,
    page: Option<usize>,
    scale: f32,
    out: &str,
    jpeg_quality: Option<u8>,
) -> Result<(), Failure> {
    if !(scale.is_finite() && scale > 0.0) {
        return Err(Failure::new("--scale must be a positive number"));
    }
    let layout = docboss_layout::layout_document(document);
    let count = layout.pages.len();
    let indices: Vec<usize> = match page {
        Some(0) => return Err(Failure::new("pages are numbered from 1")),
        Some(number) if number > count => {
            return Err(Failure::new(format!(
                "page {number} does not exist; the document has {count}"
            )));
        }
        Some(number) => vec![number - 1],
        None => (0..count).collect(),
    };
    let template = template_for(out, indices.len());
    let format = format_for(&template, jpeg_quality)?;
    let workers = std::thread::available_parallelism()
        .map_or(1, |n| n.get())
        .min(indices.len().max(1));
    let next = AtomicUsize::new(0);
    let results: Mutex<Vec<Option<Result<PathBuf, String>>>> =
        Mutex::new(vec![None; indices.len()]);
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                let mut renderer = Renderer::new();
                loop {
                    let slot = next.fetch_add(1, Ordering::Relaxed);
                    let Some(&index) = indices.get(slot) else {
                        return;
                    };
                    let result =
                        render_one(&mut renderer, &layout, index, scale, &template, format);
                    let mut results = results.lock().unwrap_or_else(|e| e.into_inner());
                    results[slot] = Some(result);
                }
            });
        }
    });
    let results = results.into_inner().unwrap_or_else(|e| e.into_inner());
    let mut failures = Vec::new();
    for result in results.into_iter().flatten() {
        match result {
            Ok(path) => println!("{}", path.display()),
            Err(message) => failures.push(message),
        }
    }
    if failures.is_empty() {
        return Ok(());
    }
    Err(Failure::new(failures.join("; ")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templates_number_pages() {
        assert_eq!(template_for("out.png", 1), "out.png");
        assert_eq!(template_for("out.png", 3), "out-%d.png");
        assert_eq!(template_for("p%d.jpg", 3), "p%d.jpg");
        assert_eq!(template_for("dir/page", 2), "dir/page-%d");
        assert_eq!(page_path("p-%d.png", 12), PathBuf::from("p-12.png"));
    }
}
