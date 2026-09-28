//! Python bindings for docboss, compiled as the extension module
//! `docboss._docboss` and re-exported by the `docboss` package shim.
//!
//! Every class is frozen and usable from any Python thread. A `Document`
//! holds its parsed model behind an `Arc`; extraction, layout and rendering
//! release the GIL, so calls on different threads run in parallel.

use std::borrow::Cow;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use pyo3::buffer::PyBuffer;
use pyo3::create_exception;
use pyo3::exceptions::PyException;
use pyo3::prelude::*;
use pyo3::types::PyBytes;

mod document;
mod types;

pub use document::Document;
pub use types::{Block, Diagnostic, FontInfo, Image, Metadata, StyleInfo};

create_exception!(
    docboss,
    DocbossError,
    PyException,
    "Raised for any document processing error (bad data, encryption, unsupported format, I/O)."
);

pub(crate) fn doc_err(error: impl std::fmt::Display) -> PyErr {
    DocbossError::new_err(error.to_string())
}

/// The bytes of a buffer-like argument, copied once: `bytes` and
/// `bytearray` through `Cow<[u8]>`, `memoryview` and other buffer exporters
/// through `PyBuffer`, and a plain sequence of ints element by element.
pub(crate) fn byte_arg(data: &Bound<'_, PyAny>) -> PyResult<Vec<u8>> {
    if let Ok(buffer) = data.extract::<Cow<'_, [u8]>>() {
        return Ok(buffer.into_owned());
    }
    if let Ok(buffer) = PyBuffer::<u8>::get(data) {
        return buffer.to_vec(data.py());
    }
    data.extract::<Vec<u8>>()
}

/// The format of a file from its bytes: `docx`, `doc`, `encrypted_docx`,
/// `rtf` or `unknown`.
#[pyfunction]
fn detect(data: &Bound<'_, PyAny>) -> PyResult<&'static str> {
    let bytes = byte_arg(data)?;
    Ok(match docboss_core::detect(&bytes) {
        docboss_core::Format::Docx => "docx",
        docboss_core::Format::Doc => "doc",
        docboss_core::Format::EncryptedDocx => "encrypted_docx",
        docboss_core::Format::Rtf => "rtf",
        docboss_core::Format::Unknown => "unknown",
    })
}

/// Applies `f` to every item on `workers` threads, results in input order.
pub(crate) fn parallel_map<T: Sync, R: Send>(
    items: &[T],
    workers: usize,
    f: impl Fn(&T) -> R + Sync,
) -> Vec<R> {
    let next = AtomicUsize::new(0);
    let slots: Mutex<Vec<Option<R>>> = Mutex::new((0..items.len()).map(|_| None).collect());
    std::thread::scope(|scope| {
        for _ in 0..workers.clamp(1, items.len().max(1)) {
            scope.spawn(|| loop {
                let index = next.fetch_add(1, Ordering::Relaxed);
                let Some(item) = items.get(index) else {
                    return;
                };
                let result = f(item);
                slots.lock().unwrap_or_else(|e| e.into_inner())[index] = Some(result);
            });
        }
    });
    let slots = slots.into_inner().unwrap_or_else(|e| e.into_inner());
    slots.into_iter().flatten().collect()
}

fn text_of(path: &PathBuf) -> Result<String, String> {
    let document = docboss_core::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(docboss_output::to_text(
        &document,
        &docboss_output::TextOptions::default(),
    ))
}

/// Extracts the text of many files on a pool of Rust threads, in input
/// order. With `strict=False` a file that fails to open yields `None`
/// instead of raising.
#[pyfunction]
#[pyo3(signature = (paths, threads=None, strict=true))]
fn extract_texts(
    py: Python<'_>,
    paths: Vec<PathBuf>,
    threads: Option<usize>,
    strict: bool,
) -> PyResult<Vec<Option<String>>> {
    let count = paths.len();
    let cores = std::thread::available_parallelism().map_or(1, |n| n.get());
    let workers = threads.unwrap_or(cores).clamp(1, count.max(1));
    let results = py.allow_threads(|| parallel_map(&paths, workers, text_of));
    results
        .into_iter()
        .map(|slot| match slot {
            Ok(text) => Ok(Some(text)),
            Err(why) if strict => Err(DocbossError::new_err(why)),
            Err(_) => Ok(None),
        })
        .collect()
}

/// Composes CommonMark + GFM into DOCX bytes. Relative image paths are read
/// under `images_dir`; without it images become their alt text.
#[pyfunction]
#[pyo3(signature = (markdown, images_dir=None, title=None))]
fn md_to_docx<'py>(
    py: Python<'py>,
    markdown: &str,
    images_dir: Option<PathBuf>,
    title: Option<String>,
) -> PyResult<Bound<'py, PyBytes>> {
    let bytes = py.allow_threads(|| {
        let mut options = match images_dir {
            Some(dir) => docboss_write::markdown::Options::local_images(dir),
            None => docboss_write::markdown::Options::default(),
        };
        options.title = title;
        docboss_write::markdown::to_docx(markdown, &options)
    });
    Ok(PyBytes::new(py, &bytes.map_err(doc_err)?))
}

#[pymodule]
fn _docboss(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("DocbossError", m.py().get_type::<DocbossError>())?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    m.add_class::<Document>()?;
    m.add_class::<Metadata>()?;
    m.add_class::<Diagnostic>()?;
    m.add_class::<FontInfo>()?;
    m.add_class::<StyleInfo>()?;
    m.add_class::<Block>()?;
    m.add_class::<Image>()?;
    m.add_function(wrap_pyfunction!(detect, m)?)?;
    m.add_function(wrap_pyfunction!(extract_texts, m)?)?;
    m.add_function(wrap_pyfunction!(md_to_docx, m)?)?;
    Ok(())
}
