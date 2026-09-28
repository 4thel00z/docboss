//! Agile encryption ([MS-OFFCRYPTO] §2.3.4.10 to §2.3.4.15): an XML
//! descriptor naming the cipher and hash, a password key encryptor holding
//! the intermediate key, the package in 4096-byte AES-CBC segments, and an
//! HMAC over the encrypted package.

use docboss_xml::{Event, Ns, Reader};

use crate::cipher::decrypt_cbc;
use crate::hash::Hash;
use crate::{base64, split_package, utf16le, Error, Result};

const SEGMENT: usize = 4096;
const BLOCK_VERIFIER_INPUT: [u8; 8] = [0xfe, 0xa7, 0xd2, 0x76, 0x3b, 0x4b, 0x9e, 0x79];
const BLOCK_VERIFIER_VALUE: [u8; 8] = [0xd7, 0xaa, 0x0f, 0x6d, 0x30, 0x61, 0x34, 0x4e];
const BLOCK_KEY_VALUE: [u8; 8] = [0x14, 0x6e, 0x0b, 0xe7, 0xab, 0xac, 0xd0, 0xd6];
const BLOCK_HMAC_KEY: [u8; 8] = [0x5f, 0xb2, 0xad, 0x01, 0x0c, 0xb9, 0xe1, 0xf6];
const BLOCK_HMAC_VALUE: [u8; 8] = [0xa0, 0x67, 0x7f, 0x02, 0xb2, 0x2c, 0x84, 0x33];

/// The cipher parameters shared by `keyData` and `p:encryptedKey`
/// ([MS-OFFCRYPTO] §2.3.4.10).
#[derive(Debug, Default)]
struct Params {
    salt: Vec<u8>,
    block_size: usize,
    key_bits: usize,
    hash_size: usize,
    hash: Option<Hash>,
}

#[derive(Debug, Default)]
struct Descriptor {
    key_data: Params,
    hmac_key: Vec<u8>,
    hmac_value: Vec<u8>,
    password: Params,
    spin_count: u32,
    verifier_input: Vec<u8>,
    verifier_value: Vec<u8>,
    key_value: Vec<u8>,
}

fn params(element: &docboss_xml::Element<'_>) -> Result<Params> {
    let number = |name: &str| -> usize {
        element
            .attr_raw(Ns::NONE, name)
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(0)
    };
    let cipher = element
        .attr_raw(Ns::NONE, "cipherAlgorithm")
        .unwrap_or("AES");
    if !cipher.eq_ignore_ascii_case("AES") {
        return Err(Error::Unsupported(format!("cipher {cipher}")));
    }
    let chaining = element
        .attr_raw(Ns::NONE, "cipherChaining")
        .unwrap_or("ChainingModeCBC");
    if chaining != "ChainingModeCBC" {
        return Err(Error::Unsupported(format!("chaining {chaining}")));
    }
    Ok(Params {
        salt: binary(element, "saltValue")?,
        block_size: number("blockSize"),
        key_bits: number("keyBits"),
        hash_size: number("hashSize"),
        hash: element
            .attr_raw(Ns::NONE, "hashAlgorithm")
            .and_then(Hash::from_name),
    })
}

fn binary(element: &docboss_xml::Element<'_>, name: &str) -> Result<Vec<u8>> {
    let raw = element.attr_raw(Ns::NONE, name).unwrap_or("");
    base64::decode(raw).ok_or(Error::Malformed("base64 value in the Agile descriptor"))
}

/// Reads the XML descriptor after the eight-byte version and reserved
/// fields ([MS-OFFCRYPTO] §2.3.4.10).
fn descriptor(info: &[u8]) -> Result<Descriptor> {
    let xml = info
        .get(8..)
        .ok_or(Error::Malformed("Agile EncryptionInfo"))?;
    let text = docboss_xml::decode(xml);
    let mut reader = Reader::new(&text);
    let mut out = Descriptor::default();
    let mut seen_password = false;
    loop {
        let element = match reader.next_event() {
            Event::Eof => break,
            Event::Start(element) => element,
            _ => continue,
        };
        match element.local {
            "keyData" => out.key_data = params(&element)?,
            "dataIntegrity" => {
                out.hmac_key = binary(&element, "encryptedHmacKey")?;
                out.hmac_value = binary(&element, "encryptedHmacValue")?;
            }
            "encryptedKey" if !seen_password => {
                seen_password = true;
                out.password = params(&element)?;
                out.spin_count = element
                    .attr_raw(Ns::NONE, "spinCount")
                    .and_then(|v| v.trim().parse().ok())
                    .unwrap_or(100_000);
                out.verifier_input = binary(&element, "encryptedVerifierHashInput")?;
                out.verifier_value = binary(&element, "encryptedVerifierHashValue")?;
                out.key_value = binary(&element, "encryptedKeyValue")?;
            }
            _ => {}
        }
    }
    if !seen_password {
        return Err(Error::Unsupported("no password key encryptor".into()));
    }
    Ok(out)
}

/// `bytes` cut to `size`, or padded with 0x36 up to it ([MS-OFFCRYPTO]
/// §2.3.4.11, §2.3.4.12).
fn fit(mut bytes: Vec<u8>, size: usize, fill: u8) -> Vec<u8> {
    bytes.resize(size, fill);
    bytes
}

fn hash_of(params: &Params) -> Result<Hash> {
    params
        .hash
        .ok_or_else(|| Error::Unsupported("hash algorithm".into()))
}

/// The key that decrypts one password key encryptor value
/// ([MS-OFFCRYPTO] §2.3.4.11).
fn password_key(descriptor: &Descriptor, spun: &[u8], block: &[u8]) -> Result<Vec<u8>> {
    let params = &descriptor.password;
    let hash = hash_of(params)?;
    Ok(fit(hash.digest(&[spun, block]), params.key_bits / 8, 0x36))
}

/// The intermediate key of the package, after checking the password
/// against the verifier ([MS-OFFCRYPTO] §2.3.4.13).
fn intermediate_key(descriptor: &Descriptor, password: &str) -> Result<Vec<u8>> {
    let params = &descriptor.password;
    let hash = hash_of(params)?;
    let seed = hash.digest(&[&params.salt, &utf16le(password)]);
    let spun = hash.spin(seed, descriptor.spin_count);
    let iv = &params.salt;
    let input_key = password_key(descriptor, &spun, &BLOCK_VERIFIER_INPUT)?;
    let input = decrypt_cbc(&input_key, iv, &descriptor.verifier_input)?;
    let value_key = password_key(descriptor, &spun, &BLOCK_VERIFIER_VALUE)?;
    let value = decrypt_cbc(&value_key, iv, &descriptor.verifier_value)?;
    let salt_size = params.salt.len().min(input.len());
    let expected = hash.digest(&[&input[..salt_size]]);
    let size = params.hash_size.min(expected.len()).min(value.len());
    if size == 0 || expected[..size] != value[..size] {
        return Err(Error::WrongPassword);
    }
    let key_key = password_key(descriptor, &spun, &BLOCK_KEY_VALUE)?;
    let key = decrypt_cbc(&key_key, iv, &descriptor.key_value)?;
    let length = descriptor.key_data.key_bits / 8;
    if key.len() < length {
        return Err(Error::Malformed("encrypted key value is too short"));
    }
    Ok(key[..length].to_vec())
}

/// The IV of a data block: the hash of the key data salt and the block key,
/// fitted to the block size ([MS-OFFCRYPTO] §2.3.4.12).
fn block_iv(key_data: &Params, block: &[u8]) -> Result<Vec<u8>> {
    let hash = hash_of(key_data)?;
    Ok(fit(
        hash.digest(&[&key_data.salt, block]),
        key_data.block_size.max(16),
        0x36,
    ))
}

/// Checks the HMAC of the whole `EncryptedPackage` stream
/// ([MS-OFFCRYPTO] §2.3.4.14).
fn check_integrity(descriptor: &Descriptor, key: &[u8], package: &[u8]) -> Result<()> {
    let key_data = &descriptor.key_data;
    let hash = hash_of(key_data)?;
    if descriptor.hmac_key.is_empty() {
        return Ok(());
    }
    let hmac_key = decrypt_cbc(
        key,
        &block_iv(key_data, &BLOCK_HMAC_KEY)?,
        &descriptor.hmac_key,
    )?;
    let hmac_value = decrypt_cbc(
        key,
        &block_iv(key_data, &BLOCK_HMAC_VALUE)?,
        &descriptor.hmac_value,
    )?;
    let size = hash.size();
    if hmac_key.len() < size || hmac_value.len() < size {
        return Err(Error::Malformed("data integrity values are too short"));
    }
    if hash.hmac(&hmac_key[..size], package) != hmac_value[..size] {
        return Err(Error::Integrity);
    }
    Ok(())
}

pub fn decrypt(info: &[u8], package: &[u8], password: &str) -> Result<Vec<u8>> {
    let descriptor = descriptor(info)?;
    let key = intermediate_key(&descriptor, password)?;
    check_integrity(&descriptor, &key, package)?;
    let (size, data) = split_package(package)?;
    let mut plain = Vec::with_capacity(data.len());
    for (index, segment) in data.chunks(SEGMENT).enumerate() {
        let iv = block_iv(&descriptor.key_data, &(index as u32).to_le_bytes())?;
        plain.extend(decrypt_cbc(&key, &iv, segment)?);
    }
    plain.truncate(size);
    Ok(plain)
}
