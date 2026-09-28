/-!
The list label formats of ECMA-376 Part 1 §17.18.59 (`ST_NumberFormat`) that
docboss numbers with, as an executable reference: `decimal`, `upperRoman`,
`lowerRoman`, `upperLetter`, `lowerLetter` and `ordinal` in English. A label
is a list of ASCII codes.

For decimal, letter and ordinal labels, `parse` inverts `format` for every
number at least 1, proved by the kernel below. Roman labels have no such
theorem: the greedy numeral expansion is only checked against the vectors.
-/

namespace Ledger.Reference.NumberFormat

/-- The decimal digits of `n`, most significant first. -/
def digits (n : Nat) : List Nat :=
  if n < 10 then [n] else digits (n / 10) ++ [n % 10]
termination_by n
decreasing_by omega

def fromDigits (ds : List Nat) : Nat := ds.foldl (fun acc d => acc * 10 + d) 0

theorem fromDigits_snoc (ds : List Nat) (d : Nat) :
    fromDigits (ds ++ [d]) = fromDigits ds * 10 + d := by
  simp [fromDigits, List.foldl_append]

theorem fromDigits_digits (n : Nat) : fromDigits (digits n) = n := by
  induction n using Nat.strongRecOn with
  | _ n ih =>
    unfold digits
    split
    · simp [fromDigits]
    · rw [fromDigits_snoc, ih (n / 10) (by omega)]
      omega

theorem digits_lt (n : Nat) : ∀ d ∈ digits n, d < 10 := by
  induction n using Nat.strongRecOn with
  | _ n ih =>
    unfold digits
    split
    · intro d hd
      simp at hd
      omega
    · intro d hd
      simp only [List.mem_append, List.mem_singleton] at hd
      rcases hd with hd | hd
      · exact ih (n / 10) (by omega) d hd
      · omega

def decimal (n : Nat) : List Nat := (digits n).map (· + 48)

def parseDecimal (cs : List Nat) : Nat := fromDigits (cs.map (· - 48))

theorem parseDecimal_decimal (n : Nat) : parseDecimal (decimal n) = n := by
  unfold parseDecimal decimal
  rw [List.map_map]
  have : (digits n).map ((· - 48) ∘ (· + 48)) = digits n := by
    apply List.map_id''
    intro x
    simp
  rw [this, fromDigits_digits]

/-- `A` to `Z`, then `AA` to `ZZ`, and so on, from the letter at `base`. -/
def letters (base : Nat) (n : Nat) : List Nat :=
  List.replicate ((n - 1) / 26 + 1) (base + (n - 1) % 26)

def parseLetters (base : Nat) : List Nat → Nat
  | [] => 0
  | c :: rest => rest.length * 26 + (c - base) + 1

theorem parseLetters_letters (base n : Nat) (h : 1 ≤ n) :
    parseLetters base (letters base n) = n := by
  unfold letters
  rw [show (n - 1) / 26 + 1 = ((n - 1) / 26) + 1 from rfl, List.replicate_succ]
  simp only [parseLetters, List.length_replicate]
  omega

def upperLetter : Nat → List Nat := letters 65
def lowerLetter : Nat → List Nat := letters 97

def ordinalSuffix (n : Nat) : List Nat :=
  if 11 ≤ n % 100 ∧ n % 100 ≤ 13 then [116, 104]
  else if n % 10 = 1 then [115, 116]
  else if n % 10 = 2 then [110, 100]
  else if n % 10 = 3 then [114, 100]
  else [116, 104]

theorem ordinalSuffix_length (n : Nat) : (ordinalSuffix n).length = 2 := by
  unfold ordinalSuffix
  split <;> (try split) <;> (try split) <;> (try split) <;> rfl

def ordinal (n : Nat) : List Nat := decimal n ++ ordinalSuffix n

def parseOrdinal (cs : List Nat) : Nat := parseDecimal (cs.take (cs.length - 2))

theorem parseOrdinal_ordinal (n : Nat) : parseOrdinal (ordinal n) = n := by
  unfold parseOrdinal ordinal
  have hlen : (decimal n ++ ordinalSuffix n).length - 2 = (decimal n).length := by
    simp [ordinalSuffix_length]
  rw [hlen, List.take_left', parseDecimal_decimal]
  rfl

def romanNumerals : List (Nat × List Nat) := [
  (1000, [77]), (900, [67, 77]), (500, [68]), (400, [67, 68]), (100, [67]), (90, [88, 67]),
  (50, [76]), (40, [88, 76]), (10, [88]), (9, [73, 88]), (5, [86]), (4, [73, 86]), (1, [73])
]

/-- Greedy expansion of `n` over the numerals, largest first. -/
def romanFrom : List (Nat × List Nat) → Nat → List Nat
  | [], _ => []
  | (value, numeral) :: rest, n =>
    (List.replicate (n / value) numeral).flatten ++ romanFrom rest (n % value)

def upperRoman (n : Nat) : List Nat := romanFrom romanNumerals n

def lowerRoman (n : Nat) : List Nat := (upperRoman n).map (· + 32)

/-- The formats with their `ST_NumberFormat` names. -/
def formats : List (String × (Nat → List Nat)) := [
  ("decimal", decimal),
  ("upperRoman", upperRoman),
  ("lowerRoman", lowerRoman),
  ("upperLetter", upperLetter),
  ("lowerLetter", lowerLetter),
  ("ordinal", ordinal)
]

theorem roman_1994 : upperRoman 1994 = [77, 67, 77, 88, 67, 73, 86] := by decide

end Ledger.Reference.NumberFormat
