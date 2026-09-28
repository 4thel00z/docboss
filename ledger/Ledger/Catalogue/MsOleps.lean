import Ledger.Feature

/-!
Ledger rows for [MS-OLEPS]. One row per clause the scope requires, plus any deeper row a note needs.
-/

namespace Ledger.Catalogue.MsOleps

open Ledger

def rows0 : List Feature := [
  { standard := .msOleps, ref := .clause [2, 1], title := "PropertyIdentifier", status := .implemented,
    note := "Property identifiers 2 to 19 map to metadata fields (property.rs read_metadata, test cfb.rs reads_summary_metadata)." },
  { standard := .msOleps, ref := .clause [2, 2], title := "PropertyType", status := .implemented,
    note := "The property type selects the value decoder (property.rs parse_value, test cfb.rs reads_summary_metadata)." },
  { standard := .msOleps, ref := .clause [2, 3], title := "CURRENCY (Packet Version)", status := .notImplemented,
    note := "Not read: the metadata docboss models uses none of it." },
  { standard := .msOleps, ref := .clause [2, 4], title := "DATE (Packet Version)", status := .notImplemented,
    note := "Not read: the metadata docboss models uses none of it." },
  { standard := .msOleps, ref := .clause [2, 5], title := "CodePageString", status := .implemented,
    note := "CodePageString values decode through the set's code page, with 874, 1250 to 1258, Mac Roman, UTF-8, UTF-16 and the CJK code pages supported (codepage.rs, test cfb.rs reads_summary_metadata)." },
  { standard := .msOleps, ref := .clause [2, 6], title := "DECIMAL (Packet Version)", status := .notImplemented,
    note := "Not read: the metadata docboss models uses none of it." },
  { standard := .msOleps, ref := .clause [2, 7], title := "UnicodeString", status := .incomplete,
    note := "UnicodeString values decode (property.rs parse_value); no test file stores one." },
  { standard := .msOleps, ref := .clause [2, 8], title := "FILETIME (Packet Version)", status := .implemented,
    note := "FILETIME values become ISO 8601 dates (time.rs, unit test filetime_epoch_and_known_dates)." },
  { standard := .msOleps, ref := .clause [2, 9], title := "BLOB", status := .notImplemented,
    note := "Not read: the metadata docboss models uses none of it." },
  { standard := .msOleps, ref := .clause [2, 10], title := "IndirectPropertyName", status := .notImplemented,
    note := "Not read: the metadata docboss models uses none of it." },
  { standard := .msOleps, ref := .clause [2, 11], title := "ClipboardData", status := .notImplemented,
    note := "Not read: the metadata docboss models uses none of it." },
  { standard := .msOleps, ref := .clause [2, 12], title := "GUID (Packet Version)", status := .notImplemented,
    note := "Not read: the metadata docboss models uses none of it." },
  { standard := .msOleps, ref := .clause [2, 13], title := "VersionedStream", status := .notImplemented,
    note := "Not read: the metadata docboss models uses none of it." },
  { standard := .msOleps, ref := .clause [2, 14], title := "Vector and Array Property Types", status := .notImplemented,
    note := "Not read: the metadata docboss models uses none of it." },
  { standard := .msOleps, ref := .clause [2, 14, 1], title := "Property Types in Variable-Typed Vectors and Arrays", status := .notImplemented,
    note := "Not read: the metadata docboss models uses none of it." },
  { standard := .msOleps, ref := .clause [2, 14, 2], title := "VectorHeader", status := .notImplemented,
    note := "Not read: the metadata docboss models uses none of it." },
  { standard := .msOleps, ref := .clause [2, 14, 3], title := "ArrayDimension", status := .notImplemented,
    note := "Not read: the metadata docboss models uses none of it." },
  { standard := .msOleps, ref := .clause [2, 14, 4], title := "ArrayHeader", status := .notImplemented,
    note := "Not read: the metadata docboss models uses none of it." },
  { standard := .msOleps, ref := .clause [2, 15], title := "TypedPropertyValue", status := .incomplete,
    note := "VT_I2, VT_I4, VT_UI4, VT_BOOL, VT_FILETIME, VT_LPSTR and VT_LPWSTR decode; other types are kept as Value::Other and reported (property.rs parse_value)." },
  { standard := .msOleps, ref := .clause [2, 16], title := "DictionaryEntry", status := .notImplemented,
    note := "Not read: the metadata docboss models uses none of it." },
  { standard := .msOleps, ref := .clause [2, 17], title := "Dictionary", status := .notImplemented,
    note := "Not read: the metadata docboss models uses none of it." },
  { standard := .msOleps, ref := .clause [2, 18], title := "Special Properties", status := .incomplete,
    note := "Only the CodePage property is read among the special properties." },
  { standard := .msOleps, ref := .clause [2, 18, 1], title := "Dictionary Property", status := .notImplemented,
    note := "Not read: the metadata docboss models uses none of it." },
  { standard := .msOleps, ref := .clause [2, 18, 2], title := "CodePage Property", status := .implemented,
    note := "The CodePage property selects the decoding of the set's strings (test cfb.rs reads_summary_metadata)." },
  { standard := .msOleps, ref := .clause [2, 18, 3], title := "Locale Property", status := .notImplemented,
    note := "Not read: the metadata docboss models uses none of it." },
  { standard := .msOleps, ref := .clause [2, 18, 4], title := "Behavior Property", status := .notImplemented,
    note := "Not read: the metadata docboss models uses none of it." },
  { standard := .msOleps, ref := .clause [2, 19], title := "PropertyIdentifierAndOffset", status := .implemented,
    note := "Property identifier and offset pairs locate each value (test cfb.rs reads_summary_metadata)." },
  { standard := .msOleps, ref := .clause [2, 20], title := "PropertySet", status := .implemented,
    note := "A property set's size, count and pairs are read with bounds checks (property.rs parse_set, test cfb.rs reads_summary_metadata)." },
  { standard := .msOleps, ref := .clause [2, 21], title := "PropertySetStream", status := .incomplete,
    note := "The stream header and the first property set are read (test cfb.rs reads_summary_metadata); the user-defined second set of DocumentSummaryInformation is not." },
  { standard := .msOleps, ref := .clause [2, 22], title := "Non-Simple Property Set Storage Format", status := .notImplemented,
    note := "Not read: the metadata docboss models uses none of it." },
  { standard := .msOleps, ref := .clause [2, 23], title := "Property Set Stream and Storage Names", status := .implemented,
    note := "The summary streams are opened by their names with the 0x05 prefix (test cfb.rs reads_summary_metadata)." },
  { standard := .msOleps, ref := .clause [2, 24], title := "Standard Bindings", status := .notImplemented,
    note := "Not read: the metadata docboss models uses none of it." },
  { standard := .msOleps, ref := .clause [2, 24, 1], title := "Compound File Binding", status := .notImplemented,
    note := "Not read: the metadata docboss models uses none of it." },
  { standard := .msOleps, ref := .clause [2, 24, 2], title := "Alternate Stream Binding", status := .notImplemented,
    note := "Not read: the metadata docboss models uses none of it." },
  { standard := .msOleps, ref := .clause [2, 24, 3], title := "Control Stream", status := .notImplemented,
    note := "Not read: the metadata docboss models uses none of it." },
  { standard := .msOleps, ref := .clause [2, 24, 4], title := "Simple Property Set Stream", status := .notImplemented,
    note := "Not read: the metadata docboss models uses none of it." },
  { standard := .msOleps, ref := .clause [2, 24, 5], title := "Non-Simple Property Set Storage", status := .notImplemented,
    note := "Not read: the metadata docboss models uses none of it." },
  { standard := .msOleps, ref := .clause [2, 25], title := "Well-Known Property Set Formats", status := .incomplete,
    note := "SummaryInformation and the DocumentSummaryInformation fields docboss models are read; PropertyBag is not." },
  { standard := .msOleps, ref := .clause [2, 25, 1], title := "SummaryInformation", status := .implemented,
    note := "Title, subject, author, keywords, comments, last author, revision, application, dates and counts are read into Metadata (test cfb.rs reads_summary_metadata)." },
  { standard := .msOleps, ref := .clause [2, 25, 2], title := "PropertyBag", status := .notImplemented,
    note := "Not read: the metadata docboss models uses none of it." }
]

/-- Every row of the [MS-OLEPS] ledger. -/
def rows : List Feature :=
  rows0

end Ledger.Catalogue.MsOleps
