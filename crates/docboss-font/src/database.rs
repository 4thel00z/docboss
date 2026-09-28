//! Font discovery and matching: system font directories, fonts embedded
//! in a document, family and style matching with metric-compatible
//! substitutes, and per-character fallback.

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use docboss_model::FontEntry;

use crate::bytes::{u16_at, u32_at};
use crate::font::{face_count, read_names, read_style, FaceStyle, Font};

/// Index of a face in a [`FontDatabase`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FontId(pub u32);

#[derive(Debug, Clone)]
enum Source {
    File(PathBuf),
    Data(Arc<[u8]>),
}

/// What the database knows about a face before loading it.
#[derive(Debug, Clone)]
pub struct FaceInfo {
    source: Source,
    index: u32,
    /// The family names the face answers to, normalized.
    families: Vec<String>,
    /// The face's own family name as the font states it.
    pub family: String,
    pub style: FaceStyle,
    /// Whether the face came from the document rather than the system.
    pub embedded: bool,
}

/// A set of font faces, loaded lazily and cached.
#[derive(Debug, Default)]
pub struct FontDatabase {
    faces: Vec<FaceInfo>,
    loaded: Mutex<HashMap<u32, Option<Arc<Font>>>>,
    matches: Mutex<HashMap<(String, bool, bool), Option<FontId>>>,
    fallbacks: Mutex<HashMap<(char, bool, bool), Option<FontId>>>,
}

const MAX_DEPTH: usize = 6;
const MAX_FALLBACK_PROBES: usize = 400;

/// Faces with broad character coverage, tried first for fallback.
const COVERAGE_FAMILIES: &[&str] = &[
    "arialunicodems",
    "notosans",
    "dejavusans",
    "segoeui",
    "segoeuisymbol",
    "applesymbols",
    "pingfangsc",
    "hiraginosans",
    "hiraginokakugothicpron",
    "notosanscjksc",
    "notosanscjkjp",
    "msgothic",
    "simsun",
    "stheitimedium",
    "applegothic",
    "geezapro",
    "kohinoordevanagari",
    "thonburi",
    "lucidagrande",
];

fn normalize(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Metric-compatible and look-alike substitutes for common document fonts.
fn substitutes(family: &str) -> &'static [&'static str] {
    match family {
        "calibri" | "calibrilight" => &["carlito", "calibri"],
        "cambria" | "cambriamath" => &["caladea", "cambria"],
        "arial" | "helvetica" | "helveticaneue" | "arialmt" => &[
            "arial",
            "liberationsans",
            "arimo",
            "helvetica",
            "nimbussans",
            "freesans",
        ],
        "timesnewroman" | "times" | "timesnewromanpsmt" => &[
            "timesnewroman",
            "liberationserif",
            "tinos",
            "times",
            "nimbusroman",
            "freeserif",
        ],
        "couriernew" | "courier" => &[
            "couriernew",
            "liberationmono",
            "cousine",
            "courier",
            "nimbusmono",
            "freemono",
        ],
        "georgia" => &["georgia", "gelasio"],
        "verdana" | "tahoma" => &["verdana", "tahoma", "dejavusans"],
        "consolas" | "lucidaconsole" => &[
            "consolas",
            "menlo",
            "dejavusansmono",
            "liberationmono",
            "cousine",
        ],
        "symbol" => &["symbol", "opensymbol", "standardsymbolsps"],
        "segoeui" | "aptos" | "aptosdisplay" | "calibribody" => {
            &["segoeui", "carlito", "arial", "liberationsans", "arimo"]
        }
        "msmincho" | "mincho" | "simsun" | "mingliu" => &[
            "msmincho",
            "hiraginominchopron",
            "notoserifcjkjp",
            "songtisc",
        ],
        "msgothic" | "yugothic" | "meiryo" | "simhei" | "microsoftyahei" => &[
            "msgothic",
            "hiraginosans",
            "hiraginokakugothicpron",
            "notosanscjkjp",
            "pingfangsc",
        ],
        _ => &[],
    }
}

/// Generic families for a font table's `w:family` value.
fn generic(family_class: Option<&str>, requested: &str) -> &'static [&'static str] {
    let serif: &[&str] = &[
        "timesnewroman",
        "liberationserif",
        "tinos",
        "times",
        "dejavuserif",
        "notoserif",
        "georgia",
    ];
    let sans: &[&str] = &[
        "arial",
        "liberationsans",
        "arimo",
        "helvetica",
        "dejavusans",
        "notosans",
        "roboto",
        "carlito",
    ];
    let mono: &[&str] = &[
        "couriernew",
        "liberationmono",
        "cousine",
        "courier",
        "menlo",
        "dejavusansmono",
    ];
    let class = family_class.map(str::to_ascii_lowercase);
    match class.as_deref() {
        Some("roman") => serif,
        Some("modern") => mono,
        Some("swiss") | Some("script") | Some("decorative") => sans,
        _ if requested.contains("mono")
            || requested.contains("courier")
            || requested.contains("code") =>
        {
            mono
        }
        _ if requested.contains("serif") && !requested.contains("sans") => serif,
        _ if requested.contains("times")
            || requested.contains("roman")
            || requested.contains("garamond") =>
        {
            serif
        }
        _ => sans,
    }
}

impl FontDatabase {
    pub fn new() -> Self {
        Self::default()
    }

    /// A database of the system's fonts, plus any directory listed in
    /// `DOCBOSS_FONT_PATH`. The directory scan runs once per process.
    pub fn system() -> Self {
        static SCAN: OnceLock<Vec<FaceInfo>> = OnceLock::new();
        let faces = SCAN.get_or_init(|| {
            let mut db = FontDatabase::new();
            system_font_dirs().iter().for_each(|dir| db.add_dir(dir));
            db.faces
        });
        FontDatabase {
            faces: faces.clone(),
            ..FontDatabase::default()
        }
    }

    pub fn len(&self) -> usize {
        self.faces.len()
    }

    pub fn is_empty(&self) -> bool {
        self.faces.is_empty()
    }

    pub fn faces(&self) -> &[FaceInfo] {
        &self.faces
    }

    /// Adds every font file under a directory, recursively.
    pub fn add_dir(&mut self, dir: &Path) {
        self.walk(dir, 0);
    }

    fn walk(&mut self, dir: &Path, depth: usize) {
        if depth > MAX_DEPTH {
            return;
        }
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        let mut paths: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
        paths.sort();
        for path in paths {
            if path.is_dir() {
                self.walk(&path, depth + 1);
                continue;
            }
            let extension = path
                .extension()
                .and_then(|e| e.to_str())
                .map(str::to_ascii_lowercase);
            if matches!(extension.as_deref(), Some("ttf" | "otf" | "ttc" | "otc")) {
                self.add_file(&path);
            }
        }
    }

    /// Adds every face of one font file; returns how many were added.
    pub fn add_file(&mut self, path: &Path) -> usize {
        let Ok(scanned) = scan_file(path) else {
            return 0;
        };
        let count = scanned.len();
        for (index, families, family, style) in scanned {
            self.push(FaceInfo {
                source: Source::File(path.to_path_buf()),
                index,
                families,
                family,
                style,
                embedded: false,
            });
        }
        count
    }

    /// Adds every face of a font held in memory.
    pub fn add_data(&mut self, data: Arc<[u8]>) -> Vec<FontId> {
        let mut ids = Vec::new();
        for index in 0..face_count(&data) {
            let Ok(font) = Font::parse(data.clone(), index) else {
                continue;
            };
            let names = font.names();
            let families = family_keys(
                &names.family,
                names.typographic_family.as_deref(),
                names.full_name.as_deref(),
            );
            ids.push(self.push(FaceInfo {
                source: Source::Data(data.clone()),
                index,
                families,
                family: names.family.clone(),
                style: font.style(),
                embedded: false,
            }));
        }
        ids
    }

    /// Adds the fonts a document embeds, answering to the font table's
    /// names with the style of the slot each program was embedded in.
    pub fn add_document_fonts(&mut self, fonts: &[FontEntry]) {
        for entry in fonts {
            let slots = [
                (&entry.embedded_regular, false, false),
                (&entry.embedded_bold, true, false),
                (&entry.embedded_italic, false, true),
                (&entry.embedded_bold_italic, true, true),
            ];
            for (data, bold, italic) in slots {
                let Some(data) = data else { continue };
                let Ok(font) = Font::parse(data.clone(), 0) else {
                    continue;
                };
                let mut families = vec![normalize(&entry.name)];
                families.extend(entry.alt_name.as_deref().map(normalize));
                let style = FaceStyle {
                    weight: if bold { 700 } else { 400 },
                    italic,
                    monospace: font.style().monospace,
                };
                let id = self.push(FaceInfo {
                    source: Source::Data(data.clone()),
                    index: 0,
                    families,
                    family: entry.name.clone(),
                    style,
                    embedded: true,
                });
                self.loaded
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .insert(id.0, Some(Arc::new(font)));
            }
        }
    }

    fn push(&mut self, info: FaceInfo) -> FontId {
        self.faces.push(info);
        self.matches
            .get_mut()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
        self.fallbacks
            .get_mut()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
        FontId(self.faces.len() as u32 - 1)
    }

    pub fn info(&self, id: FontId) -> Option<&FaceInfo> {
        self.faces.get(id.0 as usize)
    }

    /// Loads a face, parsing it on first use.
    pub fn font(&self, id: FontId) -> Option<Arc<Font>> {
        let info = self.faces.get(id.0 as usize)?;
        let mut loaded = self.loaded.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(found) = loaded.get(&id.0) {
            return found.clone();
        }
        let data: Option<Arc<[u8]>> = match &info.source {
            Source::Data(data) => Some(data.clone()),
            Source::File(path) => std::fs::read(path).ok().map(Arc::from),
        };
        let font = data
            .and_then(|d| Font::parse(d, info.index).ok())
            .map(Arc::new);
        loaded.insert(id.0, font.clone());
        font
    }

    fn best_in(&self, family: &str, bold: bool, italic: bool) -> Option<FontId> {
        let key = normalize(family);
        self.faces
            .iter()
            .enumerate()
            .filter(|(_, face)| face.families.contains(&key))
            .min_by_key(|(_, face)| {
                let weight_gap =
                    (i32::from(face.style.weight) - if bold { 700 } else { 400 }).unsigned_abs();
                (
                    !face.embedded,
                    face.style.italic != italic,
                    face.style.is_bold() != bold,
                    weight_gap,
                )
            })
            .map(|(i, _)| FontId(i as u32))
    }

    /// The face for a document font request: the family itself, then its
    /// metric-compatible substitutes, then the generic family of its font
    /// table class, then any face.
    pub fn select(
        &self,
        family: &str,
        family_class: Option<&str>,
        bold: bool,
        italic: bool,
    ) -> Option<FontId> {
        let key = (
            format!("{}\u{0}{}", normalize(family), family_class.unwrap_or("")),
            bold,
            italic,
        );
        if let Some(found) = self
            .matches
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&key)
        {
            return *found;
        }
        let requested = normalize(family);
        let candidates = std::iter::once(requested.as_str())
            .chain(substitutes(&requested).iter().copied())
            .chain(generic(family_class, &requested).iter().copied());
        let found = candidates
            .filter_map(|name| self.best_in(name, bold, italic))
            .find(|&id| self.font(id).is_some())
            .or_else(|| self.any_face(bold, italic));
        self.matches
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(key, found);
        found
    }

    fn any_face(&self, bold: bool, italic: bool) -> Option<FontId> {
        let mut order: Vec<usize> = (0..self.faces.len()).collect();
        order.sort_by_key(|&i| {
            let s = self.faces[i].style;
            (s.italic != italic, s.is_bold() != bold, s.monospace)
        });
        order
            .into_iter()
            .map(|i| FontId(i as u32))
            .find(|&id| self.font(id).is_some_and(|f| f.glyph_index('a').is_some()))
    }

    /// A face that has a glyph for `c`, for text the selected face lacks.
    pub fn fallback(&self, c: char, bold: bool, italic: bool) -> Option<FontId> {
        let key = (c, bold, italic);
        if let Some(found) = self
            .fallbacks
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&key)
        {
            return *found;
        }
        let preferred = COVERAGE_FAMILIES
            .iter()
            .filter_map(|name| self.best_in(name, bold, italic));
        let rest = (0..self.faces.len()).map(|i| FontId(i as u32));
        let found = preferred.chain(rest).take(MAX_FALLBACK_PROBES).find(|&id| {
            self.font(id)
                .is_some_and(|font| font.glyph_index(c).is_some())
        });
        self.fallbacks
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(key, found);
        found
    }
}

fn family_keys(family: &str, typographic: Option<&str>, full: Option<&str>) -> Vec<String> {
    let mut keys = vec![normalize(family)];
    keys.extend(typographic.map(normalize));
    keys.extend(full.map(normalize));
    keys.retain(|k| !k.is_empty());
    keys.dedup();
    keys
}

fn system_font_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::var_os("DOCBOSS_FONT_PATH")
        .map(|paths| std::env::split_paths(&paths).collect())
        .unwrap_or_default();
    let home = std::env::var_os("HOME").map(PathBuf::from);
    if cfg!(target_os = "macos") {
        dirs.extend(["/System/Library/Fonts", "/Library/Fonts"].map(PathBuf::from));
        dirs.extend(home.iter().map(|h| h.join("Library/Fonts")));
    }
    if cfg!(all(unix, not(target_os = "macos"))) {
        dirs.extend(["/usr/share/fonts", "/usr/local/share/fonts"].map(PathBuf::from));
        dirs.extend(
            home.iter()
                .flat_map(|h| [h.join(".local/share/fonts"), h.join(".fonts")]),
        );
    }
    if cfg!(windows) {
        let root =
            std::env::var_os("WINDIR").map_or_else(|| PathBuf::from("C:\\Windows"), PathBuf::from);
        dirs.push(root.join("Fonts"));
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            dirs.push(PathBuf::from(local).join("Microsoft\\Windows\\Fonts"));
        }
    }
    dirs
}

type Scanned = (u32, Vec<String>, String, FaceStyle);

fn read_range(file: &mut File, offset: u64, length: usize) -> std::io::Result<Vec<u8>> {
    file.seek(SeekFrom::Start(offset))?;
    let mut buffer = vec![0u8; length];
    file.read_exact(&mut buffer)?;
    Ok(buffer)
}

/// Reads only the table directory and the name, OS/2, head and post tables
/// of each face, so scanning hundreds of system fonts stays cheap.
fn scan_file(path: &Path) -> std::io::Result<Vec<Scanned>> {
    let mut file = File::open(path)?;
    let size = file.metadata()?.len();
    let header = read_range(&mut file, 0, 12.min(size as usize))?;
    let mut offsets = vec![0u64];
    if header.starts_with(b"ttcf") {
        let count = u32_at(&header, 8).unwrap_or(0).min(256) as usize;
        let table = read_range(&mut file, 12, count * 4)?;
        offsets = (0..count)
            .filter_map(|i| u32_at(&table, i * 4))
            .map(u64::from)
            .collect();
    }
    let invalid = || std::io::Error::new(std::io::ErrorKind::InvalidData, "font");
    let mut out = Vec::new();
    for (index, offset) in offsets.into_iter().enumerate() {
        let head = read_range(&mut file, offset, 12)?;
        let count = u16_at(&head, 4).ok_or_else(invalid)? as usize;
        let directory = read_range(&mut file, offset + 12, count.min(512) * 16)?;
        let mut wanted: HashMap<[u8; 4], Vec<u8>> = HashMap::new();
        for i in 0..count.min(512) {
            let rec = i * 16;
            let Some(tag) = directory.get(rec..rec + 4) else {
                break;
            };
            if !matches!(
                tag,
                b"name" | b"OS/2" | b"head" | b"post" | b"glyf" | b"CFF "
            ) {
                continue;
            }
            let (Some(o), Some(l)) = (u32_at(&directory, rec + 8), u32_at(&directory, rec + 12))
            else {
                break;
            };
            let tag: [u8; 4] = [tag[0], tag[1], tag[2], tag[3]];
            if matches!(&tag, b"glyf" | b"CFF ") {
                wanted.insert(tag, Vec::new());
                continue;
            }
            if u64::from(o) + u64::from(l) > size || l > 4 << 20 {
                continue;
            }
            wanted.insert(tag, read_range(&mut file, u64::from(o), l as usize)?);
        }
        if !wanted.contains_key(b"glyf") && !wanted.contains_key(b"CFF ") {
            continue;
        }
        let Some(name) = wanted.get(b"name") else {
            continue;
        };
        let names = read_names(name);
        if names.family.is_empty() {
            continue;
        }
        let style = read_style(
            wanted.get(b"OS/2").map(Vec::as_slice),
            wanted.get(b"head").map(Vec::as_slice),
            wanted.get(b"post").map(Vec::as_slice),
            &names.subfamily,
        );
        let families = family_keys(
            &names.family,
            names.typographic_family.as_deref(),
            names.full_name.as_deref(),
        );
        out.push((index as u32, families, names.family, style));
    }
    Ok(out)
}
