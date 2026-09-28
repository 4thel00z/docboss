import Ledger.Ref

/-!
The clause outline of [MS-CFB]: Compound File Binary File Format,
derived from the specification text by `tools/outline.py`. Regenerate rather than edit.
-/

namespace Ledger.Outline.MsCfb

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
  ⟨.clause [1, 4], "Relationship to Protocols and Other Structures"⟩,
  ⟨.clause [1, 5], "Applicability Statement"⟩,
  ⟨.clause [1, 6], "Versioning and Localization"⟩,
  ⟨.clause [1, 7], "Vendor-Extensible Fields"⟩,
  ⟨.clause [2, 1], "Compound File Sector Numbers and Types"⟩,
  ⟨.clause [2, 2], "Compound File Header"⟩,
  ⟨.clause [2, 3], "Compound File FAT Sectors"⟩,
  ⟨.clause [2, 4], "Compound File Mini FAT Sectors"⟩,
  ⟨.clause [2, 5], "Compound File DIFAT Sectors"⟩,
  ⟨.clause [2, 6], "Compound File Directory Sectors"⟩,
  ⟨.clause [2, 6, 1], "Compound File Directory Entry"⟩,
  ⟨.clause [2, 6, 2], "Root Directory Entry"⟩,
  ⟨.clause [2, 6, 3], "Other Directory Entries"⟩,
  ⟨.clause [2, 6, 4], "Red-Black Tree"⟩,
  ⟨.clause [2, 7], "Compound File User-Defined Data Sectors"⟩,
  ⟨.clause [2, 8], "Compound File Range Lock Sector"⟩,
  ⟨.clause [2, 9], "Compound File Size Limits"⟩,
  ⟨.clause [3, 1], "The Header"⟩,
  ⟨.clause [3, 2], "Sector #0: FAT Sector"⟩,
  ⟨.clause [3, 3], "Sector #1: Directory Sector"⟩,
  ⟨.clause [3, 3, 1], "Stream ID 0: Root Directory Entry"⟩,
  ⟨.clause [3, 3, 2], "Stream ID 1: Storage 1"⟩,
  ⟨.clause [3, 3, 3], "Stream ID 2: Stream 1"⟩,
  ⟨.clause [3, 3, 4], "Stream ID 3: Unused, Free"⟩,
  ⟨.clause [3, 4], "Sector #2: MiniFAT Sector"⟩,
  ⟨.clause [3, 5], "Sector #3: Mini Stream Sector"⟩,
  ⟨.clause [4, 1], "Validation and Corruption"⟩,
  ⟨.clause [4, 2], "File Security"⟩,
  ⟨.clause [4, 3], "Unallocated Ranges"⟩
]

/-- Every numbered heading of the specification, in document order. -/
def headings : List Heading := headings0

end Ledger.Outline.MsCfb
