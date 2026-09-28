use hmac::{Hmac, Mac};
use sha1::Sha1;
use sha2::{Digest, Sha256, Sha384, Sha512};

/// The hash algorithms an Agile descriptor may name ([MS-OFFCRYPTO]
/// §2.3.4.10), plus SHA-1 for Standard encryption.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hash {
    Sha1,
    Sha256,
    Sha384,
    Sha512,
}

impl Hash {
    pub fn from_name(name: &str) -> Option<Hash> {
        match name.to_ascii_uppercase().replace('-', "").as_str() {
            "SHA1" => Some(Hash::Sha1),
            "SHA256" => Some(Hash::Sha256),
            "SHA384" => Some(Hash::Sha384),
            "SHA512" => Some(Hash::Sha512),
            _ => None,
        }
    }

    pub fn size(self) -> usize {
        match self {
            Hash::Sha1 => 20,
            Hash::Sha256 => 32,
            Hash::Sha384 => 48,
            Hash::Sha512 => 64,
        }
    }

    /// The digest of the concatenation of `parts`.
    pub fn digest(self, parts: &[&[u8]]) -> Vec<u8> {
        fn run<D: Digest>(parts: &[&[u8]]) -> Vec<u8> {
            let mut hasher = D::new();
            parts.iter().for_each(|part| hasher.update(part));
            hasher.finalize().to_vec()
        }
        match self {
            Hash::Sha1 => run::<Sha1>(parts),
            Hash::Sha256 => run::<Sha256>(parts),
            Hash::Sha384 => run::<Sha384>(parts),
            Hash::Sha512 => run::<Sha512>(parts),
        }
    }

    /// Iterates `H(iterator ‖ H)` `spin` times over `seed`, the password
    /// hashing loop of [MS-OFFCRYPTO] §2.3.4.7 and §2.3.4.11.
    pub fn spin(self, seed: Vec<u8>, spin: u32) -> Vec<u8> {
        fn run<D: Digest>(seed: Vec<u8>, spin: u32) -> Vec<u8> {
            let mut current = seed;
            for i in 0..spin {
                let mut hasher = D::new();
                hasher.update(i.to_le_bytes());
                hasher.update(&current);
                current = hasher.finalize().to_vec();
            }
            current
        }
        match self {
            Hash::Sha1 => run::<Sha1>(seed, spin),
            Hash::Sha256 => run::<Sha256>(seed, spin),
            Hash::Sha384 => run::<Sha384>(seed, spin),
            Hash::Sha512 => run::<Sha512>(seed, spin),
        }
    }

    /// HMAC over `data`; an empty result when the key is rejected, which
    /// then fails any comparison.
    pub fn hmac(self, key: &[u8], data: &[u8]) -> Vec<u8> {
        macro_rules! run {
            ($digest:ty) => {{
                let Ok(mut mac) = <Hmac<$digest> as Mac>::new_from_slice(key) else {
                    return Vec::new();
                };
                mac.update(data);
                mac.finalize().into_bytes().to_vec()
            }};
        }
        match self {
            Hash::Sha1 => run!(Sha1),
            Hash::Sha256 => run!(Sha256),
            Hash::Sha384 => run!(Sha384),
            Hash::Sha512 => run!(Sha512),
        }
    }
}
