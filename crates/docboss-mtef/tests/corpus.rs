//! Equation Native streams taken from corpus documents (Apache POI's
//! Bug61268.doc and Bug50936_1.doc).

fn equation(name: &str) -> String {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    let bytes = std::fs::read(path).expect("fixture");
    let math = docboss_mtef::read(&bytes).expect("the equation reads");
    docboss_model::linear_text(&math.nodes)
}

/// A script attaches to the one character before it: Q_A = eP + Q_CA, not
/// a script on the whole run before it.
#[test]
fn scripts_attach_to_the_character_before_them() {
    assert_eq!(equation("bug61268-charge.bin"), "Q_A=eP+Q_(CA)");
    assert_eq!(
        equation("bug50936-variance.bin"),
        "var(¯(x)(y))=(s^2(y))/(n(y))"
    );
    assert_eq!(
        equation("bug50936-mean.bin"),
        "¯(x)   =   (Σ_(y=1)^34▒ tf_y ¯(x)(y))/(Σ_(y=1)^34▒ tf_y)"
    );
}
