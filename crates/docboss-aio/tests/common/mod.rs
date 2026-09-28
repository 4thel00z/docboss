//! A minimal HTTP/1.1 file server for the range tests: `HEAD` reports the
//! length, `GET` answers `Range: bytes=a-b` with 206, or ignores it and
//! sends the whole body with 200 when built with `ranges = false`.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

pub struct Server {
    pub url: String,
    pub served: Arc<AtomicU64>,
    pub gets: Arc<AtomicU64>,
}

fn range_of(request: &str, len: usize) -> Option<(usize, usize)> {
    let line = request
        .lines()
        .find(|l| l.to_ascii_lowercase().starts_with("range:"))?;
    let spec = line.split_once("bytes=")?.1.trim();
    let (a, b) = spec.split_once('-')?;
    let start: usize = a.parse().ok()?;
    let end: usize = b.parse::<usize>().ok().map_or(len - 1, |e| e.min(len - 1));
    Some((start, end))
}

async fn serve(
    mut stream: TcpStream,
    body: Arc<Vec<u8>>,
    ranges: bool,
    served: Arc<AtomicU64>,
    gets: Arc<AtomicU64>,
) {
    let mut pending = Vec::new();
    loop {
        let mut buffer = [0u8; 4096];
        let end = loop {
            if let Some(at) = pending.windows(4).position(|w| w == b"\r\n\r\n") {
                break at + 4;
            }
            match stream.read(&mut buffer).await {
                Ok(0) | Err(_) => return,
                Ok(n) => pending.extend_from_slice(&buffer[..n]),
            }
        };
        let request = String::from_utf8_lossy(&pending[..end]).into_owned();
        pending.drain(..end);
        let head = request.starts_with("HEAD");
        let (status, slice) = match range_of(&request, body.len()).filter(|_| ranges && !head) {
            Some((start, end)) => ("206 Partial Content", &body[start..=end]),
            None => ("200 OK", &body[..]),
        };
        let header = format!(
            "HTTP/1.1 {status}\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\n\r\n",
            slice.len()
        );
        if stream.write_all(header.as_bytes()).await.is_err() {
            return;
        }
        if head {
            continue;
        }
        gets.fetch_add(1, Ordering::SeqCst);
        served.fetch_add(slice.len() as u64, Ordering::SeqCst);
        if stream.write_all(slice).await.is_err() {
            return;
        }
    }
}

pub async fn start(body: Vec<u8>, ranges: bool) -> Server {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/doc", listener.local_addr().unwrap());
    let body = Arc::new(body);
    let served = Arc::new(AtomicU64::new(0));
    let gets = Arc::new(AtomicU64::new(0));
    let (s, g) = (served.clone(), gets.clone());
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            tokio::spawn(serve(stream, body.clone(), ranges, s.clone(), g.clone()));
        }
    });
    Server { url, served, gets }
}
