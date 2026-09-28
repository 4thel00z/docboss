//! Reads one file repeatedly, for attaching a sampling profiler.
//!
//! `cargo run --release -p docboss-docx --example profile -- <file> [iterations]`

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("a file");
    let iterations: usize = args.next().and_then(|n| n.parse().ok()).unwrap_or(100);
    let data = std::fs::read(path).expect("readable file");
    let started = std::time::Instant::now();
    let mut blocks = 0;
    for _ in 0..iterations {
        let document = docboss_docx::read(&data).expect("a document");
        blocks += document.blocks().count();
    }
    let each = started.elapsed() / iterations as u32;
    println!("{blocks} blocks, {each:?} per read");
}
