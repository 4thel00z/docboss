import Ledger.Feature

/-!
Ledger rows for [MS-OLEPS]. One row per clause the scope requires, plus any deeper row a note needs.
-/

namespace Ledger.Catalogue.MsOleps

open Ledger

def rows0 : List Feature := [
  { standard := .msOleps, ref := .clause [2, 1], title := "PropertyIdentifier", status := .notImplemented,
    note := "Not read yet." },
  { standard := .msOleps, ref := .clause [2, 2], title := "PropertyType", status := .notImplemented,
    note := "Not read yet." },
  { standard := .msOleps, ref := .clause [2, 3], title := "CURRENCY (Packet Version)", status := .notImplemented,
    note := "Not read yet." },
  { standard := .msOleps, ref := .clause [2, 4], title := "DATE (Packet Version)", status := .notImplemented,
    note := "Not read yet." },
  { standard := .msOleps, ref := .clause [2, 5], title := "CodePageString", status := .notImplemented,
    note := "Not read yet." },
  { standard := .msOleps, ref := .clause [2, 6], title := "DECIMAL (Packet Version)", status := .notImplemented,
    note := "Not read yet." },
  { standard := .msOleps, ref := .clause [2, 7], title := "UnicodeString", status := .notImplemented,
    note := "Not read yet." },
  { standard := .msOleps, ref := .clause [2, 8], title := "FILETIME (Packet Version)", status := .notImplemented,
    note := "Not read yet." },
  { standard := .msOleps, ref := .clause [2, 9], title := "BLOB", status := .notImplemented,
    note := "Not read yet." },
  { standard := .msOleps, ref := .clause [2, 10], title := "IndirectPropertyName", status := .notImplemented,
    note := "Not read yet." },
  { standard := .msOleps, ref := .clause [2, 11], title := "ClipboardData", status := .notImplemented,
    note := "Not read yet." },
  { standard := .msOleps, ref := .clause [2, 12], title := "GUID (Packet Version)", status := .notImplemented,
    note := "Not read yet." },
  { standard := .msOleps, ref := .clause [2, 13], title := "VersionedStream", status := .notImplemented,
    note := "Not read yet." },
  { standard := .msOleps, ref := .clause [2, 14], title := "Vector and Array Property Types", status := .notImplemented,
    note := "Not read yet." },
  { standard := .msOleps, ref := .clause [2, 14, 1], title := "Property Types in Variable-Typed Vectors and Arrays", status := .notImplemented,
    note := "Not read yet." },
  { standard := .msOleps, ref := .clause [2, 14, 2], title := "VectorHeader", status := .notImplemented,
    note := "Not read yet." },
  { standard := .msOleps, ref := .clause [2, 14, 3], title := "ArrayDimension", status := .notImplemented,
    note := "Not read yet." },
  { standard := .msOleps, ref := .clause [2, 14, 4], title := "ArrayHeader", status := .notImplemented,
    note := "Not read yet." },
  { standard := .msOleps, ref := .clause [2, 15], title := "TypedPropertyValue", status := .notImplemented,
    note := "Not read yet." },
  { standard := .msOleps, ref := .clause [2, 16], title := "DictionaryEntry", status := .notImplemented,
    note := "Not read yet." },
  { standard := .msOleps, ref := .clause [2, 17], title := "Dictionary", status := .notImplemented,
    note := "Not read yet." },
  { standard := .msOleps, ref := .clause [2, 18], title := "Special Properties", status := .notImplemented,
    note := "Not read yet." },
  { standard := .msOleps, ref := .clause [2, 18, 1], title := "Dictionary Property", status := .notImplemented,
    note := "Not read yet." },
  { standard := .msOleps, ref := .clause [2, 18, 2], title := "CodePage Property", status := .notImplemented,
    note := "Not read yet." },
  { standard := .msOleps, ref := .clause [2, 18, 3], title := "Locale Property", status := .notImplemented,
    note := "Not read yet." },
  { standard := .msOleps, ref := .clause [2, 18, 4], title := "Behavior Property", status := .notImplemented,
    note := "Not read yet." },
  { standard := .msOleps, ref := .clause [2, 19], title := "PropertyIdentifierAndOffset", status := .notImplemented,
    note := "Not read yet." },
  { standard := .msOleps, ref := .clause [2, 20], title := "PropertySet", status := .notImplemented,
    note := "Not read yet." },
  { standard := .msOleps, ref := .clause [2, 21], title := "PropertySetStream", status := .notImplemented,
    note := "Not read yet." },
  { standard := .msOleps, ref := .clause [2, 22], title := "Non-Simple Property Set Storage Format", status := .notImplemented,
    note := "Not read yet." },
  { standard := .msOleps, ref := .clause [2, 23], title := "Property Set Stream and Storage Names", status := .notImplemented,
    note := "Not read yet." },
  { standard := .msOleps, ref := .clause [2, 24], title := "Standard Bindings", status := .notImplemented,
    note := "Not read yet." },
  { standard := .msOleps, ref := .clause [2, 24, 1], title := "Compound File Binding", status := .notImplemented,
    note := "Not read yet." },
  { standard := .msOleps, ref := .clause [2, 24, 2], title := "Alternate Stream Binding", status := .notImplemented,
    note := "Not read yet." },
  { standard := .msOleps, ref := .clause [2, 24, 3], title := "Control Stream", status := .notImplemented,
    note := "Not read yet." },
  { standard := .msOleps, ref := .clause [2, 24, 4], title := "Simple Property Set Stream", status := .notImplemented,
    note := "Not read yet." },
  { standard := .msOleps, ref := .clause [2, 24, 5], title := "Non-Simple Property Set Storage", status := .notImplemented,
    note := "Not read yet." },
  { standard := .msOleps, ref := .clause [2, 25], title := "Well-Known Property Set Formats", status := .notImplemented,
    note := "Not read yet." },
  { standard := .msOleps, ref := .clause [2, 25, 1], title := "SummaryInformation", status := .notImplemented,
    note := "Not read yet." },
  { standard := .msOleps, ref := .clause [2, 25, 2], title := "PropertyBag", status := .notImplemented,
    note := "Not read yet." }
]

/-- Every row of the [MS-OLEPS] ledger. -/
def rows : List Feature :=
  rows0

end Ledger.Catalogue.MsOleps
