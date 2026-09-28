import Ledger.Reference

/-!
`lake exe ledger-vectors <repo-root>` writes the differential test vectors of
the reference implementations in `Ledger.Reference`, whose theorems the
kernel has checked:

- `crates/docboss-zip/tests/vectors/ledger/crc32.tsv`, the CRC-32 of
  APPNOTE §4.4.7: `name<TAB>input-hex<TAB>crc-hex`, the checksum as eight
  lower-case hexadecimal digits.
- `crates/docboss-model/tests/vectors/ledger/number_format.tsv`, the list
  labels of ECMA-376 Part 1 §17.18.59: `format<TAB>number<TAB>label`.

Lines starting with `#` are comments.
-/

open Ledger.Reference System

namespace Vectors

def hexDigits : Array Char := "0123456789abcdef".toList.toArray

def hexOf (bs : List Nat) : String :=
  String.ofList (bs.flatMap fun b => [hexDigits[b / 16 % 16]!, hexDigits[b % 16]!])

def hex32 (n : Nat) : String :=
  hexOf [n / 16777216 % 256, n / 65536 % 256, n / 256 % 256, n % 256]

def bytesOf (s : String) : List Nat := s.toList.map (·.toNat)

/-- A fixed linear congruential sequence, so the vectors are reproducible. -/
def randomBytes (seed length : Nat) : List Nat :=
  let step (state : Nat) : Nat := (state * 1103515245 + 12345) % 2147483648
  (List.range length).foldl (fun (acc, state) _ =>
    let next := step state
    (acc ++ [next / 65536 % 256], next)) ([], seed) |>.1

def lengths : List Nat := (List.range 21) ++ [31, 32, 33, 63, 64, 65, 127, 128, 129, 255, 256, 257, 1000, 4096]

def crcCases : List (String × List Nat) :=
  [ ("check-123456789", bytesOf "123456789"),
    ("ascii-docboss", bytesOf "docboss"),
    ("zeros-32", List.replicate 32 0),
    ("ones-32", List.replicate 32 255),
    ("all-bytes", List.range 256) ] ++
  lengths.map fun n => (s!"random-{n}", randomBytes (n + 7) n)

def crcLines : List String :=
  crcCases.map fun (name, bytes) => s!"{name}\t{hexOf bytes}\t{hex32 (Crc32.crc32 bytes)}"

def numbers : List Nat :=
  (List.range 130).map (· + 1) ++
    [199, 200, 211, 212, 213, 221, 222, 223, 399, 400, 444, 499, 500, 675, 676, 677, 702, 703,
      888, 999, 1000, 1001, 1111, 1994, 2024, 2999, 3000, 3888, 3999]

def labelOf (codes : List Nat) : String := String.ofList (codes.map Char.ofNat)

def numberLines : List String :=
  NumberFormat.formats.flatMap fun (name, format) =>
    numbers.map fun n => s!"{name}\t{n}\t{labelOf (format n)}"

def header (lines : List String) : String :=
  "\n".intercalate (lines.map (s!"# {·}")) ++ "\n"

def write (path : FilePath) (comment : List String) (lines : List String) : IO Unit := do
  if let some parent := path.parent then IO.FS.createDirAll parent
  IO.FS.writeFile path (header comment ++ "\n".intercalate lines ++ "\n")

end Vectors

open Vectors in
def main (args : List String) : IO UInt32 := do
  let root : FilePath := (args.head?.getD ".." : String)
  write (root / "crates/docboss-zip/tests/vectors/ledger/crc32.tsv")
    ["CRC-32 (APPNOTE §4.4.7) vectors written by `lake exe ledger-vectors` from",
     "ledger/Ledger/Reference/Crc32.lean. Regenerate rather than edit.",
     "name<TAB>input-hex<TAB>crc-hex"] crcLines
  write (root / "crates/docboss-model/tests/vectors/ledger/number_format.tsv")
    ["ST_NumberFormat (ECMA-376 Part 1 §17.18.59) label vectors written by `lake exe ledger-vectors`",
     "from ledger/Ledger/Reference/NumberFormat.lean. Regenerate rather than edit.",
     "format<TAB>number<TAB>label"] numberLines
  IO.eprintln s!"{crcLines.length} CRC-32 vectors, {numberLines.length} number format vectors"
  return 0
