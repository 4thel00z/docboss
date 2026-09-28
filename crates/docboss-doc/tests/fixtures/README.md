# Fixtures

- `text`, `lists`, `tables`, `notes`, `image`, `sections`: `.fodt` sources
  written for these tests, converted with
  `soffice --headless --convert-to doc`; the `.txt` files are
  `soffice --headless --convert-to "txt:Text (encoded):UTF8"` of the `.doc`.
- `encrypted-cryptoapi.doc` (password `password`) and `encrypted-rc4.doc`
  (password `tika`): Apache POI test data (`password_password_cryptoapi.doc`,
  `password_tika_binaryrc4.doc`), Apache License 2.0.
