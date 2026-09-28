import Ledger.Feature

/-!
Citations of the tracked specifications in the docboss source tree, written by
`lake exe ledger-index`. Regenerate rather than edit.
-/

namespace Ledger

def Generated.citations0 : List Citation := [
  ⟨.msDoc, none, "crates/docboss-model/src/document.rs", 13, false⟩,
  ⟨.msDoc, none, "crates/docboss-model/src/lib.rs", 4, false⟩,
  ⟨.ecma376Part1, some (.clause [17, 7, 3]), "crates/docboss-model/src/style.rs", 147, false⟩,
  ⟨.ecma376Part1, some (.clause [17, 18, 59]), "crates/docboss-model/tests/ledger_vectors.rs", 2, true⟩,
  ⟨.ecma376Part1, some (.clause [17, 7, 3]), "crates/docboss-model/tests/resolution.rs", 13, true⟩,
  ⟨.ecma376Part1, some (.clause [17, 7, 4, 3]), "crates/docboss-model/tests/resolution.rs", 48, true⟩,
  ⟨.ecma376Part1, some (.clause [17, 9, 11]), "crates/docboss-model/tests/resolution.rs", 102, true⟩
]

def Generated.citations : List Citation :=
  Generated.citations0

end Ledger
