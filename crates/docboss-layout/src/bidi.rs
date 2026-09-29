//! The Unicode Bidirectional Algorithm (UAX #9) over one paragraph:
//! explicit levels and directions (X1 to X10), weak types (W1 to W7),
//! paired brackets (N0), neutrals (N1, N2), implicit levels (I1, I2), and
//! the reordering of one line (L1, L2).

use crate::ucd::{Bidi, Joining, ASCII, BIDI, BRACKETS, JOINING, MIRRORS};

const MAX_DEPTH: u8 = 125;
const MAX_BRACKETS: usize = 63;

/// The bidirectional class of a character.
pub(crate) fn class(c: char) -> Bidi {
    let cp = c as u32;
    if cp < 0x80 {
        return ASCII[cp as usize];
    }
    let found = BIDI.binary_search_by(|&(start, end, _)| {
        if end < cp {
            return std::cmp::Ordering::Less;
        }
        if start > cp {
            return std::cmp::Ordering::Greater;
        }
        std::cmp::Ordering::Equal
    });
    found.map_or(Bidi::L, |i| BIDI[i].2)
}

/// The character's Bidi_Mirroring_Glyph, if it has one.
pub(crate) fn mirror(c: char) -> Option<char> {
    let cp = c as u32;
    let i = MIRRORS.binary_search_by_key(&cp, |&(a, _)| a).ok()?;
    char::from_u32(MIRRORS[i].1)
}

/// The character's joining type.
pub(crate) fn joining(c: char) -> Joining {
    let cp = c as u32;
    let found = JOINING.binary_search_by(|&(start, end, _)| {
        if end < cp {
            return std::cmp::Ordering::Less;
        }
        if start > cp {
            return std::cmp::Ordering::Greater;
        }
        std::cmp::Ordering::Equal
    });
    found.map_or(Joining::NonJoin, |i| JOINING[i].2)
}

/// The bracket paired with `c` and whether `c` opens the pair, with the
/// canonical equivalents U+2329 and U+232A folded into U+3008 and U+3009.
fn bracket(c: char) -> Option<(u32, bool)> {
    let cp = match c as u32 {
        0x2329 => 0x3008,
        0x232A => 0x3009,
        cp => cp,
    };
    let i = BRACKETS.binary_search_by_key(&cp, |&(a, _, _)| a).ok()?;
    let (_, pair, opening) = BRACKETS[i];
    let pair = match pair {
        0x2329 => 0x3008,
        0x232A => 0x3009,
        pair => pair,
    };
    Some((if opening { cp } else { pair }, opening))
}

/// Whether a class needs the algorithm: a right-to-left, Arabic number,
/// mark or explicit formatting class.
pub(crate) fn is_complex(class: Bidi) -> bool {
    !matches!(
        class,
        Bidi::L
            | Bidi::EN
            | Bidi::ES
            | Bidi::ET
            | Bidi::CS
            | Bidi::BN
            | Bidi::B
            | Bidi::S
            | Bidi::WS
            | Bidi::ON
    )
}

fn is_isolate_initiator(class: Bidi) -> bool {
    matches!(class, Bidi::LRI | Bidi::RLI | Bidi::FSI)
}

fn is_removed(class: Bidi) -> bool {
    matches!(
        class,
        Bidi::RLE | Bidi::LRE | Bidi::RLO | Bidi::LRO | Bidi::PDF | Bidi::BN
    )
}

fn is_neutral(class: Bidi) -> bool {
    matches!(
        class,
        Bidi::B | Bidi::S | Bidi::WS | Bidi::ON | Bidi::LRI | Bidi::RLI | Bidi::FSI | Bidi::PDI
    )
}

fn direction(level: u8) -> Bidi {
    if level % 2 == 1 {
        return Bidi::R;
    }
    Bidi::L
}

/// P2, P3: the level of the first strong character outside isolates in
/// `classes[from..]`, up to a paragraph separator or the PDI matching an
/// isolate the range opens.
fn first_strong(classes: &[Bidi], from: usize) -> Option<u8> {
    let mut depth = 0usize;
    for &class in &classes[from..] {
        match class {
            Bidi::L if depth == 0 => return Some(0),
            Bidi::R | Bidi::AL if depth == 0 => return Some(1),
            Bidi::LRI | Bidi::RLI | Bidi::FSI => depth += 1,
            Bidi::PDI if depth > 0 => depth -= 1,
            Bidi::PDI => return None,
            Bidi::B => return None,
            _ => {}
        }
    }
    None
}

/// The paragraph level P2 and P3 give a paragraph without a stated
/// direction.
#[cfg(test)]
fn paragraph_level(classes: &[Bidi]) -> u8 {
    first_strong(classes, 0).unwrap_or(0)
}

#[derive(Clone, Copy)]
struct Entry {
    level: u8,
    over: Option<Bidi>,
    isolate: bool,
}

/// Resolves the embedding level of every character of a paragraph at
/// `paragraph` level. `classes` are the characters' bidirectional classes,
/// any higher-level overrides already applied; `chars` are the characters,
/// for bracket pairs. Characters X9 removes take the level of the
/// character before them.
pub(crate) fn resolve(chars: &[char], classes: &[Bidi], paragraph: u8) -> Vec<u8> {
    let n = classes.len().min(chars.len());
    let mut levels = vec![paragraph; n];
    let mut types: Vec<Bidi> = classes[..n].to_vec();
    explicit(classes, paragraph, &mut levels, &mut types);
    for sequence in isolating_sequences(classes, &levels, paragraph) {
        resolve_sequence(chars, classes, &sequence, &levels, &mut types);
        for &i in &sequence.indices {
            let level = levels[i];
            levels[i] = match (level % 2 == 1, types[i]) {
                (false, Bidi::R) => level + 1,
                (false, Bidi::AN | Bidi::EN) => level + 2,
                (true, Bidi::L | Bidi::EN | Bidi::AN) => level + 1,
                _ => level,
            };
        }
    }
    let mut previous = paragraph;
    for i in 0..n {
        if is_removed(classes[i]) {
            levels[i] = previous;
            continue;
        }
        previous = levels[i];
    }
    levels
}

/// X1 to X8: explicit embeddings, overrides and isolates.
fn explicit(classes: &[Bidi], paragraph: u8, levels: &mut [u8], types: &mut [Bidi]) {
    let mut stack: Vec<Entry> = Vec::with_capacity(8);
    stack.push(Entry {
        level: paragraph,
        over: None,
        isolate: false,
    });
    let mut overflow_isolates = 0usize;
    let mut overflow_embeddings = 0usize;
    let mut valid_isolates = 0usize;
    for i in 0..levels.len() {
        let top = stack.last().copied().unwrap_or(Entry {
            level: paragraph,
            over: None,
            isolate: false,
        });
        let class = classes[i];
        match class {
            Bidi::RLE | Bidi::LRE | Bidi::RLO | Bidi::LRO => {
                levels[i] = top.level;
                let rtl = matches!(class, Bidi::RLE | Bidi::RLO);
                let level = if rtl {
                    (top.level + 1) | 1
                } else {
                    (top.level + 2) & !1
                };
                if level <= MAX_DEPTH && overflow_isolates == 0 && overflow_embeddings == 0 {
                    let over = match class {
                        Bidi::RLO => Some(Bidi::R),
                        Bidi::LRO => Some(Bidi::L),
                        _ => None,
                    };
                    stack.push(Entry {
                        level,
                        over,
                        isolate: false,
                    });
                    continue;
                }
                if overflow_isolates == 0 {
                    overflow_embeddings += 1;
                }
            }
            Bidi::RLI | Bidi::LRI | Bidi::FSI => {
                levels[i] = top.level;
                if let Some(over) = top.over {
                    types[i] = over;
                }
                let rtl = match class {
                    Bidi::RLI => true,
                    Bidi::LRI => false,
                    _ => first_strong(classes, i + 1) == Some(1),
                };
                let level = if rtl {
                    (top.level + 1) | 1
                } else {
                    (top.level + 2) & !1
                };
                if level <= MAX_DEPTH && overflow_isolates == 0 && overflow_embeddings == 0 {
                    valid_isolates += 1;
                    stack.push(Entry {
                        level,
                        over: None,
                        isolate: true,
                    });
                    continue;
                }
                overflow_isolates += 1;
            }
            Bidi::PDI => {
                if overflow_isolates > 0 {
                    overflow_isolates -= 1;
                } else if valid_isolates > 0 {
                    overflow_embeddings = 0;
                    while stack.last().is_some_and(|e| !e.isolate) && stack.len() > 1 {
                        stack.pop();
                    }
                    if stack.len() > 1 {
                        stack.pop();
                    }
                    valid_isolates -= 1;
                }
                let top = stack.last().copied().unwrap_or(top);
                levels[i] = top.level;
                if let Some(over) = top.over {
                    types[i] = over;
                }
            }
            Bidi::PDF => {
                levels[i] = top.level;
                if overflow_isolates > 0 {
                    continue;
                }
                if overflow_embeddings > 0 {
                    overflow_embeddings -= 1;
                    continue;
                }
                if !top.isolate && stack.len() >= 2 {
                    stack.pop();
                }
            }
            Bidi::B => levels[i] = paragraph,
            Bidi::BN => levels[i] = top.level,
            _ => {
                levels[i] = top.level;
                if let Some(over) = top.over {
                    types[i] = over;
                }
            }
        }
    }
}

struct Sequence {
    indices: Vec<usize>,
    sos: Bidi,
    eos: Bidi,
}

/// X10: the isolating run sequences, each with its start and end types.
fn isolating_sequences(classes: &[Bidi], levels: &[u8], paragraph: u8) -> Vec<Sequence> {
    let n = levels.len();
    let mut runs: Vec<Vec<usize>> = Vec::new();
    let mut current: Vec<usize> = Vec::new();
    let mut run_level = None;
    for i in (0..n).filter(|&i| !is_removed(classes[i])) {
        if run_level.is_some_and(|l| l != levels[i]) {
            runs.push(std::mem::take(&mut current));
        }
        run_level = Some(levels[i]);
        current.push(i);
    }
    if !current.is_empty() {
        runs.push(current);
    }
    let mut run_of = vec![usize::MAX; n];
    for (r, run) in runs.iter().enumerate() {
        run.iter().for_each(|&i| run_of[i] = r);
    }
    let matching = matching_pdis(classes);
    let matched: std::collections::HashSet<usize> = matching.values().copied().collect();
    let mut sequences = Vec::new();
    for run in &runs {
        if classes[run[0]] == Bidi::PDI && matched.contains(&run[0]) {
            continue;
        }
        let mut indices = run.clone();
        while let Some(&last) = indices.last() {
            let Some(&pdi) = matching.get(&last) else {
                break;
            };
            let Some(next) = runs.get(run_of[pdi]) else {
                break;
            };
            indices.extend_from_slice(next);
        }
        let first = indices[0];
        let last = indices[indices.len() - 1];
        let level = levels[first];
        let before = (0..first)
            .rev()
            .find(|&i| !is_removed(classes[i]))
            .map_or(paragraph, |i| levels[i]);
        let after = match is_isolate_initiator(classes[last]) {
            true => paragraph,
            false => (last + 1..n)
                .find(|&i| !is_removed(classes[i]))
                .map_or(paragraph, |i| levels[i]),
        };
        sequences.push(Sequence {
            indices,
            sos: direction(level.max(before)),
            eos: direction(levels[last].max(after)),
        });
    }
    sequences
}

/// Isolate initiators and the PDIs matching them (BD9).
fn matching_pdis(classes: &[Bidi]) -> std::collections::HashMap<usize, usize> {
    let mut out = std::collections::HashMap::new();
    let mut open: Vec<usize> = Vec::new();
    for (i, &class) in classes.iter().enumerate() {
        match class {
            Bidi::LRI | Bidi::RLI | Bidi::FSI => open.push(i),
            Bidi::PDI => {
                if let Some(start) = open.pop() {
                    out.insert(start, i);
                }
            }
            Bidi::B => open.clear(),
            _ => {}
        }
    }
    out
}

/// W1 to W7, N0 to N2 over one isolating run sequence.
fn resolve_sequence(
    chars: &[char],
    classes: &[Bidi],
    sequence: &Sequence,
    levels: &[u8],
    types: &mut [Bidi],
) {
    let idx = &sequence.indices;
    let mut t: Vec<Bidi> = idx.iter().map(|&i| types[i]).collect();
    let m = t.len();
    let level = levels[idx[0]];
    let embedding = direction(level);

    let mut previous = sequence.sos;
    for class in t.iter_mut() {
        if *class == Bidi::NSM {
            *class = match previous {
                Bidi::LRI | Bidi::RLI | Bidi::FSI | Bidi::PDI => Bidi::ON,
                p => p,
            };
        }
        previous = *class;
    }
    let mut strong = sequence.sos;
    for class in t.iter_mut() {
        match *class {
            Bidi::L | Bidi::R | Bidi::AL => strong = *class,
            Bidi::EN if strong == Bidi::AL => *class = Bidi::AN,
            _ => {}
        }
    }
    for class in t.iter_mut() {
        if *class == Bidi::AL {
            *class = Bidi::R;
        }
    }
    for k in 1..m.saturating_sub(1) {
        let (a, b) = (t[k - 1], t[k + 1]);
        t[k] = match (t[k], a, b) {
            (Bidi::ES, Bidi::EN, Bidi::EN) => Bidi::EN,
            (Bidi::CS, Bidi::EN, Bidi::EN) => Bidi::EN,
            (Bidi::CS, Bidi::AN, Bidi::AN) => Bidi::AN,
            (class, ..) => class,
        };
    }
    let mut k = 0;
    while k < m {
        if t[k] != Bidi::ET {
            k += 1;
            continue;
        }
        let start = k;
        while k < m && t[k] == Bidi::ET {
            k += 1;
        }
        let touches = (start > 0 && t[start - 1] == Bidi::EN) || (k < m && t[k] == Bidi::EN);
        if touches {
            t[start..k].iter_mut().for_each(|c| *c = Bidi::EN);
        }
    }
    for class in t.iter_mut() {
        if matches!(*class, Bidi::ES | Bidi::ET | Bidi::CS) {
            *class = Bidi::ON;
        }
    }
    let mut strong = sequence.sos;
    for class in t.iter_mut() {
        match *class {
            Bidi::L | Bidi::R => strong = *class,
            Bidi::EN if strong == Bidi::L => *class = Bidi::L,
            _ => {}
        }
    }

    brackets(chars, classes, idx, &mut t, sequence.sos, embedding);

    let strong_of = |class: Bidi| match class {
        Bidi::L => Some(Bidi::L),
        Bidi::R | Bidi::EN | Bidi::AN => Some(Bidi::R),
        _ => None,
    };
    let mut k = 0;
    while k < m {
        if !is_neutral(t[k]) {
            k += 1;
            continue;
        }
        let start = k;
        while k < m && is_neutral(t[k]) {
            k += 1;
        }
        let before = if start == 0 {
            Some(sequence.sos)
        } else {
            strong_of(t[start - 1])
        };
        let after = if k == m {
            Some(sequence.eos)
        } else {
            strong_of(t[k])
        };
        let resolved = match (before, after) {
            (Some(a), Some(b)) if a == b => a,
            _ => embedding,
        };
        t[start..k].iter_mut().for_each(|c| *c = resolved);
    }
    for (k, &i) in idx.iter().enumerate() {
        types[i] = t[k];
    }
}

/// N0: paired brackets take the direction of the strong text inside them,
/// or of the context before them when only the other direction is inside.
fn brackets(
    chars: &[char],
    classes: &[Bidi],
    idx: &[usize],
    t: &mut [Bidi],
    sos: Bidi,
    embedding: Bidi,
) {
    let mut open: Vec<(u32, usize)> = Vec::new();
    let mut pairs: Vec<(usize, usize)> = Vec::new();
    for (k, &i) in idx.iter().enumerate() {
        if t[k] != Bidi::ON {
            continue;
        }
        let Some((id, opening)) = bracket(chars[i]) else {
            continue;
        };
        if opening {
            if open.len() >= MAX_BRACKETS {
                break;
            }
            open.push((id, k));
            continue;
        }
        let Some(depth) = open.iter().rposition(|&(o, _)| o == id) else {
            continue;
        };
        pairs.push((open[depth].1, k));
        open.truncate(depth);
    }
    if pairs.is_empty() {
        return;
    }
    pairs.sort_unstable();
    let strong_of = |class: Bidi| match class {
        Bidi::L => Some(Bidi::L),
        Bidi::R | Bidi::EN | Bidi::AN => Some(Bidi::R),
        _ => None,
    };
    for (a, b) in pairs {
        let mut inside_same = false;
        let mut inside_other = false;
        for &class in &t[a + 1..b] {
            match strong_of(class) {
                Some(d) if d == embedding => inside_same = true,
                Some(_) => inside_other = true,
                None => {}
            }
        }
        let resolved = if inside_same {
            embedding
        } else if inside_other {
            let context = (0..a).rev().find_map(|k| strong_of(t[k])).unwrap_or(sos);
            if context != embedding {
                context
            } else {
                embedding
            }
        } else {
            continue;
        };
        t[a] = resolved;
        t[b] = resolved;
        for at in [a, b] {
            for k in at + 1..t.len() {
                if classes[idx[k]] != Bidi::NSM {
                    break;
                }
                t[k] = resolved;
            }
        }
    }
}

/// L1: the levels of one line with segment and paragraph separators, and
/// the whitespace before them and at the end of the line, reset to the
/// paragraph level. `classes` are the characters' original classes.
#[cfg(test)]
fn line_levels(classes: &[Bidi], levels: &mut [u8], paragraph: u8) {
    let mut trailing = true;
    for i in (0..levels.len().min(classes.len())).rev() {
        let class = classes[i];
        if matches!(class, Bidi::S | Bidi::B) {
            levels[i] = paragraph;
            trailing = true;
            continue;
        }
        let space = matches!(
            class,
            Bidi::WS | Bidi::LRI | Bidi::RLI | Bidi::FSI | Bidi::PDI
        ) || is_removed(class);
        if trailing && space {
            levels[i] = paragraph;
            continue;
        }
        trailing = false;
    }
}

/// L2: the visual order of a line's characters from their levels, as
/// indices into the line.
pub(crate) fn visual_order(levels: &[u8]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..levels.len()).collect();
    let Some(&highest) = levels.iter().max() else {
        return order;
    };
    let lowest_odd = levels
        .iter()
        .copied()
        .filter(|l| l % 2 == 1)
        .min()
        .unwrap_or(highest + 1);
    let mut level = highest;
    while level >= lowest_odd && level > 0 {
        let mut i = 0;
        while i < order.len() {
            if levels[order[i]] < level {
                i += 1;
                continue;
            }
            let start = i;
            while i < order.len() && levels[order[i]] >= level {
                i += 1;
            }
            order[start..i].reverse();
        }
        level -= 1;
    }
    order
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_hex(field: &str) -> Vec<char> {
        field
            .split_whitespace()
            .filter_map(|h| u32::from_str_radix(h, 16).ok())
            .filter_map(char::from_u32)
            .collect()
    }

    /// Runs the lines of BidiCharacterTest.txt; returns (passed, failed).
    fn run(text: &str) -> (usize, Vec<String>) {
        let mut passed = 0;
        let mut failed = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let fields: Vec<&str> = line.split(';').collect();
            if fields.len() < 5 {
                continue;
            }
            let chars = parse_hex(fields[0]);
            let classes: Vec<Bidi> = chars.iter().map(|&c| class(c)).collect();
            let paragraph = match fields[1].trim() {
                "0" => 0,
                "1" => 1,
                _ => paragraph_level(&classes),
            };
            let expected_paragraph: u8 = fields[2].trim().parse().unwrap_or(0);
            let expected: Vec<Option<u8>> = fields[3]
                .split_whitespace()
                .map(|l| l.parse().ok())
                .collect();
            let mut levels = resolve(&chars, &classes, paragraph);
            line_levels(&classes, &mut levels, paragraph);
            let levels_ok = paragraph == expected_paragraph
                && expected
                    .iter()
                    .zip(&levels)
                    .all(|(e, l)| e.is_none_or(|e| e == *l));
            let kept: Vec<usize> = (0..chars.len())
                .filter(|&i| expected.get(i).is_some_and(|e| e.is_some()))
                .collect();
            let kept_levels: Vec<u8> = kept.iter().map(|&i| levels[i]).collect();
            let order: Vec<usize> = visual_order(&kept_levels)
                .into_iter()
                .map(|k| kept[k])
                .collect();
            let expected_order: Vec<usize> = fields[4]
                .split_whitespace()
                .filter_map(|v| v.parse().ok())
                .collect();
            if levels_ok && order == expected_order {
                passed += 1;
                continue;
            }
            failed.push(format!("{line} => {levels:?} {order:?}"));
        }
        (passed, failed)
    }

    /// UAX #9 conformance on an excerpt of BidiCharacterTest.txt (Unicode
    /// 16.0.0): explicit embeddings and isolates, weak types, paired
    /// brackets, neutrals, implicit levels and line reordering.
    #[test]
    fn passes_the_bidi_character_test_excerpt() {
        let text = include_str!("../tests/fixtures/bidi-character-test.txt");
        let (passed, failed) = run(text);
        assert!(
            failed.is_empty(),
            "{} failed: {:#?}",
            failed.len(),
            &failed[..failed.len().min(10)]
        );
        assert!(passed > 100);
    }

    /// The full BidiCharacterTest.txt, when `make ucd` has fetched it.
    #[test]
    fn passes_the_full_bidi_character_test_when_present() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../ledger/specs/ucd/BidiCharacterTest.txt"
        );
        let Ok(text) = std::fs::read_to_string(path) else {
            return;
        };
        let (passed, failed) = run(&text);
        assert!(passed > 90_000, "{passed} passed");
        assert!(
            failed.is_empty(),
            "{passed} passed, {} failed: {:#?}",
            failed.len(),
            &failed[..failed.len().min(10)]
        );
    }

    #[test]
    fn classes_mirrors_and_joining() {
        assert_eq!(class('a'), Bidi::L);
        assert_eq!(class('\u{05D0}'), Bidi::R);
        assert_eq!(class('\u{0627}'), Bidi::AL);
        assert_eq!(class('\u{0661}'), Bidi::AN);
        assert_eq!(class('\u{064B}'), Bidi::NSM);
        assert_eq!(class('\u{05FF}'), Bidi::R);
        assert_eq!(mirror('('), Some(')'));
        assert_eq!(mirror('a'), None);
        assert_eq!(joining('\u{0628}'), Joining::Dual);
        assert_eq!(joining('\u{0627}'), Joining::Right);
        assert_eq!(joining('\u{064E}'), Joining::Transparent);
        assert_eq!(joining(' '), Joining::NonJoin);
    }
}
