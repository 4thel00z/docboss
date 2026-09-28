import Ledger.Feature

/-!
Ledger rows for [MS-CFB]. One row per clause the scope requires, plus any deeper row a note needs.
-/

namespace Ledger.Catalogue.MsCfb

open Ledger

def rows0 : List Feature := [
  { standard := .msCfb, ref := .clause [2, 1], title := "Compound File Sector Numbers and Types", status := .implemented,
    note := "FREESECT, ENDOFCHAIN, FATSECT and DIFSECT end or skip chains; a chain that loops or runs past the file is cut and reported (tests cfb.rs walks_the_directory_of_a_word_file and fat_loops_are_cut)." },
  { standard := .msCfb, ref := .clause [2, 2], title := "Compound File Header", status := .implemented,
    note := "Signature, version, sector shifts, the FAT, directory, mini FAT and DIFAT locations and the first 109 DIFAT entries are read for v3 and v4 files (lib.rs parse, test cfb.rs walks_the_directory_of_a_word_file)." },
  { standard := .msCfb, ref := .clause [2, 3], title := "Compound File FAT Sectors", status := .implemented,
    note := "FAT sectors are read and chains followed with loop detection (tests cfb.rs walks_the_directory_of_a_word_file and fat_loops_are_cut)." },
  { standard := .msCfb, ref := .clause [2, 4], title := "Compound File Mini FAT Sectors", status := .implemented,
    note := "The mini FAT and the mini stream held by the root entry serve streams below the cutoff (test cfb.rs walks_the_directory_of_a_word_file)." },
  { standard := .msCfb, ref := .clause [2, 5], title := "Compound File DIFAT Sectors", status := .implemented,
    note := "DIFAT sectors after the header's 109 entries are followed to find every FAT sector (lib.rs parse, test cfb.rs walks_the_directory_of_a_word_file)." },
  { standard := .msCfb, ref := .clause [2, 6], title := "Compound File Directory Sectors", status := .implemented,
    note := "Directory sectors are read into entries with names, types, sizes and sibling and child links (test cfb.rs walks_the_directory_of_a_word_file)." },
  { standard := .msCfb, ref := .clause [2, 6, 1], title := "Compound File Directory Entry", status := .implemented,
    note := "Directory entries are read with their name, object type, links, start sector and size (directory.rs, test cfb.rs walks_the_directory_of_a_word_file)." },
  { standard := .msCfb, ref := .clause [2, 6, 2], title := "Root Directory Entry", status := .implemented,
    note := "The root entry's start sector and size locate the mini stream (test cfb.rs walks_the_directory_of_a_word_file)." },
  { standard := .msCfb, ref := .clause [2, 6, 3], title := "Other Directory Entries", status := .implemented,
    note := "Storage and stream entries are read and opened by path (test cfb.rs walks_the_directory_of_a_word_file)." },
  { standard := .msCfb, ref := .clause [2, 6, 4], title := "Red-Black Tree", status := .implemented,
    note := "Children are walked through the sibling tree in order and names compare case-insensitively (lib.rs children, test cfb.rs lookup_ignores_case)." },
  { standard := .msCfb, ref := .clause [2, 7], title := "Compound File User-Defined Data Sectors", status := .outOfScope,
    note := "User-defined data sectors are ordinary sectors of their stream's chain; nothing more to read." },
  { standard := .msCfb, ref := .clause [2, 8], title := "Compound File Range Lock Sector", status := .outOfScope,
    note := "The range lock sector is never read." },
  { standard := .msCfb, ref := .clause [2, 9], title := "Compound File Size Limits", status := .notImplemented,
    note := "The size limits are not enforced; reads are bounded by the file's length." }
]

/-- Every row of the [MS-CFB] ledger. -/
def rows : List Feature :=
  rows0

end Ledger.Catalogue.MsCfb
