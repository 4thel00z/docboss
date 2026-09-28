import Ledger.Feature

/-!
Ledger rows for ECMA-376 Part 3. One row per clause the scope requires, plus any deeper row a note needs.
-/

namespace Ledger.Catalogue.Ecma376Part3

open Ledger

def rows0 : List Feature := [
  { standard := .ecma376Part3, ref := .clause [7, 2], title := "Ignorable Attribute", status := .notImplemented,
    note := "mc:Ignorable is not read; elements in namespaces the reader does not know are skipped whether or not they are ignorable." },
  { standard := .ecma376Part3, ref := .clause [7, 3], title := "ProcessContent Attribute", status := .notImplemented,
    note := "ProcessContent is not read; the content of unknown elements is dropped." },
  { standard := .ecma376Part3, ref := .clause [7, 4], title := "MustUnderstand Attribute", status := .notImplemented,
    note := "MustUnderstand is not read; documents that set it are not refused." },
  { standard := .ecma376Part3, ref := .clause [7, 5], title := "AlternateContent Element", status := .implemented,
    note := "AlternateContent is read at block, run and drawing level through one chooser (story.rs alternate_content, test structure.rs markup_compatibility_and_content_controls)." },
  { standard := .ecma376Part3, ref := .clause [7, 6], title := "Choice Element", status := .implemented,
    note := "The first Choice whose Requires namespaces the reader understands is read (test structure.rs markup_compatibility_and_content_controls)." },
  { standard := .ecma376Part3, ref := .clause [7, 7], title := "Fallback Element", status := .implemented,
    note := "Fallback is read when no Choice is understood (test structure.rs markup_compatibility_and_content_controls)." },
  { standard := .ecma376Part3, ref := .clause [9, 1], title := "Overview", status := .outOfScope,
    note := "Informative overview." },
  { standard := .ecma376Part3, ref := .clause [9, 2], title := "Step 1: Processing the Ignorable and ProcessContent Attributes", status := .notImplemented,
    note := "Step 1 is not performed: Ignorable and ProcessContent are not read." },
  { standard := .ecma376Part3, ref := .clause [9, 3], title := "Step 2: Processing the AlternateContent, Choice and Fallback Elements", status := .implemented,
    note := "Choice and Fallback selection follows step 2 (story.rs alternate_content, test structure.rs markup_compatibility_and_content_controls)." },
  { standard := .ecma376Part3, ref := .clause [9, 4], title := "Step 3: Processing the MustUnderstand Attribute and Creating the Output Document", status := .notImplemented,
    note := "Step 3 is not performed: MustUnderstand is not checked." }
]

/-- Every row of the ECMA-376 Part 3 ledger. -/
def rows : List Feature :=
  rows0

end Ledger.Catalogue.Ecma376Part3
