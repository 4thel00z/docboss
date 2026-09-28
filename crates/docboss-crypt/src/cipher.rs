//! AES in the two chaining modes [MS-OFFCRYPTO] uses: ECB for Standard
//! encryption and CBC for Agile encryption, decryption only, no padding.

use aes::cipher::{generic_array::GenericArray, BlockDecrypt, KeyInit};
use aes::{Aes128, Aes192, Aes256};

use crate::{Error, Result};

enum Key {
    Aes128(Aes128),
    Aes192(Aes192),
    Aes256(Aes256),
}

impl Key {
    fn new(key: &[u8]) -> Result<Key> {
        let invalid = |_| Error::Malformed("AES key length");
        match key.len() {
            16 => Ok(Key::Aes128(Aes128::new_from_slice(key).map_err(invalid)?)),
            24 => Ok(Key::Aes192(Aes192::new_from_slice(key).map_err(invalid)?)),
            32 => Ok(Key::Aes256(Aes256::new_from_slice(key).map_err(invalid)?)),
            _ => Err(Error::Unsupported(format!(
                "AES key of {} bytes",
                key.len()
            ))),
        }
    }

    fn decrypt_block(&self, block: &mut [u8; BLOCK]) {
        let block = GenericArray::from_mut_slice(block.as_mut_slice());
        match self {
            Key::Aes128(cipher) => cipher.decrypt_block(block),
            Key::Aes192(cipher) => cipher.decrypt_block(block),
            Key::Aes256(cipher) => cipher.decrypt_block(block),
        }
    }
}

const BLOCK: usize = 16;

fn whole_blocks(data: &[u8]) -> &[u8] {
    &data[..data.len() - data.len() % BLOCK]
}

/// AES-ECB decryption of every whole block of `data`.
pub fn decrypt_ecb(key: &[u8], data: &[u8]) -> Result<Vec<u8>> {
    let key = Key::new(key)?;
    let mut out = whole_blocks(data).to_vec();
    out.as_chunks_mut::<BLOCK>()
        .0
        .iter_mut()
        .for_each(|block| key.decrypt_block(block));
    Ok(out)
}

/// AES-CBC decryption of every whole block of `data` under `iv`, which is
/// cut or zero-padded to one block.
pub fn decrypt_cbc(key: &[u8], iv: &[u8], data: &[u8]) -> Result<Vec<u8>> {
    let key = Key::new(key)?;
    let mut previous = [0u8; BLOCK];
    let take = iv.len().min(BLOCK);
    previous[..take].copy_from_slice(&iv[..take]);
    let mut out = whole_blocks(data).to_vec();
    for block in out.as_chunks_mut::<BLOCK>().0 {
        let cipher_text = *block;
        key.decrypt_block(block);
        block
            .iter_mut()
            .zip(previous)
            .for_each(|(byte, mask)| *byte ^= mask);
        previous = cipher_text;
    }
    Ok(out)
}
