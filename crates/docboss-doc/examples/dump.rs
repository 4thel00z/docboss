//! Prints a Word binary document's text, one line per paragraph, and its
//! diagnostics on stderr.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("usage: dump <file.doc> [--debug]")?;
    let bytes = std::fs::read(&path)?;
    let document = docboss_doc::read(&bytes)?;
    if std::env::args().any(|a| a == "--debug") {
        println!("{document:#?}");
        return Ok(());
    }
    print!("{}", docboss_model::plain_text(&document));
    for diagnostic in &document.diagnostics {
        eprintln!(
            "{:?} {}: {}",
            diagnostic.severity, diagnostic.location, diagnostic.message
        );
    }
    Ok(())
}
