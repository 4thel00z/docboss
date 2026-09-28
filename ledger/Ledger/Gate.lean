import Ledger.Checks

/-!
The quality gate. Every theorem here holds the ledger in `Catalogue` to the
citations `ledger-index` found in the source tree and to the outlines of the
specifications. A false claim in the ledger, a clause whose tests stopped
citing it, or a required clause with no ledger row fails the build.
`lake exe ledger-report --check` names the cause.

The ledger theorems are decided by `native_decide`: Lean compiles the check,
runs it, and the kernel accepts the verdict through the `Lean.ofReduceBool`
axiom, so they trust the compiler as well as the kernel. The theorems of the
reference implementations in `Ledger.Reference` use the kernel only.
-/

namespace Ledger

theorem every_row_is_in_its_own_file : Gate.misfiledRows.isEmpty = true := by
  native_decide

theorem every_row_names_a_chapter_of_its_standard : Gate.unslicedRows.isEmpty = true := by
  native_decide

theorem every_row_names_a_clause_by_its_title : Gate.unknownRows.isEmpty = true := by
  native_decide

theorem ledger_rows_are_unique : Gate.duplicateRefs.isEmpty = true := by
  native_decide

theorem every_row_meets_its_obligations : Gate.offenders.isEmpty = true := by
  native_decide

theorem every_required_clause_is_spoken_to : Gate.unaddressed.isEmpty = true := by
  native_decide

theorem every_cited_clause_exists : Gate.danglingCitations.isEmpty = true := by
  native_decide

end Ledger
