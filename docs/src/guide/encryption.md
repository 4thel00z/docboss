# Encrypted documents

Every read command takes `--password`, and `Document` takes `password=`:

```bash
docboss text locked.docx --password secret
docboss convert locked.docx --password secret -o unlocked.docx
```

```python
import docboss

doc = docboss.Document("locked.docx", password="secret")
print(doc.extract_text())
```

What opens:

- **DOCX** protected with a password to open is an OLE compound file holding
  `EncryptionInfo` and `EncryptedPackage` streams ([MS-OFFCRYPTO]). docboss
  decrypts Agile encryption (AES-128 or AES-256 in CBC mode, SHA-1 or SHA-512
  key derivation with the spin count, the data-integrity HMAC checked) and
  Standard encryption (AES-128 in ECB mode, SHA-1), then reads the package
  like any other. Word 2007 to 2013 and LibreOffice files open.
- **DOC** with RC4 or RC4 CryptoAPI encryption ([MS-DOC] with the
  [MS-OFFCRYPTO] key derivations).

A wrong password is an error that says so; a tampered Agile package fails its
integrity check. XOR-obfuscated DOC files are refused with an error.

`convert` writes the decrypted document as a plain, unencrypted DOCX; there
is no way to write an encrypted one yet.
