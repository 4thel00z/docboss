use std::path::PathBuf;

use docboss_crypt::{decrypt, is_encrypted_package, scheme, Error, Scheme};

fn fixture(name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    std::fs::read(path).unwrap()
}

fn info_scheme(bytes: &[u8]) -> Scheme {
    let file = docboss_cfb::CompoundFile::parse(bytes).unwrap();
    scheme(&file.open_stream("EncryptionInfo").unwrap()).unwrap()
}

fn opens(name: &str, password: &str) -> Vec<u8> {
    let bytes = fixture(name);
    assert!(is_encrypted_package(&bytes));
    let package = decrypt(&bytes, password).unwrap();
    assert!(
        package.starts_with(b"PK\x03\x04"),
        "{name} did not decrypt to a ZIP"
    );
    package
}

/// [MS-OFFCRYPTO] §2.3.4.5, §2.3.4.7, §2.3.4.9: Standard encryption as
/// Word 2007 and LibreOffice write it.
#[test]
fn standard_encryption_opens_with_its_password() {
    for (name, password) in [
        ("Encrypted_MSO2007_abc.docx", "abc"),
        ("Encrypted_LO_Standard_abc.docx", "abc"),
        ("bug53475-password-is-solrcell.docx", "solrcell"),
    ] {
        assert_eq!(info_scheme(&fixture(name)), Scheme::Standard, "{name}");
        opens(name, password);
        assert_eq!(decrypt(&fixture(name), "abd"), Err(Error::WrongPassword));
    }
}

/// [MS-OFFCRYPTO] §2.3.4.10 to §2.3.4.14: Agile encryption with SHA-1
/// (Word 2010) and SHA-512 (Word 2013), the data integrity HMAC checked.
#[test]
fn agile_encryption_opens_with_its_password() {
    for (name, password) in [
        ("Encrypted_MSO2010_abc.docx", "abc"),
        ("Encrypted_MSO2013_abc.docx", "abc"),
        ("bug53475-password-is-pass.docx", "pass"),
    ] {
        assert_eq!(info_scheme(&fixture(name)), Scheme::Agile, "{name}");
        opens(name, password);
        assert_eq!(decrypt(&fixture(name), "wrong"), Err(Error::WrongPassword));
    }
}

/// [MS-OFFCRYPTO] §2.3.4.14: a flipped byte in the encrypted package fails
/// the HMAC check instead of decrypting to damaged data.
#[test]
fn agile_integrity_check_catches_tampering() {
    let bytes = fixture("Encrypted_MSO2013_abc.docx");
    let file = docboss_cfb::CompoundFile::parse(&bytes).unwrap();
    let info = file.open_stream("EncryptionInfo").unwrap().into_owned();
    let mut package = file.open_stream("EncryptedPackage").unwrap().into_owned();
    let last = package.len() - 1;
    package[last] ^= 1;
    assert_eq!(
        docboss_crypt::decrypt_streams(&info, &package, "abc"),
        Err(Error::Integrity)
    );
}

#[test]
fn plain_zip_is_not_an_encrypted_package() {
    assert!(!is_encrypted_package(b"PK\x03\x04rest"));
    assert!(matches!(
        decrypt(b"not a compound file", "x"),
        Err(Error::Cfb(_))
    ));
}
