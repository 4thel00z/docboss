import Ledger.Ref

/-!
The clause outline of [MS-OLEPS]: Object Linking and Embedding (OLE) Property Set Data Structures,
derived from the specification text by `tools/outline.py`. Regenerate rather than edit.
-/

namespace Ledger.Outline.MsOleps

/-- Chapter titles by number. -/
def chapters : List (Nat × String) := [
  (1, "Introduction"),
  (2, "Structures"),
  (3, "Structure Examples"),
  (4, "Security Considerations"),
  (5, "Appendix A: Product Behavior"),
  (6, "Change Tracking"),
  (7, "Index")
]

/-- Annex titles by letter. -/
def annexes : List (Char × String) := [

]

def headings0 : List Heading := [
  ⟨.clause [1, 1], "Glossary"⟩,
  ⟨.clause [1, 2], "References"⟩,
  ⟨.clause [1, 2, 1], "Normative References"⟩,
  ⟨.clause [1, 2, 2], "Informative References"⟩,
  ⟨.clause [1, 3], "Overview"⟩,
  ⟨.clause [1, 3, 1], "Background"⟩,
  ⟨.clause [1, 3, 2], "Properties and Property Sets"⟩,
  ⟨.clause [1, 4], "Relationship to Protocols and Other Structures"⟩,
  ⟨.clause [1, 5], "Applicability Statement"⟩,
  ⟨.clause [1, 6], "Versioning and Localization"⟩,
  ⟨.clause [1, 7], "Vendor-Extensible Fields"⟩,
  ⟨.clause [2, 1], "PropertyIdentifier"⟩,
  ⟨.clause [2, 2], "PropertyType"⟩,
  ⟨.clause [2, 3], "CURRENCY (Packet Version)"⟩,
  ⟨.clause [2, 4], "DATE (Packet Version)"⟩,
  ⟨.clause [2, 5], "CodePageString"⟩,
  ⟨.clause [2, 6], "DECIMAL (Packet Version)"⟩,
  ⟨.clause [2, 7], "UnicodeString"⟩,
  ⟨.clause [2, 8], "FILETIME (Packet Version)"⟩,
  ⟨.clause [2, 9], "BLOB"⟩,
  ⟨.clause [2, 10], "IndirectPropertyName"⟩,
  ⟨.clause [2, 11], "ClipboardData"⟩,
  ⟨.clause [2, 12], "GUID (Packet Version)"⟩,
  ⟨.clause [2, 13], "VersionedStream"⟩,
  ⟨.clause [2, 14], "Vector and Array Property Types"⟩,
  ⟨.clause [2, 14, 1], "Property Types in Variable-Typed Vectors and Arrays"⟩,
  ⟨.clause [2, 14, 2], "VectorHeader"⟩,
  ⟨.clause [2, 14, 3], "ArrayDimension"⟩,
  ⟨.clause [2, 14, 4], "ArrayHeader"⟩,
  ⟨.clause [2, 15], "TypedPropertyValue"⟩,
  ⟨.clause [2, 16], "DictionaryEntry"⟩,
  ⟨.clause [2, 17], "Dictionary"⟩,
  ⟨.clause [2, 18], "Special Properties"⟩,
  ⟨.clause [2, 18, 1], "Dictionary Property"⟩,
  ⟨.clause [2, 18, 2], "CodePage Property"⟩,
  ⟨.clause [2, 18, 3], "Locale Property"⟩,
  ⟨.clause [2, 18, 4], "Behavior Property"⟩,
  ⟨.clause [2, 19], "PropertyIdentifierAndOffset"⟩,
  ⟨.clause [2, 20], "PropertySet"⟩,
  ⟨.clause [2, 21], "PropertySetStream"⟩,
  ⟨.clause [2, 22], "Non-Simple Property Set Storage Format"⟩,
  ⟨.clause [2, 23], "Property Set Stream and Storage Names"⟩,
  ⟨.clause [2, 24], "Standard Bindings"⟩,
  ⟨.clause [2, 24, 1], "Compound File Binding"⟩,
  ⟨.clause [2, 24, 2], "Alternate Stream Binding"⟩,
  ⟨.clause [2, 24, 3], "Control Stream"⟩,
  ⟨.clause [2, 24, 4], "Simple Property Set Stream"⟩,
  ⟨.clause [2, 24, 5], "Non-Simple Property Set Storage"⟩,
  ⟨.clause [2, 25], "Well-Known Property Set Formats"⟩,
  ⟨.clause [2, 25, 1], "SummaryInformation"⟩,
  ⟨.clause [2, 25, 2], "PropertyBag"⟩,
  ⟨.clause [3, 1], "SummaryInformation Property Set"⟩,
  ⟨.clause [3, 1, 1], "CodePage Property"⟩,
  ⟨.clause [3, 1, 2], "PIDSI_TITLE"⟩,
  ⟨.clause [3, 1, 3], "PIDSI_SUBJECT"⟩,
  ⟨.clause [3, 1, 4], "PIDSI_AUTHOR"⟩,
  ⟨.clause [3, 1, 5], "PIDSI_KEYWORDS"⟩,
  ⟨.clause [3, 1, 6], "PIDSI_COMMENTS"⟩,
  ⟨.clause [3, 1, 7], "PIDSI_TEMPLATE"⟩,
  ⟨.clause [3, 1, 8], "PIDSI_LASTAUTHOR"⟩,
  ⟨.clause [3, 1, 9], "PIDSI_REVNUMBER"⟩,
  ⟨.clause [3, 1, 10], "PIDSI_APPNAME"⟩,
  ⟨.clause [3, 1, 11], "PIDSI_EDITTIME"⟩,
  ⟨.clause [3, 1, 12], "PIDSI_LASTPRINTED"⟩,
  ⟨.clause [3, 1, 13], "PIDSI_CREATE_DTM"⟩,
  ⟨.clause [3, 1, 14], "PIDSI_LASTSAVE_DTM"⟩,
  ⟨.clause [3, 1, 15], "PIDSI_PAGECOUNT"⟩,
  ⟨.clause [3, 1, 16], "PIDSI_WORDCOUNT"⟩,
  ⟨.clause [3, 1, 17], "PIDSI_CHARCOUNT"⟩,
  ⟨.clause [3, 1, 18], "PIDSI_DOC_SECURITY"⟩,
  ⟨.clause [3, 2], "PropertyBag Property Set"⟩,
  ⟨.clause [3, 2, 1], "Control Stream (\"{4c8cc155-6c1e-11d1-8e41-00c04fb9386d}\")"⟩,
  ⟨.clause [3, 2, 2], "PropertyBag Stream (\"Docf_\\005Bagaaqy23kudbhchAaq5u2chNd\")"⟩,
  ⟨.clause [3, 2, 2, 1, 1], "CodePage"⟩,
  ⟨.clause [3, 2, 2, 1, 2], "Locale"⟩,
  ⟨.clause [3, 2, 2, 1, 3], "Behavior"⟩,
  ⟨.clause [3, 2, 2, 1, 4], "Dictionary"⟩,
  ⟨.clause [3, 2, 2, 1, 4, 1], "Dictionary Entry 0"⟩,
  ⟨.clause [3, 2, 2, 1, 4, 2], "Dictionary Entry 1"⟩,
  ⟨.clause [3, 2, 2, 1, 4, 3], "Dictionary Entry 2"⟩,
  ⟨.clause [3, 2, 2, 1, 4, 4], "Dictionary Entry 3"⟩,
  ⟨.clause [3, 2, 2, 1, 4, 5], "Dictionary Entry 4"⟩,
  ⟨.clause [3, 2, 2, 1, 4, 6], "Dictionary Entry 5"⟩,
  ⟨.clause [3, 2, 2, 1, 5], "DisplayColour"⟩,
  ⟨.clause [3, 2, 2, 1, 6], "MyStream"⟩,
  ⟨.clause [3, 2, 2, 1, 7], "Price(GBP)"⟩,
  ⟨.clause [3, 2, 2, 1, 8], "MyStorage"⟩,
  ⟨.clause [3, 2, 2, 1, 9], "CaseSensitive Mixed Case"⟩,
  ⟨.clause [3, 2, 2, 1, 10], "CASESENSITIVE All Uppercase"⟩
]

/-- Every numbered heading of the specification, in document order. -/
def headings : List Heading := headings0

end Ledger.Outline.MsOleps
