import Ledger.Feature

/-!
`lake exe ledger-index <repo-root>` scans the Rust and Python sources for
citations of the tracked specifications and writes them to
`Ledger/Generated.lean`.

A citation is a standard's name followed by a clause: `ECMA-376 Part 1
§17.3.1.29`, `ECMA-376 Part 2 §9.1`, `[MS-DOC] §2.5.1`, `[MS-CFB] §2.6.1`,
`[MS-OLEPS] §2.3`, `[MS-OSHARED] §2.3.1`, `[MS-ODRAW] §2.2.1` or
`APPNOTE §4.3.7`. The section sign may be replaced by `clause` or `section`,
an annex clause is written `Annex A.1`, and further clauses of the same
standard may follow after a comma, a semicolon or `and`. When the name ends
the line, the clause is looked for at the start of the next line. A name
followed by anything else is recorded with no reference so the report can
list it; `ECMA-376` without a tracked part is not a citation.

A line is in test context when its file sits under a `tests/` directory or
follows a `#[cfg(test)]` attribute that opens a module.
-/

open Ledger System

namespace Index

def scannedRoots : List String := ["crates", "python", "tests"]

def skippedDirs : List String := ["target", "benches", "__pycache__", ".venv", "fixtures", "vectors"]

def isSourceFile (path : FilePath) : Bool :=
  match path.extension with
  | some "rs" => true
  | some "py" => true
  | _ => false

def dropWord (s : String) (word : String) : Option String :=
  if s.startsWith word then some (s.drop word.length).toString else none

def separatorChars : List Char := [' ', '`', ',', ':', '(', '§', '\t']

/-- Skips the punctuation and filler words allowed between a standard's name
and the reference proper. -/
partial def skipSeparators (s : String) : String :=
  let trimmed := (s.dropWhile (fun c => separatorChars.contains c)).toString
  let stripped := ["clause", "Clause", "Section", "section", "sub-clause", "subclause"].foldl
    (fun acc word => match dropWord acc word with
      | some rest => rest
      | none => acc) trimmed
  if stripped == s then s else skipSeparators stripped

/-- Parses `17.3.1` or `17.3.1.` (trailing period tolerated) into segments. -/
def parseDotted (s : String) : Option (List Nat × String) :=
  let token := (s.takeWhile (fun c => c.isDigit || c == '.')).toString
  let rest := (s.drop token.length).toString
  let cleaned := (token.dropEndWhile (fun c => c == '.')).toString
  if cleaned.isEmpty then none
  else
    let parts := cleaned.splitOn "."
    match parts.mapM String.toNat? with
    | some segments => if segments.isEmpty then none else some (segments, rest)
    | none => none

def parseAnnex (s : String) : Option Ref :=
  match s.toList with
  | letter :: '.' :: rest =>
    if !letter.isUpper then none
    else if rest.head?.any Char.isDigit then
      (parseDotted (String.ofList rest)).map fun (path, _) => .annex letter path
    else some (.annex letter [])
  | letter :: rest =>
    if letter.isUpper && (rest.isEmpty || rest.head?.any (fun c => !c.isAlphanum)) then
      some (.annex letter [])
    else none
  | [] => none

/-- The reference named by the text following a standard's name, if any. -/
def parseReference (s : String) : Option Ref :=
  let s := skipSeparators s
  match dropWord s "Annex " with
  | some rest => parseAnnex rest
  | none =>
    match s.front.isDigit, parseDotted s with
    | true, some (path, _) =>
      match path with
      | chapter :: _ => if 1 ≤ chapter && chapter ≤ 99 then some (.clause path) else none
      | [] => none
    | _, _ => none

/-- The text of a comment line without its leader (`///`, `//!`, `//`, `#`,
`*`), so a reference wrapped onto the next line can be read. -/
def commentBody (line : String) : String :=
  let trimmed := line.trimAscii.toString
  let leaders := ["///", "//!", "//", "#", "*"]
  match leaders.find? (fun l => trimmed.startsWith l) with
  | some leader => (trimmed.drop leader.length).trimAscii.toString
  | none => trimmed

/-- The text after the first reference in `s`, when it introduces another
reference of the same standard: `, §17.3.2`, `; Annex A.1`, ` and §2.4`. -/
def continuation (s : String) : Option String :=
  let rest := (s.dropWhile (fun c => c == ' ' || c == ',' || c == ';')).toString
  let rest := match dropWord rest "and " with
    | some r => r
    | none => rest
  if rest.startsWith "§" || rest.startsWith "Annex " then some rest
  else
    match parseDotted rest with
    | some (_ :: _ :: _, _) => some rest
    | _ => none

/-- Every reference introduced by one mention: the first one and the ones
that follow it. -/
partial def referencesIn (s : String) : List Ref :=
  match parseReference s with
  | none => []
  | some r =>
    let afterRef := skipSeparators s
    let afterRef := (dropWord afterRef "Annex ").getD afterRef
    let afterRef := (afterRef.dropWhile (fun c => c.isDigit || c == '.' || c.isUpper)).toString
    match continuation afterRef with
    | some more => r :: referencesIn more
    | none => [r]

/-- The names a citation can start with, each with the standard it names.
`ECMA-376` is resolved by the part that follows it. -/
def bracketed : List (String × Standard) := [
  ("[MS-DOC]", .msDoc),
  ("[MS-CFB]", .msCfb),
  ("[MS-OLEPS]", .msOleps),
  ("[MS-OSHARED]", .msOshared),
  ("[MS-ODRAW]", .msOdraw)
]

/-- The ECMA-376 part named at the start of `s` (` Part 1 …`), and the text
after it. -/
def ecmaPart (s : String) : Option (Standard × String) :=
  let s := (s.dropWhile (· == ' ')).toString
  match dropWord s "Part " with
  | none => none
  | some rest =>
    let digits := (rest.takeWhile Char.isDigit).toString
    let after := (rest.drop digits.length).toString
    match digits.toNat? with
    | some 1 => some (.ecma376Part1, after)
    | some 2 => some (.ecma376Part2, after)
    | some 3 => some (.ecma376Part3, after)
    | _ => none

/-- One mention of a standard's name: the standard, the text after the name,
and whether that text already comes from the next line. -/
structure Mention where
  standard : Standard
  rest : String
  wrapped : Bool

/-- Every mention of a standard's name on a line. -/
def mentions (line : String) (nextLine : String) : List Mention :=
  let ecma := match line.splitOn "ECMA-376" with
    | [] => []
    | _ :: rests => rests.filterMap fun rest =>
      match ecmaPart rest with
      | some (standard, after) => some { standard, rest := after, wrapped := false }
      | none =>
        if !rest.trimAscii.toString.isEmpty then none
        else (ecmaPart (commentBody nextLine)).map fun (standard, after) =>
          { standard, rest := after, wrapped := true }
  let named := bracketed.flatMap fun (name, standard) =>
    match line.splitOn name with
    | [] => []
    | _ :: rests => rests.map fun rest => { standard, rest, wrapped := false }
  let appnote := match line.splitOn "APPNOTE" with
    | [] => []
    | _ :: rests => rests.map fun rest =>
      { standard := Standard.appnote, rest := (dropWord rest ".TXT").getD rest, wrapped := false }
  ecma ++ named ++ appnote

/-- Every citation on one line, one per reference. When a mention ends the
line, the reference is looked for at the start of the next line. -/
def citationsOnLine (file : String) (lineNumber : Nat) (inTest : Bool) (line : String)
    (nextLine : String) : List Citation :=
  (mentions line nextLine).flatMap fun m =>
    let refs := match referencesIn m.rest with
      | [] =>
        if (skipSeparators m.rest).isEmpty && !m.wrapped then referencesIn (commentBody nextLine)
        else []
      | found => found
    match refs with
    | [] => [{ standard := m.standard, ref := none, file, line := lineNumber, inTest }]
    | found => found.map fun r =>
      { standard := m.standard, ref := some r, file, line := lineNumber, inTest }

def isTestPath (relative : String) : Bool :=
  relative.startsWith "tests/" || (relative.splitOn "/").contains "tests"

/-- Test context begins at a `#[cfg(test)]` attribute whose next non-blank
line opens a module. -/
def opensTestModule (lines : Array String) (i : Nat) : Bool :=
  lines[i]!.trimAscii.toString == "#[cfg(test)]" &&
    (nextNonBlank (i + 1)).any fun l =>
      let l := l.trimAscii.toString
      ["mod ", "pub mod ", "pub(crate) mod ", "pub(super) mod "].any (l.startsWith ·)
where
  nextNonBlank (j : Nat) : Option String :=
    (lines.toList.drop j).find? (fun l => !l.trimAscii.toString.isEmpty)

def scanFile (relative : String) (contents : String) : List Citation := Id.run do
  let lines := (contents.splitOn "\n").toArray
  let mut inTest := isTestPath relative
  let mut found : Array Citation := #[]
  for i in [0:lines.size] do
    if !inTest && opensTestModule lines i then
      inTest := true
    let nextLine := lines[i + 1]?.getD ""
    found := found ++ (citationsOnLine relative (i + 1) inTest lines[i]! nextLine).toArray
  return found.toList

def relativeTo (root : FilePath) (path : FilePath) : String :=
  let rootText := root.toString
  let full := path.toString
  if full.startsWith (rootText ++ "/") then (full.drop (rootText.length + 1)).toString else full

def collectFiles (root : FilePath) : IO (Array FilePath) := do
  let mut files : Array FilePath := #[]
  for sub in scannedRoots do
    let dir := root / sub
    if !(← dir.pathExists) then continue
    let walked ← dir.walkDir fun p => pure !(skippedDirs.contains (p.fileName.getD ""))
    files := files ++ walked.filter isSourceFile
  return files

def renderPath (path : List Nat) : String :=
  "[" ++ ", ".intercalate (path.map toString) ++ "]"

def renderRef : Ref → String
  | .clause path => s!".clause {renderPath path}"
  | .annex letter path => s!".annex '{letter}' {renderPath path}"

def renderRef? : Option Ref → String
  | some r => s!"some ({renderRef r})"
  | none => "none"

def renderCitation (c : Citation) : String :=
  s!"  ⟨{c.standard.constructor}, {renderRef? c.ref}, {c.file.quote}, {c.line}, {c.inTest}⟩"

/-- A total order on citations: by file, then line, then the rendered row, so
the index is reproducible across machines. -/
def orderCitations (a b : Citation) : Bool :=
  a.file < b.file || (a.file == b.file && (a.line < b.line ||
    (a.line == b.line && renderCitation a < renderCitation b)))

/-- Rows per generated list literal: Lean elaborates a list literal one
element per recursion step, so the index is written as literals of this size
joined with `++`. -/
def chunkRows : Nat := 200

partial def chunks (xs : List Citation) : List (List Citation) :=
  if xs.isEmpty then [] else xs.take chunkRows :: chunks (xs.drop chunkRows)

def renderChunk (index : Nat) (citations : List Citation) : String :=
  let rows := ",\n".intercalate (citations.map renderCitation)
  s!"def Generated.citations{index} : List Citation := [\n" ++ rows ++ "\n]\n\n"

def renderModule (citations : Array Citation) : String :=
  let parts := chunks citations.toList
  let indices := List.range parts.length
  let defs := String.join ((indices.zip parts).map fun (i, part) => renderChunk i part)
  let joined := if parts.isEmpty then "[]" else
    " ++ ".intercalate (indices.map fun i => s!"Generated.citations{i}")
  "import Ledger.Feature\n\n" ++
  "/-!\nCitations of the tracked specifications in the docboss source tree, written by\n" ++
  "`lake exe ledger-index`. Regenerate rather than edit.\n-/\n\n" ++
  "namespace Ledger\n\n" ++ defs ++
  "def Generated.citations : List Citation :=\n  " ++ joined ++ "\n\n" ++
  "end Ledger\n"

end Index

open Index in
def main (args : List String) : IO UInt32 := do
  let root : FilePath := (args.head?.getD ".." : String)
  let output : FilePath := ((args[1]?).getD "Ledger/Generated.lean" : String)
  let files ← collectFiles root
  let mut citations : Array Citation := #[]
  for file in files.qsort (fun a b => a.toString < b.toString) do
    let contents ← IO.FS.readFile file
    citations := citations ++ (scanFile (relativeTo root file) contents).toArray
  let sorted := citations.qsort orderCitations
  IO.FS.writeFile output (renderModule sorted)
  let resolved := sorted.filter (·.ref.isSome)
  IO.eprintln (s!"{files.size} files, {sorted.size} mentions, {resolved.size} resolved, " ++
    s!"{(resolved.filter (·.inTest)).size} in tests")
  return 0
