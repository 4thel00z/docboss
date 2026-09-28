import Ledger.Ref

/-!
The clause outline of ECMA-376 Part 3, 5th edition (2015): Markup Compatibility and Extensibility,
derived from the specification text by `tools/outline.py`. Regenerate rather than edit.
-/

namespace Ledger.Outline.Ecma376Part3

/-- Chapter titles by number. -/
def chapters : List (Nat × String) := [
  (1, "Scope"),
  (2, "Normative References"),
  (3, "Terms and Definitions"),
  (4, "Notational Conventions"),
  (5, "General Description"),
  (6, "Overview"),
  (7, "MCE Elements and Attributes"),
  (8, "Application-Defined Extension Elements"),
  (9, "Semantic Definitions and Reference Processing Model")
]

/-- Annex titles by letter. -/
def annexes : List (Char × String) := [
  ('A', "Examples"),
  ('B', "Validation Using NVDL")
]

def headings0 : List Heading := [
  ⟨.clause [7, 1], "Introduction"⟩,
  ⟨.clause [7, 2], "Ignorable Attribute"⟩,
  ⟨.clause [7, 3], "ProcessContent Attribute"⟩,
  ⟨.clause [7, 4], "MustUnderstand Attribute"⟩,
  ⟨.clause [7, 5], "AlternateContent Element"⟩,
  ⟨.clause [7, 6], "Choice Element"⟩,
  ⟨.clause [7, 7], "Fallback Element"⟩,
  ⟨.clause [9, 1], "Overview"⟩,
  ⟨.clause [9, 2], "Step 1: Processing the Ignorable and ProcessContent Attributes"⟩,
  ⟨.clause [9, 3], "Step 2: Processing the AlternateContent, Choice and Fallback Elements"⟩,
  ⟨.clause [9, 4], "Step 3: Processing the MustUnderstand Attribute and Creating the Output Document"⟩,
  ⟨.annex 'A' [1], "Syntactic Examples"⟩,
  ⟨.annex 'A' [1, 1], "General"⟩,
  ⟨.annex 'A' [1, 2], "Ignorable Attribute: Multiple Prefixes Bound to a Namespace"⟩,
  ⟨.annex 'A' [1, 3], "Ignorable Attribute: Non-conformant Use"⟩,
  ⟨.annex 'A' [1, 4], "ProcessContent Attribute: Multiple Prefixes Bound to a Namespace"⟩,
  ⟨.annex 'A' [1, 5], "ProcessContent Attribute: Non-conformant Use"⟩,
  ⟨.annex 'A' [1, 6], "MustUnderstand Attribute: Non-conformant Use"⟩,
  ⟨.annex 'A' [1, 7], "AlternateContent Element: Future Extensibility"⟩,
  ⟨.annex 'A' [2], "Semantic Examples"⟩,
  ⟨.annex 'A' [2, 1], "General"⟩,
  ⟨.annex 'A' [2, 2], "Ignorable Attribute"⟩,
  ⟨.annex 'A' [2, 3], "Ignorable and ProcessContent Attributes"⟩,
  ⟨.annex 'A' [2, 4], "Non-Ignorable and Non-Understood Namespace"⟩,
  ⟨.annex 'A' [2, 5], "MustUnderstand Attribute"⟩,
  ⟨.annex 'A' [2, 6], "AlternateContent Element"⟩,
  ⟨.annex 'A' [2, 7], "Ignorable Content Inside Application-Defined Extension Elements"⟩
]

/-- Every numbered heading of the specification, in document order. -/
def headings : List Heading := headings0

end Ledger.Outline.Ecma376Part3
