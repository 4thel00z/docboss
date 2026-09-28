//! Holds `number_label` to the list label vectors the ledger's Lean
//! reference writes for ECMA-376 Part 1 §17.18.59 (`ST_NumberFormat`).

use docboss_model::{number_label, NumberFormat};

const VECTORS: &str = include_str!("vectors/ledger/number_format.tsv");

fn format(name: &str) -> NumberFormat {
    match name {
        "decimal" => NumberFormat::Decimal,
        "upperRoman" => NumberFormat::UpperRoman,
        "lowerRoman" => NumberFormat::LowerRoman,
        "upperLetter" => NumberFormat::UpperLetter,
        "lowerLetter" => NumberFormat::LowerLetter,
        "ordinal" => NumberFormat::Ordinal,
        other => panic!("unknown format {other} in the vectors"),
    }
}

#[test]
fn number_labels_match_the_reference() {
    let mut checked = 0;
    for line in VECTORS.lines().filter(|line| !line.starts_with('#') && !line.is_empty()) {
        let fields: Vec<&str> = line.split('\t').collect();
        let [name, number, label] = fields[..] else {
            panic!("malformed vector line {line:?}");
        };
        let n: u32 = number.parse().expect("vector number");
        assert_eq!(number_label(&format(name), n), label, "{name} {n}");
        checked += 1;
    }
    assert!(checked > 900, "only {checked} vectors read");
}
