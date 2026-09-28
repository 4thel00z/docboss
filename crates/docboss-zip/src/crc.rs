//! CRC-32 (IEEE 802.3, reflected polynomial 0xEDB88320), as APPNOTE §4.4.7
//! requires for every entry, computed eight bytes at a time.

const POLY: u32 = 0xEDB8_8320;

const fn tables() -> [[u32; 256]; 8] {
    let mut tables = [[0u32; 256]; 8];
    let mut i = 0;
    while i < 256 {
        let mut crc = i as u32;
        let mut bit = 0;
        while bit < 8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ POLY
            } else {
                crc >> 1
            };
            bit += 1;
        }
        tables[0][i] = crc;
        i += 1;
    }
    let mut i = 0;
    while i < 256 {
        let mut t = 1;
        while t < 8 {
            let previous = tables[t - 1][i];
            tables[t][i] = (previous >> 8) ^ tables[0][(previous & 0xFF) as usize];
            t += 1;
        }
        i += 1;
    }
    tables
}

static TABLES: [[u32; 256]; 8] = tables();

/// Continues a CRC-32 over `bytes`; start with `0`.
pub fn crc32_update(crc: u32, bytes: &[u8]) -> u32 {
    let mut crc = !crc;
    let (chunks, remainder) = bytes.as_chunks::<8>();
    for chunk in chunks {
        let low = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]) ^ crc;
        let high = u32::from_le_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]);
        crc = TABLES[7][(low & 0xFF) as usize]
            ^ TABLES[6][((low >> 8) & 0xFF) as usize]
            ^ TABLES[5][((low >> 16) & 0xFF) as usize]
            ^ TABLES[4][(low >> 24) as usize]
            ^ TABLES[3][(high & 0xFF) as usize]
            ^ TABLES[2][((high >> 8) & 0xFF) as usize]
            ^ TABLES[1][((high >> 16) & 0xFF) as usize]
            ^ TABLES[0][(high >> 24) as usize];
    }
    for &byte in remainder {
        crc = (crc >> 8) ^ TABLES[0][((crc ^ u32::from(byte)) & 0xFF) as usize];
    }
    !crc
}

/// The CRC-32 of `bytes`.
pub fn crc32(bytes: &[u8]) -> u32 {
    crc32_update(0, bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// APPNOTE §4.4.7: the check value of the ASCII digits 1 to 9.
    #[test]
    fn check_value() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(crc32(b""), 0);
    }

    #[test]
    fn sliced_matches_bytewise_on_every_length() {
        let data: Vec<u8> = (0..300u32).map(|i| (i * 31 + 7) as u8).collect();
        for len in 0..data.len() {
            let mut bytewise = !0u32;
            for &byte in &data[..len] {
                bytewise =
                    (bytewise >> 8) ^ TABLES[0][((bytewise ^ u32::from(byte)) & 0xFF) as usize];
            }
            assert_eq!(crc32(&data[..len]), !bytewise, "length {len}");
        }
        let (a, b) = data.split_at(123);
        assert_eq!(crc32_update(crc32(a), b), crc32(&data));
    }
}
