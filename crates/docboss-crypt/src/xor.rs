//! XOR obfuscation of Office binary documents ([MS-OFFCRYPTO] §2.3.7): the
//! 16-bit password verifier (§2.3.7.1), the XOR key (§2.3.7.2), the 32-bit
//! verifier of method 2 (§2.3.7.4), its 16-byte XOR array (§2.3.7.5) and
//! the byte transformation (§2.3.7.6) Word documents use.

use crate::{Error, Result};

const MAX_PASSWORD: usize = 15;

const PAD: [u8; 15] = [
    0xBB, 0xFF, 0xFF, 0xBA, 0xFF, 0xFF, 0xB9, 0x80, 0x00, 0xBE, 0x0F, 0x00, 0xBF, 0x0F, 0x00,
];

const INITIAL_CODE: [u16; 15] = [
    0xE1F0, 0x1D0F, 0xCC9C, 0x84C0, 0x110C, 0x0E10, 0xF1CE, 0x313E, 0x1872, 0xE139, 0xD40F, 0x84F9,
    0x280C, 0xA96A, 0x4EC3,
];

const XOR_MATRIX: [u16; 105] = [
    0xAEFC, 0x4DD9, 0x9BB2, 0x2745, 0x4E8A, 0x9D14, 0x2A09, 0x7B61, 0xF6C2, 0xFDA5, 0xEB6B, 0xC6F7,
    0x9DCF, 0x2BBF, 0x4563, 0x8AC6, 0x05AD, 0x0B5A, 0x16B4, 0x2D68, 0x5AD0, 0x0375, 0x06EA, 0x0DD4,
    0x1BA8, 0x3750, 0x6EA0, 0xDD40, 0xD849, 0xA0B3, 0x5147, 0xA28E, 0x553D, 0xAA7A, 0x44D5, 0x6F45,
    0xDE8A, 0xAD35, 0x4A4B, 0x9496, 0x390D, 0x721A, 0xEB23, 0xC667, 0x9CEF, 0x29FF, 0x53FE, 0xA7FC,
    0x5FD9, 0x47D3, 0x8FA6, 0x0F6D, 0x1EDA, 0x3DB4, 0x7B68, 0xF6D0, 0xB861, 0x60E3, 0xC1C6, 0x93AD,
    0x377B, 0x6EF6, 0xDDEC, 0x45A0, 0x8B40, 0x06A1, 0x0D42, 0x1A84, 0x3508, 0x6A10, 0xAA51, 0x4483,
    0x8906, 0x022D, 0x045A, 0x08B4, 0x1168, 0x76B4, 0xED68, 0xCAF1, 0x85C3, 0x1BA7, 0x374E, 0x6E9C,
    0x3730, 0x6E60, 0xDCC0, 0xA9A1, 0x4363, 0x86C6, 0x1DAD, 0x3331, 0x6662, 0xCCC4, 0x89A9, 0x0373,
    0x06E6, 0x0DCC, 0x1021, 0x2042, 0x4084, 0x8108, 0x1231, 0x2462, 0x48C4,
];

/// [MS-OFFCRYPTO] §2.3.7.1 CreatePasswordVerifier_Method1: the 16-bit
/// verifier of a single-byte password.
pub fn verifier_method1(password: &[u8]) -> u16 {
    let length = password.len().min(MAX_PASSWORD);
    let bytes = std::iter::once(length as u8).chain(password[..length].iter().copied());
    let verifier = bytes.rev().fold(0u16, |verifier, byte| {
        let carry = u16::from(verifier & 0x4000 != 0);
        (((verifier << 1) & 0x7FFF) | carry) ^ u16::from(byte)
    });
    verifier ^ 0xCE4B
}

/// [MS-OFFCRYPTO] §2.3.7.2 CreateXorKey_Method1: the 16-bit XOR key of a
/// single-byte password of 1 to 15 characters.
pub fn xor_key_method1(password: &[u8]) -> Option<u16> {
    let length = password.len().min(MAX_PASSWORD);
    let mut key = *INITIAL_CODE.get(length.checked_sub(1)?)?;
    let mut element = XOR_MATRIX.len();
    for &byte in password[..length].iter().rev() {
        let mut bits = byte;
        for _ in 0..7 {
            element -= 1;
            if bits & 0x40 != 0 {
                key ^= XOR_MATRIX[element];
            }
            bits <<= 1;
        }
    }
    Some(key)
}

/// [MS-OFFCRYPTO] §2.3.7.4 CreatePasswordVerifier_Method2: the XOR key in
/// the high word and the method 1 verifier in the low word.
pub fn verifier_method2(password: &[u8]) -> Option<u32> {
    let key = xor_key_method1(password)?;
    Some(u32::from(key) << 16 | u32::from(verifier_method1(password)))
}

/// [MS-OFFCRYPTO] §2.3.7.5 CreateXorArray_Method2: the password padded to
/// 16 bytes, each byte XORed with the low or high byte of the XOR key and
/// rotated right by one bit.
pub fn xor_array_method2(password: &[u8]) -> Option<[u8; 16]> {
    let key = xor_key_method1(password)?;
    let [low, high] = key.to_le_bytes();
    let length = password.len().min(MAX_PASSWORD);
    let mut array = [0u8; 16];
    for (i, slot) in array.iter_mut().enumerate() {
        let byte = match i < length {
            true => password[i],
            false => PAD[i - length],
        };
        let mixed = byte ^ if i % 2 == 0 { low } else { high };
        *slot = mixed.rotate_right(1);
    }
    Some(array)
}

/// The XOR array of a Word document protected with XOR obfuscation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct XorObfuscation {
    array: [u8; 16],
}

impl XorObfuscation {
    /// Checks `password` against the stored method 2 `verifier`. The
    /// password is made single-byte both ways [MS-OFFCRYPTO] §2.3.7.4 names:
    /// through the ANSI `code_page`, and by keeping each character's low
    /// byte, or its high byte when the low byte is zero.
    pub fn open(password: &str, verifier: u32, code_page: u32) -> Result<XorObfuscation> {
        let low_bytes: Vec<u8> = password
            .encode_utf16()
            .take(MAX_PASSWORD)
            .map(|unit| match unit.to_le_bytes() {
                [0, high] => high,
                [low, _] => low,
            })
            .collect();
        let ansi: Option<Vec<u8>> = password
            .chars()
            .take(MAX_PASSWORD)
            .map(|c| docboss_cfb::codepage::encode_char(code_page, c))
            .collect();
        [Some(low_bytes), ansi]
            .into_iter()
            .flatten()
            .find(|bytes| verifier_method2(bytes) == Some(verifier))
            .and_then(|bytes| xor_array_method2(&bytes))
            .map(|array| XorObfuscation { array })
            .ok_or(Error::WrongPassword)
    }

    /// [MS-OFFCRYPTO] §2.3.7.6 Binary Document XOR Data Transformation
    /// Method 2, which is its own inverse: each byte at stream offset `i`
    /// is XORed with array element `i % 16`, except a zero byte and a byte
    /// equal to its element, which stay as they are. The first `clear`
    /// bytes are left untouched.
    pub fn apply(&self, stream: &mut [u8], clear: usize) {
        for (i, byte) in stream.iter_mut().enumerate().skip(clear) {
            let key = self.array[i % 16];
            if *byte != 0 && *byte != key {
                *byte ^= key;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// [MS-OFFCRYPTO] §2.3.7.1, §2.3.7.2: passwords are cut to 15
    /// characters and an empty one has no key.
    #[test]
    fn passwords_are_cut_to_fifteen_characters() {
        let long = b"0123456789abcdefgh";
        assert_eq!(verifier_method2(long), verifier_method2(&long[..15]));
        assert_ne!(verifier_method2(b"a"), verifier_method2(b"b"));
        assert_eq!(xor_key_method1(b""), None);
    }

    /// [MS-OFFCRYPTO] §2.3.7.4, §2.3.7.6: the transformation undoes itself
    /// and keeps zero bytes and bytes equal to their key byte.
    #[test]
    fn transformation_round_trips() {
        let verifier = verifier_method2(b"docboss").expect("verifier");
        let xor = XorObfuscation::open("docboss", verifier, 1252).expect("opens");
        assert_eq!(
            XorObfuscation::open("wrong", verifier, 1252),
            Err(Error::WrongPassword)
        );
        let plain: Vec<u8> = (0..=255u8).chain(xor.array).collect();
        let mut data = plain.clone();
        xor.apply(&mut data, 4);
        assert_eq!(data[..4], plain[..4]);
        assert_ne!(data, plain);
        assert_eq!(data[256..], plain[256..]);
        assert_eq!(data[16], 0x10 ^ xor.array[0]);
        xor.apply(&mut data, 4);
        assert_eq!(data, plain);
    }

    /// [MS-OFFCRYPTO] §2.3.7.4: a password with characters outside the
    /// code page still opens through the low-byte conversion.
    #[test]
    fn unicode_passwords_open_by_their_low_bytes() {
        let verifier = verifier_method2(&[0x1F, b'x']).expect("verifier");
        assert!(XorObfuscation::open("\u{41F}x", verifier, 1252).is_ok());
        let cyrillic = verifier_method2(&[0xCF, b'x']).expect("verifier");
        assert!(XorObfuscation::open("\u{41F}x", cyrillic, 1251).is_ok());
    }
}
