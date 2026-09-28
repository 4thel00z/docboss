//! Lists the streams of a compound file with their sizes and first bytes.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("usage: ls <file>")?;
    let bytes = std::fs::read(path)?;
    let file = docboss_cfb::CompoundFile::parse(&bytes)?;
    for (path, size) in file.walk() {
        let head: Vec<String> = file
            .open_stream(&path)?
            .iter()
            .take(16)
            .map(|b| format!("{b:02x}"))
            .collect();
        println!("{size:>10} {path:?} {}", head.join(" "));
    }
    for diagnostic in file.diagnostics() {
        eprintln!("{diagnostic:?}");
    }
    Ok(())
}
