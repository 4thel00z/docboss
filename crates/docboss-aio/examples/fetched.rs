//! Reads a document for text over a URL or path and reports how many bytes
//! and requests it took: `cargo run -p docboss-aio --features http
//! --example fetched -- <url-or-path>...`.

use docboss_aio::{AsyncDocument, ReadOptions};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    for target in std::env::args().skip(1) {
        let document = match target.starts_with("http") {
            true => AsyncDocument::open_url(&target).await?,
            false => AsyncDocument::open(&target).await?,
        };
        let read = document.read(&ReadOptions::default()).await?;
        let words = docboss_model::plain_text(&read).split_whitespace().count();
        println!(
            "{target}: {:?}, {} of {} bytes in {} requests ({:.1}%), {words} words",
            document.format(),
            document.bytes_fetched(),
            document.len(),
            document.requests().len(),
            100.0 * document.bytes_fetched() as f64 / document.len().max(1) as f64,
        );
    }
    Ok(())
}
