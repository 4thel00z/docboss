//! Lays out and renders a document built in code, one PNG per page:
//! `cargo run --release -p docboss-render --example render_sample -- out/ 2.0`.

#[path = "../benches/sample/mod.rs"]
mod sample;

use std::path::PathBuf;
use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let out = PathBuf::from(args.next().unwrap_or_else(|| "sample-pages".into()));
    let scale: f32 = args.next().map_or(Ok(1.5), |s| s.parse())?;
    let chunks: usize = args.next().map_or(Ok(3), |s| s.parse())?;
    std::fs::create_dir_all(&out)?;
    let document = sample::document(chunks);
    let fonts = docboss_layout::fonts_for(&document);
    let started = Instant::now();
    let layout = docboss_layout::layout(&document, &fonts);
    let laid = started.elapsed();
    let pixmaps = docboss_render::render_pages(&layout, scale);
    let rendered = started.elapsed() - laid;
    for (index, pixmap) in pixmaps.into_iter().enumerate() {
        let pixmap = pixmap?;
        pixmap
            .diagnostics
            .iter()
            .for_each(|d| eprintln!("page {}: {}", index + 1, d.message));
        pixmap.save(out.join(format!("page-{:03}.png", index + 1)))?;
    }
    println!(
        "{} pages: layout {laid:?}, render {rendered:?}",
        layout.pages.len()
    );
    Ok(())
}
