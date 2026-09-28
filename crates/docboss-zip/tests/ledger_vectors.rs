use docboss_zip::crc32;

fn hex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
        .collect()
}

/// APPNOTE §4.4.7: the CRC-32 matches the Lean reference in
/// `ledger/Ledger/Reference/Crc32.lean` on every vector it wrote.
#[test]
fn crc32_matches_the_ledger_reference() {
    let vectors = include_str!("vectors/ledger/crc32.tsv");
    let mut count = 0;
    for line in vectors
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
    {
        let fields: Vec<&str> = line.split('\t').collect();
        let [name, input, expected] = fields[..] else {
            panic!("bad vector line {line:?}")
        };
        let expected = u32::from_str_radix(expected, 16).unwrap();
        assert_eq!(crc32(&hex(input)), expected, "{name}");
        count += 1;
    }
    assert!(count > 10);
}
