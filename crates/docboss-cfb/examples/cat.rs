//! Writes one stream of a compound file to stdout.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable
)]

use std::io::Write;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let (Some(path), Some(stream)) = (args.next(), args.next()) else {
        return Err("usage: cat <file> <stream path>".into());
    };
    let bytes = std::fs::read(path)?;
    let file = docboss_cfb::CompoundFile::parse(&bytes)?;
    std::io::stdout().write_all(&file.open_stream(&stream)?)?;
    Ok(())
}
