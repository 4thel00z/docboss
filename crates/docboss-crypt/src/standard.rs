//! Standard encryption ([MS-OFFCRYPTO] §2.3.4.5 to §2.3.4.9): AES-ECB with
//! a SHA-1 key derived through 50,000 hash iterations.

use crate::cipher::decrypt_ecb;
use crate::hash::Hash;
use crate::{split_package, u32_at, utf16le, Error, Result};

const SPIN: u32 = 50_000;
const CALG_AES_128: u32 = 0x660E;
const CALG_AES_192: u32 = 0x660F;
const CALG_AES_256: u32 = 0x6610;
const CALG_SHA1: u32 = 0x8004;
const FLAG_AES: u32 = 0x20;

struct Header {
    key_bytes: usize,
}

struct Verifier<'a> {
    salt: &'a [u8],
    encrypted_verifier: &'a [u8],
    hash_size: usize,
    encrypted_hash: &'a [u8],
}

/// The EncryptionHeader ([MS-OFFCRYPTO] §2.3.2) after the version and
/// flags, and the EncryptionVerifier after it (§2.3.3).
fn parse(info: &[u8]) -> Result<(Header, Verifier<'_>)> {
    let malformed = || Error::Malformed("Standard EncryptionInfo");
    let flags = u32_at(info, 4).ok_or_else(malformed)?;
    let header_size = u32_at(info, 8).ok_or_else(malformed)? as usize;
    let header = info
        .get(12..12usize.checked_add(header_size).ok_or_else(malformed)?)
        .ok_or_else(malformed)?;
    let alg_id = u32_at(header, 8).ok_or_else(malformed)?;
    let alg_hash = u32_at(header, 12).ok_or_else(malformed)?;
    let key_bits = u32_at(header, 16).ok_or_else(malformed)?;
    if flags & FLAG_AES == 0 {
        return Err(Error::Unsupported("Standard encryption with RC4".into()));
    }
    let expected_bits = match alg_id {
        CALG_AES_128 | 0 => 128,
        CALG_AES_192 => 192,
        CALG_AES_256 => 256,
        other => return Err(Error::Unsupported(format!("cipher algorithm {other:#x}"))),
    };
    if alg_hash != CALG_SHA1 && alg_hash != 0 {
        return Err(Error::Unsupported(format!("hash algorithm {alg_hash:#x}")));
    }
    let key_bits = if key_bits == 0 {
        expected_bits
    } else {
        key_bits
    };
    let verifier = &info[12 + header_size..];
    let salt_size = u32_at(verifier, 0).ok_or_else(malformed)? as usize;
    if salt_size != 16 {
        return Err(Error::Malformed("Standard verifier salt size"));
    }
    let salt = verifier.get(4..20).ok_or_else(malformed)?;
    let encrypted_verifier = verifier.get(20..36).ok_or_else(malformed)?;
    let hash_size = u32_at(verifier, 36).ok_or_else(malformed)? as usize;
    let encrypted_hash = verifier.get(40..72).ok_or_else(malformed)?;
    Ok((
        Header {
            key_bytes: key_bits as usize / 8,
        },
        Verifier {
            salt,
            encrypted_verifier,
            hash_size,
            encrypted_hash,
        },
    ))
}

/// The encryption key for block 0 ([MS-OFFCRYPTO] §2.3.4.7).
fn derive_key(password: &str, salt: &[u8], key_bytes: usize) -> Vec<u8> {
    let sha1 = Hash::Sha1;
    let seed = sha1.digest(&[salt, &utf16le(password)]);
    let spun = sha1.spin(seed, SPIN);
    let final_hash = sha1.digest(&[&spun, &0u32.to_le_bytes()]);
    let pad = |fill: u8| -> Vec<u8> {
        let mut buffer = vec![fill; 64];
        buffer
            .iter_mut()
            .zip(&final_hash)
            .for_each(|(byte, h)| *byte ^= h);
        sha1.digest(&[&buffer])
    };
    let mut derived = pad(0x36);
    derived.extend(pad(0x5C));
    derived.truncate(key_bytes);
    derived
}

/// Checks the password against the verifier ([MS-OFFCRYPTO] §2.3.4.9).
fn verify(key: &[u8], verifier: &Verifier<'_>) -> Result<bool> {
    let plain = decrypt_ecb(key, verifier.encrypted_verifier)?;
    let hash = decrypt_ecb(key, verifier.encrypted_hash)?;
    let size = verifier.hash_size.min(20);
    Ok(Hash::Sha1.digest(&[&plain])[..size] == hash[..size])
}

pub fn decrypt(info: &[u8], package: &[u8], password: &str) -> Result<Vec<u8>> {
    let (header, verifier) = parse(info)?;
    let key = derive_key(password, verifier.salt, header.key_bytes);
    if !verify(&key, &verifier)? {
        return Err(Error::WrongPassword);
    }
    let (size, data) = split_package(package)?;
    let mut plain = decrypt_ecb(&key, data)?;
    plain.truncate(size);
    Ok(plain)
}
