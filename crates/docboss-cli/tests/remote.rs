//! Every read command accepts an http(s) URL; these run the binary against
//! a local server that honours `Range`.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::Command;

fn fixture(crate_name: &str, name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(crate_name)
        .join("tests/fixtures")
        .join(name)
}

fn respond(stream: &mut std::net::TcpStream, body: &[u8]) -> Option<()> {
    let mut reader = BufReader::new(stream.try_clone().ok()?);
    loop {
        let mut head = Vec::new();
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).ok()? == 0 {
                return None;
            }
            if line == "\r\n" {
                break;
            }
            head.push(line);
        }
        let range = head.iter().find_map(|l| {
            let lower = l.to_ascii_lowercase();
            let (_, spec) = lower.split_once("bytes=")?;
            let (a, b) = spec.trim().split_once('-')?;
            Some((
                a.parse::<usize>().ok()?,
                b.parse::<usize>().ok()?.min(body.len() - 1),
            ))
        });
        let is_head = head.first().is_some_and(|l| l.starts_with("HEAD"));
        let (status, slice) = match range {
            Some((a, b)) if !is_head => ("206 Partial Content", &body[a..=b]),
            _ => ("200 OK", body),
        };
        write!(
            stream,
            "HTTP/1.1 {status}\r\nContent-Length: {}\r\n\r\n",
            slice.len()
        )
        .ok()?;
        if !is_head {
            stream.write_all(slice).ok()?;
        }
    }
}

/// Serves `body` on a background thread and returns its URL.
fn serve(body: Vec<u8>, name: &str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/{name}", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            let body = body.clone();
            std::thread::spawn(move || respond(&mut stream, &body));
        }
    });
    url
}

fn docboss(args: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_docboss"))
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn text_markdown_and_parts_over_http_match_local_files() {
    for (crate_name, name) in [
        ("docboss-docx", "libreoffice-rich.docx"),
        ("docboss-doc", "image.doc"),
    ] {
        let path = fixture(crate_name, name);
        let url = serve(std::fs::read(&path).unwrap(), name);
        let local = path.to_str().unwrap();
        assert_eq!(
            docboss(&["text", &url]),
            docboss(&["text", local]),
            "{name}"
        );
        assert_eq!(docboss(&["md", &url]), docboss(&["md", local]), "{name}");
        assert_eq!(
            docboss(&["parts", &url]),
            docboss(&["parts", local]),
            "{name}"
        );
    }
}

#[test]
fn xml_reads_one_part_over_http() {
    let path = fixture("docboss-docx", "libreoffice-rich.docx");
    let url = serve(std::fs::read(&path).unwrap(), "rich.docx");
    let remote = docboss(&["xml", &url, "word/document.xml"]);
    assert_eq!(
        remote,
        docboss(&["xml", path.to_str().unwrap(), "word/document.xml"])
    );
}

#[test]
fn encrypted_docx_over_http_takes_the_password() {
    let path = fixture("docboss-crypt", "Encrypted_MSO2010_abc.docx");
    let url = serve(std::fs::read(&path).unwrap(), "locked.docx");
    let text = docboss(&["text", &url, "--password", "abc"]);
    assert_eq!(
        text,
        docboss(&["text", path.to_str().unwrap(), "--password", "abc"])
    );
    let failed = Command::new(env!("CARGO_BIN_EXE_docboss"))
        .args(["text", &url])
        .output()
        .unwrap();
    assert_eq!(failed.status.code(), Some(1));
}

#[test]
fn unreachable_url_fails_with_an_http_error() {
    let output = Command::new(env!("CARGO_BIN_EXE_docboss"))
        .args(["text", "http://127.0.0.1:1/missing.docx"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("http"));
}
