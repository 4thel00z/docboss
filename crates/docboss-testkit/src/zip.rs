use docboss_zip::{Method, WriteOptions};

/// How the writer lays out each entry.
pub type ZipOptions = WriteOptions;

/// A chaining front end over [`docboss_zip::ZipWriter`] for building test
/// archives in one expression.
pub struct ZipWriter {
    inner: docboss_zip::ZipWriter,
    options: ZipOptions,
}

impl Default for ZipWriter {
    fn default() -> Self {
        Self::new()
    }
}

impl ZipWriter {
    pub fn new() -> Self {
        Self::with_options(ZipOptions::default())
    }

    pub fn with_options(options: ZipOptions) -> Self {
        Self {
            inner: docboss_zip::ZipWriter::with_options(options),
            options,
        }
    }

    pub fn stored(&mut self, name: &str, data: &[u8]) -> &mut Self {
        self.add(name.as_bytes(), data, Method::Stored)
    }

    pub fn deflated(&mut self, name: &str, data: &[u8]) -> &mut Self {
        self.add(name.as_bytes(), data, Method::Deflated)
    }

    /// Adds a stored entry whose name is given as raw bytes, for testing
    /// name encodings.
    pub fn raw_name(&mut self, name: &[u8], data: &[u8]) -> &mut Self {
        self.add(name, data, Method::Stored)
    }

    fn add(&mut self, name: &[u8], data: &[u8], method: Method) -> &mut Self {
        self.inner
            .add_raw_name(name, data, method)
            .expect("test archive entry fits the format");
        self
    }

    /// The archive bytes: entries, central directory and end records.
    pub fn finish(&mut self) -> Vec<u8> {
        let writer = std::mem::replace(
            &mut self.inner,
            docboss_zip::ZipWriter::with_options(self.options),
        );
        writer.finish().expect("test archive fits the format")
    }
}
