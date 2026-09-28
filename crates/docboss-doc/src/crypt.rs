//! Password-to-open decryption ([MS-DOC] §2.2.6): Office binary RC4
//! (§2.2.6.2) and RC4 CryptoAPI (§2.2.6.3), with the MD5, SHA-1 and RC4
//! primitives they need.

use crate::bytes::u32_at;
use crate::{Error, Result};

pub struct Rc4 {
    state: [u8; 256],
    i: u8,
    j: u8,
}

impl Rc4 {
    pub fn new(key: &[u8]) -> Rc4 {
        let mut state = [0u8; 256];
        for (i, slot) in state.iter_mut().enumerate() {
            *slot = i as u8;
        }
        let mut j = 0u8;
        for i in 0..256 {
            j = j.wrapping_add(state[i]).wrapping_add(key[i % key.len()]);
            state.swap(i, usize::from(j));
        }
        Rc4 { state, i: 0, j: 0 }
    }

    pub fn apply(&mut self, data: &mut [u8]) {
        for byte in data {
            self.i = self.i.wrapping_add(1);
            self.j = self.j.wrapping_add(self.state[usize::from(self.i)]);
            self.state.swap(usize::from(self.i), usize::from(self.j));
            let k = self.state[usize::from(
                self.state[usize::from(self.i)].wrapping_add(self.state[usize::from(self.j)]),
            )];
            *byte ^= k;
        }
    }
}

pub fn md5(data: &[u8]) -> [u8; 16] {
    const S: [u32; 64] = [
        7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5,
        9, 14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10,
        15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
    ];
    let k: Vec<u32> = (0..64)
        .map(|i| ((i as f64 + 1.0).sin().abs() * 4_294_967_296.0) as u32)
        .collect();
    let mut state: [u32; 4] = [0x6745_2301, 0xEFCD_AB89, 0x98BA_DCFE, 0x1032_5476];
    let mut message = data.to_vec();
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&((data.len() as u64).wrapping_mul(8)).to_le_bytes());
    for block in message.as_chunks::<64>().0 {
        let words: Vec<u32> = block
            .as_chunks::<4>()
            .0
            .iter()
            .map(|w| u32::from_le_bytes(*w))
            .collect();
        let [mut a, mut b, mut c, mut d] = state;
        for i in 0..64 {
            let (f, g) = match i / 16 {
                0 => ((b & c) | (!b & d), i),
                1 => ((d & b) | (!d & c), (5 * i + 1) % 16),
                2 => (b ^ c ^ d, (3 * i + 5) % 16),
                _ => (c ^ (b | !d), (7 * i) % 16),
            };
            let rotated = a
                .wrapping_add(f)
                .wrapping_add(k[i])
                .wrapping_add(words[g])
                .rotate_left(S[i]);
            a = d;
            d = c;
            c = b;
            b = b.wrapping_add(rotated);
        }
        state = [
            state[0].wrapping_add(a),
            state[1].wrapping_add(b),
            state[2].wrapping_add(c),
            state[3].wrapping_add(d),
        ];
    }
    let mut out = [0u8; 16];
    for (chunk, word) in out.as_chunks_mut::<4>().0.iter_mut().zip(state) {
        *chunk = word.to_le_bytes();
    }
    out
}

pub fn sha1(data: &[u8]) -> [u8; 20] {
    let mut state: [u32; 5] = [
        0x6745_2301,
        0xEFCD_AB89,
        0x98BA_DCFE,
        0x1032_5476,
        0xC3D2_E1F0,
    ];
    let mut message = data.to_vec();
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&((data.len() as u64).wrapping_mul(8)).to_be_bytes());
    for block in message.as_chunks::<64>().0 {
        let mut w = [0u32; 80];
        for (slot, word) in w.iter_mut().zip(block.as_chunks::<4>().0) {
            *slot = u32::from_be_bytes(*word);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }
        let [mut a, mut b, mut c, mut d, mut e] = state;
        for (i, &word) in w.iter().enumerate() {
            let (f, k) = match i / 20 {
                0 => ((b & c) | (!b & d), 0x5A82_7999),
                1 => (b ^ c ^ d, 0x6ED9_EBA1),
                2 => ((b & c) | (b & d) | (c & d), 0x8F1B_BCDC),
                _ => (b ^ c ^ d, 0xCA62_C1D6),
            };
            let temp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(word);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }
        for (slot, value) in state.iter_mut().zip([a, b, c, d, e]) {
            *slot = slot.wrapping_add(value);
        }
    }
    let mut out = [0u8; 20];
    for (chunk, word) in out.as_chunks_mut::<4>().0.iter_mut().zip(state) {
        *chunk = word.to_be_bytes();
    }
    out
}

fn utf16(password: &str) -> Vec<u8> {
    password
        .encode_utf16()
        .flat_map(|u| u.to_le_bytes())
        .collect()
}

/// A per-block key generator for one encrypted document.
pub enum Cipher {
    /// [MS-OFFCRYPTO] §2.3.6 Office binary RC4: MD5-derived 128-bit keys.
    Rc4 { truncated: [u8; 5] },
    /// [MS-OFFCRYPTO] §2.3.5 RC4 CryptoAPI: SHA-1-derived keys.
    CryptoApi { base: [u8; 20], key_bytes: usize },
}

impl Cipher {
    fn block_key(&self, block: u32) -> Vec<u8> {
        match self {
            Cipher::Rc4 { truncated } => {
                let mut input = truncated.to_vec();
                input.extend_from_slice(&block.to_le_bytes());
                md5(&input).to_vec()
            }
            Cipher::CryptoApi { base, key_bytes } => {
                let mut input = base.to_vec();
                input.extend_from_slice(&block.to_le_bytes());
                let mut key = sha1(&input)[..*key_bytes].to_vec();
                if *key_bytes == 5 {
                    key.resize(16, 0);
                }
                key
            }
        }
    }

    /// Decrypts a whole stream in 512-byte blocks, re-keyed per block,
    /// then restores the first `clear` bytes, which are stored unencrypted.
    pub fn decrypt_stream(&self, stream: &[u8], clear: usize) -> Vec<u8> {
        let mut out = stream.to_vec();
        for (block, chunk) in out.chunks_mut(512).enumerate() {
            Rc4::new(&self.block_key(block as u32)).apply(chunk);
        }
        let clear = clear.min(stream.len());
        out[..clear].copy_from_slice(&stream[..clear]);
        out
    }
}

/// Reads the encryption header at the start of the table stream and
/// derives the cipher for `password`, checking it against the verifier.
/// Returns the cipher and the header length left in the clear.
pub fn open(table: &[u8], password: &str) -> Result<(Cipher, usize)> {
    let major = crate::bytes::u16_at(table, 0)
        .ok_or(Error::UnsupportedEncryption("no encryption header"))?;
    let minor = crate::bytes::u16_at(table, 2).unwrap_or(0);
    match (major, minor) {
        (1, 1) => rc4(table, password),
        (2..=4, 2) => crypto_api(table, password),
        _ => Err(Error::UnsupportedEncryption(
            "unknown encryption header version",
        )),
    }
}

fn rc4(table: &[u8], password: &str) -> Result<(Cipher, usize)> {
    let salt = table
        .get(4..20)
        .ok_or(Error::UnsupportedEncryption("truncated RC4 header"))?;
    let mut verifier = table
        .get(20..52)
        .ok_or(Error::UnsupportedEncryption("truncated RC4 header"))?
        .to_vec();
    let h0 = md5(&utf16(password));
    let mut intermediate = Vec::with_capacity(336);
    for _ in 0..16 {
        intermediate.extend_from_slice(&h0[..5]);
        intermediate.extend_from_slice(salt);
    }
    let h1 = md5(&intermediate);
    let mut truncated = [0u8; 5];
    truncated.copy_from_slice(&h1[..5]);
    let cipher = Cipher::Rc4 { truncated };
    Rc4::new(&cipher.block_key(0)).apply(&mut verifier);
    if md5(&verifier[..16])[..] != verifier[16..32] {
        return Err(Error::WrongPassword);
    }
    Ok((cipher, 52))
}

fn crypto_api(table: &[u8], password: &str) -> Result<(Cipher, usize)> {
    let header_size = u32_at(table, 8)
        .ok_or(Error::UnsupportedEncryption("truncated CryptoAPI header"))?
        as usize;
    let header_at = 12;
    let key_bits = u32_at(table, header_at + 16).unwrap_or(40);
    let key_bits = if key_bits == 0 { 40 } else { key_bits };
    let verifier_at = header_at + header_size;
    let salt_size = u32_at(table, verifier_at).unwrap_or(16) as usize;
    let salt = table
        .get(verifier_at + 4..verifier_at + 4 + salt_size)
        .ok_or(Error::UnsupportedEncryption("truncated CryptoAPI verifier"))?;
    let encrypted_at = verifier_at + 4 + salt_size;
    let hash_size = u32_at(table, encrypted_at + 16).unwrap_or(20) as usize;
    let mut blob = table
        .get(encrypted_at..encrypted_at + 16)
        .map(|v| v.to_vec())
        .unwrap_or_default();
    blob.extend_from_slice(
        table
            .get(encrypted_at + 20..encrypted_at + 20 + hash_size)
            .unwrap_or(&[]),
    );
    if blob.len() != 16 + hash_size {
        return Err(Error::UnsupportedEncryption("truncated CryptoAPI verifier"));
    }
    let mut input = salt.to_vec();
    input.extend_from_slice(&utf16(password));
    let cipher = Cipher::CryptoApi {
        base: sha1(&input),
        key_bytes: (key_bits / 8).clamp(5, 16) as usize,
    };
    Rc4::new(&cipher.block_key(0)).apply(&mut blob);
    if sha1(&blob[..16])[..] != blob[16..16 + 20.min(hash_size)] {
        return Err(Error::WrongPassword);
    }
    Ok((cipher, encrypted_at + 20 + hash_size))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn digests_match_known_vectors() {
        assert_eq!(hex(&md5(b"")), "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(
            hex(&md5(b"The quick brown fox jumps over the lazy dog")),
            "9e107d9d372bb6826bd81d3542a419d6"
        );
        assert_eq!(
            hex(&sha1(b"abc")),
            "a9993e364706816aba3e25717850c26c9cd0d89d"
        );
        assert_eq!(
            hex(&sha1(&[b'a'; 1000])),
            "291e9a6c66994949b57ba5e650361e98fc36b1ba"
        );
    }

    #[test]
    fn rc4_matches_known_vector() {
        let mut data = b"Plaintext".to_vec();
        Rc4::new(b"Key").apply(&mut data);
        assert_eq!(hex(&data), "bbf316e8d940af0ad3");
    }
}
