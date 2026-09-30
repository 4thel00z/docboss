//! Decrypts password-protected Office Open XML packages.
//!
//! A DOCX protected with a password to open is not a ZIP archive but a
//! compound file holding an `EncryptionInfo` stream and an
//! `EncryptedPackage` stream ([MS-OFFCRYPTO] §2.3.4.4). [`decrypt`] derives
//! the key from the password, checks it against the stored verifier, and
//! returns the plain ZIP package. Agile encryption (AES with SHA-1 or
//! SHA-2 key derivation, [MS-OFFCRYPTO] §2.3.4.10) is checked against its
//! data integrity HMAC; Standard encryption (AES-ECB with SHA-1,
//! [MS-OFFCRYPTO] §2.3.4.5) has no integrity data.
//!
//! [`xor`] holds the XOR obfuscation of Word binary documents
//! ([MS-OFFCRYPTO] §2.3.7), which `docboss-doc` opens with the same password.

mod agile;
mod base64;
mod cipher;
mod hash;
mod standard;
pub mod xor;

use docboss_cfb::CompoundFile;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum Error {
    #[error("compound file: {0}")]
    Cfb(String),
    #[error("not an encrypted package: the {0} stream is missing")]
    MissingStream(&'static str),
    #[error("the password does not open this package")]
    WrongPassword,
    #[error("the package fails its data integrity check")]
    Integrity,
    #[error("unsupported encryption: {0}")]
    Unsupported(String),
    #[error("malformed encryption data: {0}")]
    Malformed(&'static str),
}

/// The encryption scheme an `EncryptionInfo` stream declares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scheme {
    /// Version 4.4 ([MS-OFFCRYPTO] §2.3.4.10).
    Agile,
    /// Versions 2.2, 3.2 and 4.2 with the AES flag ([MS-OFFCRYPTO] §2.3.4.5).
    Standard,
    /// Versions 3.4 and 4.4 with the extensible flag, or RC4 CryptoAPI.
    Other(u16, u16),
}

/// The scheme of an `EncryptionInfo` stream from its version fields
/// ([MS-OFFCRYPTO] §2.3.4.1).
pub fn scheme(info: &[u8]) -> Result<Scheme> {
    let major = u16_at(info, 0).ok_or(Error::Malformed("EncryptionInfo version"))?;
    let minor = u16_at(info, 2).ok_or(Error::Malformed("EncryptionInfo version"))?;
    Ok(match (major, minor) {
        (4, 4) => Scheme::Agile,
        (2..=4, 2) => Scheme::Standard,
        other => Scheme::Other(other.0, other.1),
    })
}

/// Whether `bytes` is a compound file holding an encrypted package.
pub fn is_encrypted_package(bytes: &[u8]) -> bool {
    CompoundFile::parse(bytes).is_ok_and(|file| {
        file.find("EncryptionInfo").is_some() && file.find("EncryptedPackage").is_some()
    })
}

/// Decrypts an encrypted package compound file into the ZIP package bytes.
pub fn decrypt(bytes: &[u8], password: &str) -> Result<Vec<u8>> {
    let file = CompoundFile::parse(bytes).map_err(|e| Error::Cfb(e.to_string()))?;
    let info = file
        .open_stream("EncryptionInfo")
        .map_err(|_| Error::MissingStream("EncryptionInfo"))?;
    let package = file
        .open_stream("EncryptedPackage")
        .map_err(|_| Error::MissingStream("EncryptedPackage"))?;
    decrypt_streams(&info, &package, password)
}

/// Decrypts from the two streams directly.
pub fn decrypt_streams(info: &[u8], package: &[u8], password: &str) -> Result<Vec<u8>> {
    match scheme(info)? {
        Scheme::Agile => agile::decrypt(info, package, password),
        Scheme::Standard => standard::decrypt(info, package, password),
        Scheme::Other(major, minor) => Err(Error::Unsupported(format!(
            "EncryptionInfo version {major}.{minor}"
        ))),
    }
}

/// The package size stored in the first eight bytes of `EncryptedPackage`
/// ([MS-OFFCRYPTO] §2.3.4.4) and the encrypted data after it.
fn split_package(package: &[u8]) -> Result<(usize, &[u8])> {
    let size = package
        .get(..8)
        .map(|b| u64::from_le_bytes(b.try_into().unwrap_or([0; 8])))
        .ok_or(Error::Malformed("EncryptedPackage size"))?;
    let data = &package[8..];
    let size = usize::try_from(size).map_err(|_| Error::Malformed("EncryptedPackage size"))?;
    if size > data.len() {
        return Err(Error::Malformed(
            "EncryptedPackage is shorter than its size",
        ));
    }
    Ok((size, data))
}

fn u16_at(data: &[u8], at: usize) -> Option<u16> {
    let bytes = data.get(at..at.checked_add(2)?)?;
    Some(u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn u32_at(data: &[u8], at: usize) -> Option<u32> {
    let bytes = data.get(at..at.checked_add(4)?)?;
    Some(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn utf16le(password: &str) -> Vec<u8> {
    password.encode_utf16().flat_map(u16::to_le_bytes).collect()
}
