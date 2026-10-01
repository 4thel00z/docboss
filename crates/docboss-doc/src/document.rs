//! Assembles a model [`Document`] from the streams of a Word binary file.

use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};

use docboss_cfb::CompoundFile;
use docboss_model::{
    Block, Comment, Diagnostic, Document, HeaderFooter, HeaderFooterKind, Inline, Note, NoteKind,
    Paragraph, PositionBase, Run, RunContent, Section, SectionProperties, Settings, SourceFormat,
    WrapKind, WrapSide,
};

use crate::bytes::{plc, slice, u16_at, u32_at, utf16};
use crate::fib::{slot, Fib};
use crate::picture::{
    blip, children, record, shape_alignment, shape_blip_index, shape_containers, shape_format,
    shape_geometry, shape_groups, shape_id, shape_wrap,
};
use crate::props::{apply_sep, default_section, dttm};
use crate::story::{Anchor, Context, Marker, Reference, StoryKind};
use crate::text::PieceTable;
use crate::{crypt, fkp, lists, sttb, styles, word6, Error, Result};

/// The start CP of each document part ([MS-DOC] §2.3): main text, then
/// footnotes, headers, comments, endnotes, text boxes, header text boxes.
/// [MS-DOC] §2.3.1, §2.3.2, §2.3.3, §2.3.4, §2.3.5, §2.3.6, §2.3.7, §2.2.1.
struct Parts {
    main: u32,
    footnotes: u32,
    headers: u32,
    comments: u32,
    endnotes: u32,
    textboxes: u32,
    header_textboxes: u32,
}

impl Parts {
    fn of(fib: &Fib) -> Parts {
        let c = fib.counts;
        let footnotes = c.text;
        let headers = footnotes.saturating_add(c.footnotes);
        let comments = headers.saturating_add(c.headers);
        let endnotes = comments.saturating_add(c.comments);
        let textboxes = endnotes.saturating_add(c.endnotes);
        let header_textboxes = textboxes.saturating_add(c.textboxes);
        Parts {
            main: 0,
            footnotes,
            headers,
            comments,
            endnotes,
            textboxes,
            header_textboxes,
        }
    }
}

fn table_range<'t>(table: &'t [u8], fib: &Fib, which: usize) -> &'t [u8] {
    fib.range(which)
        .map_or(&[], |(fc, lcb)| slice(table, fc, lcb))
}

pub fn read(bytes: &[u8], password: Option<&str>) -> Result<Document> {
    if crate::is_word2(bytes) {
        return Ok(word2(bytes));
    }
    let file = match CompoundFile::parse(bytes) {
        Ok(file) => file,
        Err(docboss_cfb::Error::NotCompoundFile) if !damaged_signature(bytes) => {
            return Err(Error::Cfb(docboss_cfb::Error::NotCompoundFile))
        }
        Err(error) => return recover(bytes, &format!("compound file unreadable ({error})")),
    };
    let mut diagnostics = Vec::new();
    // [MS-DOC] §2.1, §2.1.1, §2.1.3, §2.1.6, §2.1.7: the WordDocument, table and Data streams and the summary streams.
    let Ok(word_stream) = file.open_stream("WordDocument") else {
        return recover(bytes, "no WordDocument stream in the directory");
    };
    let fib = match Fib::parse(&word_stream) {
        Ok(fib) => fib,
        Err(error) => return recover(bytes, &format!("WordDocument stream unreadable ({error})")),
    };
    let metadata = docboss_cfb::property::read_metadata(&file, &mut diagnostics);
    if !fib.is_word97() && fib.encrypted {
        return Err(Error::UnsupportedEncryption(
            "Word 6/95 password protection",
        ));
    }
    if !fib.is_word97() {
        if !equation_streams(&file).is_empty() {
            diagnostics.push(Diagnostic::dropped(
                "ObjectPool",
                "Word 6/95 Equation Editor objects keep their picture; their equations are not read",
            ));
        }
        let mut document = word6_document(&word_stream, &fib, &mut diagnostics);
        document.metadata = metadata;
        document.diagnostics = file.diagnostics().into_iter().chain(diagnostics).collect();
        return Ok(document);
    }
    let table_stream = match file.open_stream(fib.table_stream_name()) {
        Ok(stream) => stream,
        Err(_) => {
            diagnostics.push(Diagnostic::dropped(
                fib.table_stream_name(),
                "table stream is missing",
            ));
            Cow::Borrowed(&[][..])
        }
    };
    let data_stream = file.open_stream("Data").unwrap_or(Cow::Borrowed(&[]));
    let (word, table, data) = match fib.encrypted {
        false => (word_stream, table_stream, data_stream),
        true => decrypt(&fib, &word_stream, &table_stream, &data_stream, password)?,
    };
    let fib = match fib.encrypted {
        false => fib,
        true => Fib::parse(&word)?,
    };
    let equations = equations(&file, &mut diagnostics);
    let mut document = assemble(&word, &table, &data, &fib, equations, &mut diagnostics);
    document.metadata = metadata;
    document.diagnostics = file.diagnostics().into_iter().chain(diagnostics).collect();
    Ok(document)
}

type Streams<'a> = (Cow<'a, [u8]>, Cow<'a, [u8]>, Cow<'a, [u8]>);

/// [MS-DOC] §2.2.6.2 and §2.2.6.3: every stream is RC4-encrypted in
/// 512-byte blocks except the FibBase and the encryption header.
fn decrypt<'a>(
    fib: &Fib,
    word: &[u8],
    table: &[u8],
    data: &[u8],
    password: Option<&str>,
) -> Result<Streams<'a>> {
    let Some(password) = password else {
        return Err(Error::Encrypted);
    };
    if fib.obfuscated {
        return deobfuscate(fib, word, table, data, password);
    }
    let (cipher, header) = crypt::open(table, password)?;
    let header = if fib.key == 0 {
        header
    } else {
        fib.key as usize
    };
    Ok((
        Cow::Owned(cipher.decrypt_stream(word, 68)),
        Cow::Owned(cipher.decrypt_stream(table, header)),
        Cow::Owned(cipher.decrypt_stream(data, 0)),
    ))
}

/// [MS-DOC] §2.2.6.1: XOR obfuscation of the WordDocument stream past its
/// first 68 bytes, the table stream and the Data stream, keyed by the
/// password whose verifier FibBase.lKey holds.
fn deobfuscate<'a>(
    fib: &Fib,
    word: &[u8],
    table: &[u8],
    data: &[u8],
    password: &str,
) -> Result<Streams<'a>> {
    let xor = docboss_crypt::xor::XorObfuscation::open(password, fib.key, code_page(fib.lid))
        .map_err(|_| Error::WrongPassword)?;
    let clear = |stream: &[u8], header: usize| {
        let mut out = stream.to_vec();
        xor.apply(&mut out, header);
        Cow::Owned(out)
    };
    Ok((clear(word, 68), clear(table, 0), clear(data, 0)))
}

/// The equations of the Equation Editor objects of the ObjectPool storage,
/// by the object id their storage name `_<id>` gives: the MTEF data of each
/// object's `Equation Native` stream. One that does not read is reported.
/// [MS-DOC] §2.1.4.
fn equations(
    file: &CompoundFile<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) -> HashMap<u32, docboss_model::Math> {
    equation_streams(file)
        .into_iter()
        .filter_map(|(id, stream)| {
            let math = docboss_mtef::read(&file.stream(stream));
            if math.is_none() {
                diagnostics.push(Diagnostic::dropped(
                    format!("ObjectPool/_{id}/Equation Native"),
                    "an Equation Editor object's equation does not read; its picture stays",
                ));
            }
            Some((id, math?))
        })
        .collect()
}

/// The `Equation Native` streams of the ObjectPool storage by object id.
/// [MS-DOC] §2.1.4.
fn equation_streams(file: &CompoundFile<'_>) -> Vec<(u32, usize)> {
    let Some(pool) = file.find("ObjectPool") else {
        return Vec::new();
    };
    file.children(pool)
        .into_iter()
        .filter_map(|storage| {
            let id = file.entries()[storage]
                .name
                .strip_prefix('_')?
                .parse::<u32>()
                .ok()?;
            Some((id, file.child(storage, "Equation Native")?))
        })
        .collect()
}

fn assemble(
    word: &[u8],
    table: &[u8],
    data: &[u8],
    fib: &Fib,
    equations: HashMap<u32, docboss_model::Math>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Document {
    let parts = Parts::of(fib);
    let pieces = match PieceTable::parse(table_range(table, fib, slot::CLX)) {
        Some(pieces) if !pieces.pieces.is_empty() => pieces,
        _ => {
            diagnostics.push(Diagnostic::approximated(
                "1Table",
                "no readable piece table; text read from fcMin",
            ));
            PieceTable::single(fib.fc_min, fib.counts.text, true)
        }
    };
    let font_entries = sttb::fonts(table_range(table, fib, slot::STTBF_FFN));
    let fonts: Vec<String> = font_entries.iter().map(|f| f.name.clone()).collect();
    let sheet = styles::parse(table_range(table, fib, slot::STSHF));
    let model_styles = sheet.to_model(&fonts);
    let numbering = lists::parse(
        table,
        fib.range(slot::PLF_LST),
        fib.range(slot::PLF_LFO),
        &fonts,
    );
    let chpx = fkp::chpx_runs(word, table_range(table, fib, slot::PLCF_BTE_CHPX));
    let papx = fkp::papx_runs(word, table_range(table, fib, slot::PLCF_BTE_PAPX));
    let mut context = Context::new(
        word,
        data,
        pieces,
        chpx,
        papx,
        sheet,
        model_styles,
        numbering,
        fonts,
    );
    context.equations = equations;
    context.authors = sttb::read(table_range(table, fib, slot::STTBF_RMARK))
        .into_iter()
        .map(|e| e.text)
        .collect();

    let footnotes = notes(
        &mut context,
        table,
        fib,
        slot::PLCFFND_REF,
        slot::PLCFFND_TXT,
        parts.footnotes,
        NoteKind::Footnote,
    );
    let endnotes = notes(
        &mut context,
        table,
        fib,
        slot::PLCFEND_REF,
        slot::PLCFEND_TXT,
        parts.endnotes,
        NoteKind::Endnote,
    );
    let comment_ranges = comment_references(&mut context, table, fib);
    bookmarks(&mut context, table, fib);
    context.markers.extend(comment_ranges);
    context.markers.sort_by_key(|(cp, rank, _)| (*cp, *rank));
    shapes(&mut context, word, table, fib, &parts);

    let footnotes: Vec<Note> = footnotes
        .into_iter()
        .map(|(id, start, end)| Note {
            id,
            kind: NoteKind::Footnote,
            blocks: context.story(start, end, StoryKind::Note),
        })
        .collect();
    let endnotes: Vec<Note> = endnotes
        .into_iter()
        .map(|(id, start, end)| Note {
            id,
            kind: NoteKind::Endnote,
            blocks: context.story(start, end, StoryKind::Note),
        })
        .collect();
    let comments = comments(&context, table, fib, parts.comments);
    let (sections, headers_footers) = sections(&context, word, table, fib, &parts, None);
    let dop = table_range(table, fib, slot::DOP);
    let settings = Settings {
        default_tab_stop: u16_at(dop, 10).map_or(720, |v| i32::from(v).max(1)),
        even_and_odd_headers: dop.first().is_some_and(|b| b & 1 != 0),
        compatibility_mode: None,
    };
    let media = context.media.take();
    diagnostics.extend(context.diagnostics.take());
    Document {
        format: SourceFormat::Doc,
        settings,
        styles: context.styles,
        numbering: context.numbering,
        sections,
        headers_footers,
        footnotes,
        endnotes,
        comments,
        media,
        fonts: font_entries,
        ..Document::default()
    }
}

/// Footnote or endnote references and text ranges ([MS-DOC] §2.8.19,
/// §2.8.20, §2.8.16, §2.8.17). Registers each reference CP and returns
/// `(id, start, end)` for each note body.
/// [MS-DOC] §2.8.19, §2.8.20, §2.8.16, §2.8.17, §2.3.2, §2.3.5.
fn notes(
    context: &mut Context<'_>,
    table: &[u8],
    fib: &Fib,
    reference_slot: usize,
    text_slot: usize,
    base: u32,
    kind: NoteKind,
) -> Vec<(i64, u32, u32)> {
    let (references, _) = plc(table_range(table, fib, reference_slot), 2);
    let (texts, _) = plc(table_range(table, fib, text_slot), 0);
    let count = references.len().saturating_sub(1);
    (0..count)
        .filter_map(|i| {
            let id = i as i64 + 1;
            let reference = match kind {
                NoteKind::Footnote => Reference::Footnote(id),
                NoteKind::Endnote => Reference::Endnote(id),
            };
            context.references.insert(references[i], reference);
            let start = base.saturating_add(*texts.get(i)?);
            let end = base.saturating_add(*texts.get(i + 1)?);
            Some((id, start, end))
        })
        .collect()
}

/// Comment references ([MS-DOC] §2.8.7 PlcfandRef with ATRDPre10) and the
/// ranges their bookmarks mark (SttbfAtnBkmk, PlcfAtnBkf, PlcfAtnBkl).
/// [MS-DOC] §2.8.7, §2.8.8, §2.9.7, §2.9.277, §2.3.4, §2.8.1, §2.8.3.
fn comment_references(
    context: &mut Context<'_>,
    table: &[u8],
    fib: &Fib,
) -> Vec<(u32, u8, Marker)> {
    let (references, atrds) = plc(table_range(table, fib, slot::PLCFAND_REF), 30);
    for (i, &cp) in references.iter().take(atrds.len()).enumerate() {
        context.references.insert(cp, Reference::Comment(i as i64));
    }
    let tags: Vec<u32> = sttb::read(table_range(table, fib, 37))
        .iter()
        .map(|e| u32_at(e.extra, 2).unwrap_or(u32::MAX))
        .collect();
    let (starts, bkfs) = plc(table_range(table, fib, 42), 4);
    let (ends, _) = plc(table_range(table, fib, 43), 0);
    let mut out = Vec::new();
    for (i, atrd) in atrds.iter().enumerate() {
        let Some(tag) = u32_at(atrd, 26).filter(|&t| t != u32::MAX) else {
            continue;
        };
        let Some(bookmark) = tags.iter().position(|&t| t == tag) else {
            continue;
        };
        let (Some(&start), Some(bkf)) = (starts.get(bookmark), bkfs.get(bookmark)) else {
            continue;
        };
        let Some(&end) = u16_at(bkf, 0).and_then(|ibkl| ends.get(usize::from(ibkl))) else {
            continue;
        };
        out.push((start, 1, Marker::CommentStart(i as i64)));
        out.push((
            end,
            if end == start { 2 } else { 0 },
            Marker::CommentEnd(i as i64),
        ));
    }
    out
}

fn comments(context: &Context<'_>, table: &[u8], fib: &Fib, base: u32) -> Vec<Comment> {
    let (_, atrds) = plc(table_range(table, fib, slot::PLCFAND_REF), 30);
    let (texts, _) = plc(table_range(table, fib, slot::PLCFAND_TXT), 0);
    let owners = sttb::xst_list(table_range(table, fib, slot::GRP_XST_ATN_OWNERS));
    let extra = table_range(table, fib, 112);
    atrds
        .iter()
        .enumerate()
        .filter_map(|(i, atrd)| {
            let start = base.saturating_add(*texts.get(i)?);
            let end = base.saturating_add(*texts.get(i + 1)?);
            let initials_length = usize::from(u16_at(atrd, 0).unwrap_or(0)).min(9);
            let initials = utf16(atrd, 2, initials_length);
            let owner = u16_at(atrd, 20)
                .and_then(|o| owners.get(usize::from(o)))
                .cloned();
            let date = u32_at(extra, i * 18).and_then(dttm);
            Some(Comment {
                id: i as i64,
                author: owner,
                initials: (!initials.is_empty()).then_some(initials),
                date,
                blocks: context.story(start, end, StoryKind::Comment),
            })
        })
        .collect()
}

/// Bookmarks ([MS-DOC] §2.8.10 Plcfbkf, §2.8.12 Plcfbkl, SttbfBkmk).
/// [MS-DOC] §2.8.10, §2.8.12, §2.9.279, §2.9.9, §2.9.11.
fn bookmarks(context: &mut Context<'_>, table: &[u8], fib: &Fib) {
    let names = sttb::read(table_range(table, fib, slot::STTBF_BKMK));
    let (starts, bkfs) = plc(table_range(table, fib, slot::PLCF_BKF), 4);
    let (ends, _) = plc(table_range(table, fib, slot::PLCF_BKL), 0);
    for (i, (bkf, name)) in bkfs.iter().zip(&names).enumerate() {
        let (Some(&start), Some(&end)) = (
            starts.get(i),
            u16_at(bkf, 0).and_then(|ibkl| ends.get(usize::from(ibkl))),
        ) else {
            continue;
        };
        context
            .markers
            .push((start, 1, Marker::BookmarkStart(i as i64, name.text.clone())));
        context.markers.push((
            end,
            if end == start { 2 } else { 0 },
            Marker::BookmarkEnd(i as i64),
        ));
    }
}

/// Floating shapes ([MS-DOC] §2.8.27 PlcfSpa) and the pictures behind them
/// in the OfficeArt drawing data ([MS-DOC] §2.9.171 OfficeArtContent: the
/// drawing group, then one OfficeArtWordDrawing per story, each a dgglbl
/// byte and a drawing container, §2.9.172).
/// The Spa bx and by fields give the origin of each axis, wr and wrk the
/// text wrapping; fBelowText counts only for a shape text runs through.
/// [MS-DOC] §2.8.27, §2.9.253, §2.9.171, §2.9.172.
fn shapes(context: &mut Context<'_>, word: &[u8], table: &[u8], fib: &Fib, parts: &Parts) {
    let main = plc(table_range(table, fib, slot::PLC_SPA_MOM), 26);
    let headers = plc(table_range(table, fib, slot::PLC_SPA_HDR), 26);
    let header_cps = headers.0.iter().map(|cp| parts.headers.saturating_add(*cp));
    let anchored = main
        .0
        .iter()
        .copied()
        .zip(main.1)
        .chain(header_cps.zip(headers.1));
    for (cp, spa) in anchored {
        let flags = u16_at(spa, 20).unwrap_or(0);
        let value = |at: usize| u32_at(spa, at).unwrap_or(0) as i32;
        context.anchors.insert(
            cp,
            Anchor {
                shape_id: u32_at(spa, 0).unwrap_or(0),
                left: value(4),
                top: value(8),
                right: value(12),
                bottom: value(16),
                horizontal: match (flags >> 1) & 3 {
                    0 => PositionBase::Margin,
                    1 => PositionBase::Page,
                    _ => PositionBase::Column,
                },
                vertical: match (flags >> 3) & 3 {
                    0 => PositionBase::Margin,
                    1 => PositionBase::Page,
                    _ => PositionBase::Paragraph,
                },
                wrap: match (flags >> 5) & 0xF {
                    1 => WrapKind::TopAndBottom,
                    3 => WrapKind::None,
                    4 => WrapKind::Tight,
                    5 => WrapKind::Through,
                    _ => WrapKind::Square,
                },
                wrap_side: match (flags >> 9) & 0xF {
                    1 => WrapSide::Left,
                    2 => WrapSide::Right,
                    3 => WrapSide::Largest,
                    _ => WrapSide::Both,
                },
                behind_text: (flags >> 5) & 0xF == 3 && flags & 0x4000 != 0,
            },
        );
    }
    if context.anchors.is_empty() {
        return;
    }
    let stories = [
        (slot::PLCFTXBX_TXT, parts.textboxes),
        // [MS-DOC] §2.8.32, §2.8.23: the text box and header text box stories.
        (slot::PLCF_HDRTXBX_TXT, parts.header_textboxes),
    ];
    for (which, base) in stories {
        let (cps, boxes) = plc(table_range(table, fib, which), 22);
        let last = boxes.len().saturating_sub(1);
        for (i, ftxbxs) in boxes.iter().enumerate().take(last) {
            if u16_at(ftxbxs, 8).unwrap_or(1) & 1 != 0 {
                continue;
            }
            let (Some(lid), Some(&start), Some(&end)) =
                (u32_at(ftxbxs, 14), cps.get(i), cps.get(i + 1))
            else {
                continue;
            };
            let guard = u32::from(end > start + 1);
            context.text_boxes.insert(
                lid,
                (base.saturating_add(start), base.saturating_add(end - guard)),
            );
        }
    }
    let Some((fc, lcb)) = fib.range(slot::DGG_INFO) else {
        return;
    };
    let end = (fc + lcb).min(table.len());
    let top = children(table, fc, end);
    let Some(dgg) = top.iter().find(|r| r.kind == 0xF000) else {
        return;
    };
    let store = children(table, dgg.body, dgg.body + dgg.length)
        .into_iter()
        .find(|r| r.kind == 0xF001);
    let fbses = store
        .map(|s| children(table, s.body, s.body + s.length))
        .unwrap_or_default();
    context.blip_store = fbses
        .iter()
        .map(|fbse| {
            let name = usize::from(table.get(fbse.body + 33).copied().unwrap_or(0));
            let inner = fbse.body + 36 + name;
            let embedded = (inner + 8 <= fbse.body + fbse.length)
                .then(|| record(table, inner))
                .flatten()
                .and_then(|r| blip(table, &r));
            embedded.or_else(|| {
                let delay = u32_at(table, fbse.body + 28)? as usize;
                blip(word, &record(word, delay)?)
            })
        })
        .collect();
    let mut containers = Vec::new();
    let mut at = dgg.body + dgg.length;
    while at + 9 <= end {
        let Some(drawing) = record(table, at + 1).filter(|r| r.kind == 0xF002) else {
            break;
        };
        shape_containers(
            table,
            drawing.body,
            drawing.body + drawing.length,
            0,
            &mut containers,
        );
        let mut groups = Vec::new();
        shape_groups(
            table,
            drawing.body,
            drawing.body + drawing.length,
            &mut groups,
        );
        context
            .shape_groups
            .extend(groups.into_iter().map(|group| (group.id, group)));
        at = drawing.body + drawing.length;
    }
    for container in containers {
        if let Some(id) = shape_id(table, &container) {
            context
                .shape_formats
                .insert(id, shape_format(table, &container));
            context
                .shape_alignments
                .insert(id, shape_alignment(table, &container));
            context
                .shape_wraps
                .insert(id, shape_wrap(table, &container));
            if let Some(geometry) = shape_geometry(table, &container) {
                context.shape_geometries.insert(id, geometry);
            }
        }
        let (Some(id), Some(pib)) = (
            shape_id(table, &container),
            shape_blip_index(table, &container),
        ) else {
            continue;
        };
        if pib > 0 {
            context.shape_blips.insert(id, pib as usize - 1);
        }
    }
}

/// Sections ([MS-DOC] §2.8.26 PlcfSed, §2.9.243 Sed) and the headers and
/// footers of each ([MS-DOC] §2.8.22 Plcfhdd).
/// [MS-DOC] §2.8.26, §2.9.243, §2.9.245, §2.8.22.
fn sections(
    context: &Context<'_>,
    word: &[u8],
    table: &[u8],
    fib: &Fib,
    parts: &Parts,
    word6: Option<&RefCell<BTreeSet<u8>>>,
) -> (Vec<Section>, Vec<HeaderFooter>) {
    let (cps, seds) = plc(table_range(table, fib, slot::PLCF_SED), 12);
    let mut bounds: Vec<(u32, u32, SectionProperties)> = Vec::new();
    let mut header_flags: Vec<u8> = Vec::new();
    let mut start = parts.main;
    for (i, sed) in seds.iter().enumerate() {
        let end = cps
            .get(i + 1)
            .copied()
            .unwrap_or(fib.counts.text)
            .min(fib.counts.text);
        let mut properties = default_section();
        let fc = u32_at(sed, 2).unwrap_or(u32::MAX);
        let mut flags = 0;
        if fc != u32::MAX {
            let size = u16_at(word, fc as usize).unwrap_or(0) as usize;
            let grpprl = slice(word, fc as usize + 2, size);
            match word6 {
                None => apply_sep(grpprl, &mut properties),
                Some(dropped) => {
                    let translated = word6::translate(grpprl, &mut dropped.borrow_mut());
                    apply_sep(&translated, &mut properties);
                    flags = word6::header_flags(grpprl);
                }
            }
        }
        if end > start {
            bounds.push((start, end, properties));
            header_flags.push(flags);
            start = end;
        }
    }
    if bounds.is_empty() || start < fib.counts.text {
        bounds.push((start, fib.counts.text, default_section()));
        header_flags.push(0);
    }
    let dop_flags = table_range(table, fib, slot::DOP)
        .get(1)
        .copied()
        .unwrap_or(0);
    let word6_stories = word6
        .is_some()
        .then(|| word6::header_stories(dop_flags, &header_flags));
    let story_index = |section: usize, kind: usize| match &word6_stories {
        None => Some(6 + section * 6 + kind),
        Some(stories) => stories.get(section).and_then(|s| s[kind]),
    };
    let (stories, _) = plc(table_range(table, fib, slot::PLCF_HDD), 0);
    let mut parts_out: Vec<HeaderFooter> = Vec::new();
    let mut ids: HashMap<usize, String> = HashMap::new();
    let mut story_id = |index: usize, kind: usize| -> Option<String> {
        let (&story_start, &story_end) = (stories.get(index)?, stories.get(index + 1)?);
        if story_end <= story_start {
            return None;
        }
        if let Some(id) = ids.get(&index) {
            return Some(id.clone());
        }
        let id = format!("hdr{index}");
        let kind = match kind {
            2 | 3 | 5 => HeaderFooterKind::Footer,
            _ => HeaderFooterKind::Header,
        };
        let guard = u32::from(story_end > story_start + 1);
        let blocks = context.story(
            parts.headers.saturating_add(story_start),
            parts.headers.saturating_add(story_end - guard),
            StoryKind::HeaderFooter,
        );
        parts_out.push(HeaderFooter {
            id: id.clone(),
            kind,
            blocks,
        });
        ids.insert(index, id.clone());
        Some(id)
    };
    let mut previous = SectionProperties::default();
    let sections = bounds
        .into_iter()
        .enumerate()
        .map(|(i, (start, end, mut properties))| {
            let slot =
                |offset: usize,
                 old: &Option<String>,
                 story_id: &mut dyn FnMut(usize, usize) -> Option<String>| {
                    story_index(i, offset)
                        .and_then(|index| story_id(index, offset))
                        .or_else(|| old.clone())
                };
            properties.headers.even = slot(0, &previous.headers.even, &mut story_id);
            properties.headers.default = slot(1, &previous.headers.default, &mut story_id);
            properties.footers.even = slot(2, &previous.footers.even, &mut story_id);
            properties.footers.default = slot(3, &previous.footers.default, &mut story_id);
            properties.headers.first = slot(4, &previous.headers.first, &mut story_id);
            properties.footers.first = slot(5, &previous.footers.first, &mut story_id);
            previous = properties.clone();
            Section {
                properties,
                blocks: context.story(start, end, StoryKind::Main),
            }
        })
        .collect();
    (sections, parts_out)
}

/// The Windows code page of 8-bit text in a language, for Word 6 and 95
/// files, which store text in the code page of FibBase.lid.
fn code_page(lid: u16) -> u32 {
    match lid & 0x03FF {
        0x05 | 0x15 | 0x0E | 0x1B | 0x24 | 0x1A | 0x18 | 0x1C => 1250,
        0x19 | 0x22 | 0x02 | 0x23 | 0x2F => 1251,
        0x08 => 1253,
        0x1F => 1254,
        0x0D => 1255,
        0x01 | 0x20 | 0x29 => 1256,
        0x25..=0x27 => 1257,
        0x2A => 1258,
        0x1E => 874,
        0x11 => 932,
        0x04 => {
            if lid == 0x0804 {
                936
            } else {
                950
            }
        }
        0x12 => 949,
        _ => 1252,
    }
}

/// Word 6 and Word 95 files: the Word 97 assembly over the one
/// WordDocument stream, which also holds the tables, with the formatting
/// translated by [`word6`] and the text in the code page of the document
/// language (or Cyrillic when the bytes say so). Numbered and bulleted
/// paragraphs become lists from their ANLDs; comments and drawing objects,
/// whose Word 6 records differ from Word 97's, are left out and reported.
fn word6_document(word: &[u8], fib: &Fib, diagnostics: &mut Vec<Diagnostic>) -> Document {
    let stated = code_page(fib.lid);
    let text_bytes = fib.fc_mac.saturating_sub(fib.fc_min).min(1 << 16);
    let sample = slice(word, fib.fc_min as usize, text_bytes as usize);
    let code_page = guess_code_page(sample, stated);
    if code_page != stated {
        diagnostics.push(Diagnostic::approximated(
            "WordDocument",
            format!("text decoded as code page {code_page}, guessed from its bytes"),
        ));
    }
    if !docboss_cfb::codepage::is_supported(code_page) {
        diagnostics.push(Diagnostic::approximated(
            "WordDocument",
            format!("code page {code_page} is not supported; text read as ISO 8859-1"),
        ));
    }
    let dropped = RefCell::new(BTreeSet::new());
    let parts = Parts::of(fib);
    let clx = table_range(word, fib, slot::CLX);
    let complex = match fib.complex && !clx.is_empty() {
        true => PieceTable::parse(clx).filter(|p| !p.pieces.is_empty()),
        false => None,
    };
    let mut pieces = match complex {
        Some(pieces) if word6::unicode_pieces(word, &pieces) => pieces,
        Some(pieces) => pieces.eight_bit(),
        None => PieceTable::single(fib.fc_min, fib.fc_mac.saturating_sub(fib.fc_min), true),
    }
    .in_code_page(code_page);
    pieces.prcs = pieces
        .prcs
        .iter()
        .map(|prc| word6::translate(prc, &mut dropped.borrow_mut()))
        .collect();
    let font_entries = word6::fonts(table_range(word, fib, slot::STTBF_FFN), code_page);
    let fonts: Vec<String> = font_entries.iter().map(|f| f.name.clone()).collect();
    let sheet = styles::parse_word6(
        table_range(word, fib, slot::STSHF),
        code_page,
        &mut dropped.borrow_mut(),
    );
    let model_styles = sheet.to_model(&fonts);
    let mut arena = Vec::new();
    let fkps = word6::fkps(
        word,
        table_range(word, fib, slot::PLCF_BTE_CHPX),
        table_range(word, fib, slot::PLCF_BTE_PAPX),
        &mut arena,
        &mut dropped.borrow_mut(),
    );
    let mut extended = Vec::with_capacity(word.len() + arena.len());
    extended.extend_from_slice(word);
    extended.extend_from_slice(&arena);
    let mut context = Context::new(
        &extended,
        word,
        pieces,
        fkps.chpx,
        fkps.papx,
        sheet,
        model_styles,
        docboss_model::Numbering::default(),
        fonts,
    );
    context.word6 = true;
    let footnotes = notes(
        &mut context,
        word,
        fib,
        slot::PLCFFND_REF,
        slot::PLCFFND_TXT,
        parts.footnotes,
        NoteKind::Footnote,
    );
    bookmarks(&mut context, word, fib);
    context.markers.sort_by_key(|(cp, rank, _)| (*cp, *rank));
    let footnotes: Vec<Note> = footnotes
        .into_iter()
        .map(|(id, start, end)| Note {
            id,
            kind: NoteKind::Footnote,
            blocks: context.story(start, end, StoryKind::Note),
        })
        .collect();
    let (sections, headers_footers) = sections(&context, word, word, fib, &parts, Some(&dropped));
    let dop = table_range(word, fib, slot::DOP);
    let settings = Settings {
        default_tab_stop: u16_at(dop, 10).map_or(720, |v| i32::from(v).max(1)),
        even_and_odd_headers: dop.first().is_some_and(|b| b & 1 != 0),
        compatibility_mode: None,
    };
    if fib.range(slot::PLCFAND_REF).is_some() {
        diagnostics.push(Diagnostic::dropped(
            "WordDocument",
            "Word 6/95 comments are not read",
        ));
    }
    if fib.counts.endnotes > 0 {
        diagnostics.push(Diagnostic::dropped(
            "WordDocument",
            "Word 6/95 endnotes are not read",
        ));
    }
    if u32_at(word, 0x196).is_some_and(|lcb| lcb > 0) {
        diagnostics.push(Diagnostic::dropped(
            "WordDocument",
            "Word 6/95 drawing objects are not read",
        ));
    }
    for (definition, instance) in context.anld_numbering() {
        context.numbering.abstracts.push(definition);
        context.numbering.instances.push(instance);
    }
    context.numbering.sort_by_id();
    let dropped = dropped.into_inner();
    let others: Vec<String> = dropped.iter().map(|code| code.to_string()).collect();
    if !others.is_empty() {
        diagnostics.push(Diagnostic::approximated(
            "WordDocument",
            format!("Word 6/95 sprms not read: {}", others.join(", ")),
        ));
    }
    let media = context.media.take();
    diagnostics.extend(context.diagnostics.take());
    Document {
        format: SourceFormat::Doc,
        settings,
        styles: context.styles,
        numbering: context.numbering,
        sections,
        headers_footers,
        footnotes,
        media,
        fonts: font_entries,
        ..Document::default()
    }
}

/// Whether the first sector of a file could be a damaged compound file
/// header: six of the eight signature bytes, or a zeroed sector in a file
/// of whole sectors ([MS-CFB] §2.2).
pub fn damaged_signature(bytes: &[u8]) -> bool {
    const SIGNATURE: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
    let Some(head) = bytes.get(..8) else {
        return false;
    };
    let same = head
        .iter()
        .zip(SIGNATURE)
        .filter(|(a, b)| **a == *b)
        .count();
    same >= 6 || (head == [0; 8] && bytes.len().is_multiple_of(512))
}

/// The offset of the first plausible FIB at a multiple of 64 bytes past the
/// header, where regular and mini stream sectors start: wIdent 0xA5EC (or
/// Word 6's 0xA5DC) and a Word 6 to Word 2007 nFib.
pub fn find_fib(bytes: &[u8]) -> Option<usize> {
    (512..bytes.len().saturating_sub(32))
        .step_by(64)
        .find(|&at| {
            matches!(u16_at(bytes, at), Some(0xA5EC | 0xA5DC))
                && u16_at(bytes, at + 2).is_some_and(|n| (0x0065..=0x0112).contains(&n))
        })
}

/// A compound file whose directory, FAT or WordDocument entry is
/// damaged: the WordDocument stream is taken to run on from the FIB found
/// by [`find_fib`]. A Word 6 or 95 stream holds everything and is read in
/// full; of a Word 97 stream, whose table stream is lost, the text from
/// fcMin to fcMac is read, as 16-bit text when nearly every second byte is
/// zero and 8-bit text otherwise.
fn recover(bytes: &[u8], reason: &str) -> Result<Document> {
    let at = find_fib(bytes).ok_or(Error::NotWord("no readable directory and no FIB found"))?;
    let word = &bytes[at..];
    let fib = Fib::parse(word)?;
    if fib.encrypted {
        return Err(Error::Encrypted);
    }
    let note = Diagnostic::approximated(
        "WordDocument",
        format!("{reason}; the WordDocument stream is read on from the FIB at offset {at:#x}"),
    );
    if !fib.is_word97() {
        let mut diagnostics = vec![note];
        let mut document = word6_document(word, &fib, &mut diagnostics);
        document.diagnostics = diagnostics;
        return Ok(document);
    }
    let end = (fib.fc_mac as usize).min(word.len());
    let start = (fib.fc_min as usize).min(end);
    let region = &word[start..end];
    let zeros = region
        .iter()
        .skip(1)
        .step_by(2)
        .filter(|&&b| b == 0)
        .count();
    let wide = region.len() >= 2 && zeros * 10 >= (region.len() / 2) * 9;
    let text: String = match wide {
        true => docboss_cfb::codepage::utf16le(region),
        false => docboss_cfb::codepage::decode(code_page(fib.lid), region).0,
    };
    let blocks = text
        .split(['\r', '\u{7}'])
        .map(|line| {
            let cleaned: String = line.chars().filter(|c| *c >= ' ' || *c == '\t').collect();
            let inlines = match cleaned.is_empty() {
                true => Vec::new(),
                false => vec![Inline::Run(Run {
                    content: vec![RunContent::Text(cleaned)],
                    ..Run::default()
                })],
            };
            Block::Paragraph(Paragraph {
                inlines,
                ..Paragraph::default()
            })
        })
        .collect();
    Ok(Document {
        format: SourceFormat::Doc,
        sections: vec![Section {
            properties: default_section(),
            blocks,
        }],
        diagnostics: vec![
            note,
            Diagnostic::dropped(
                "WordDocument",
                "without the table stream, text is read from fcMin to fcMac with no formatting, notes or headers",
            ),
        ],
        ..Document::default()
    })
}

/// Word for Windows 2.0 files, which are a bare FIB and text rather than a
/// compound file: the text between fcMin and fcMac.
fn word2(bytes: &[u8]) -> Document {
    let fib = Fib {
        n_fib: u16_at(bytes, 2).unwrap_or(0),
        lid: u16_at(bytes, 6).unwrap_or(0x0409),
        complex: false,
        encrypted: false,
        which_table_1: false,
        obfuscated: false,
        key: 0,
        counts: Default::default(),
        fc_lcb: Vec::new(),
        fc_min: u32_at(bytes, 24).unwrap_or(0),
        fc_mac: u32_at(bytes, 28).unwrap_or(0),
    };
    let mut document = legacy(bytes, &fib);
    document.diagnostics[0] = Diagnostic::approximated(
        "WordDocument",
        format!(
            "Word 2 file (nFib {:#x}): text only, formatting not read",
            fib.n_fib
        ),
    );
    document
}

/// A guess at the code page of 8-bit Cyrillic text stored without one: in
/// Russian text nearly every letter is a byte from 0xC0 up, in Western and
/// Central European text most are ASCII.
fn guess_code_page(bytes: &[u8], stated: u32) -> u32 {
    if stated != 1252 {
        return stated;
    }
    let high = bytes.iter().filter(|&&b| b >= 0xC0).count();
    let ascii = bytes.iter().filter(|b| b.is_ascii_alphabetic()).count();
    if high > ascii && high > 16 {
        return 1251;
    }
    stated
}

/// The 8-bit text of a Word 6 or 95 file: through its piece table when
/// the file is complex (fcClx at FIB offset 0x160), else fcMin to fcMac.
fn legacy_bytes(word: &[u8], fib: &Fib) -> Vec<u8> {
    let clx_at = crate::bytes::u32_at(word, 0x160).unwrap_or(0) as usize;
    let clx_size = crate::bytes::u32_at(word, 0x164).unwrap_or(0) as usize;
    let pieces = match fib.complex && clx_size > 0 {
        true => PieceTable::parse(slice(word, clx_at, clx_size)).map(PieceTable::eight_bit),
        false => None,
    };
    let Some(pieces) = pieces.filter(|p| !p.pieces.is_empty()) else {
        let length = fib.fc_mac.saturating_sub(fib.fc_min);
        return slice(word, fib.fc_min as usize, length as usize).to_vec();
    };
    let count = (fib.counts.text as usize).min(word.len());
    pieces
        .decode(word)
        .into_iter()
        .take(count)
        .map(|unit| unit as u8)
        .collect()
}

/// Word 6 and Word 95 files: the text in the code page of the document
/// language (or Cyrillic when the bytes say so), one paragraph per carriage
/// return, with no formatting.
fn legacy(word: &[u8], fib: &Fib) -> Document {
    let bytes = legacy_bytes(word, fib);
    let stated = code_page(fib.lid);
    let code_page = guess_code_page(&bytes, stated);
    let (text, fidelity) = docboss_cfb::codepage::decode(code_page, &bytes);
    let blocks = text
        .split(['\r', '\u{7}'])
        .map(|line| {
            let cleaned: String = line.chars().filter(|c| *c >= ' ' || *c == '\t').collect();
            let inlines = match cleaned.is_empty() {
                true => Vec::new(),
                false => vec![Inline::Run(Run {
                    content: vec![RunContent::Text(cleaned)],
                    ..Run::default()
                })],
            };
            Block::Paragraph(Paragraph {
                inlines,
                ..Paragraph::default()
            })
        })
        .collect();
    let mut diagnostics = vec![Diagnostic::approximated(
        "WordDocument",
        format!(
            "Word 6/95 file (nFib {:#x}): text only, formatting not read",
            fib.n_fib
        ),
    )];
    if code_page != stated {
        diagnostics.push(Diagnostic::approximated(
            "WordDocument",
            format!("text decoded as code page {code_page}, guessed from its bytes"),
        ));
    }
    if fidelity == docboss_cfb::codepage::Fidelity::Approximated {
        diagnostics.push(Diagnostic::approximated(
            "WordDocument",
            format!("code page {code_page} is not supported; text read as ISO 8859-1"),
        ));
    }
    Document {
        format: SourceFormat::Doc,
        sections: vec![Section {
            properties: default_section(),
            blocks,
        }],
        diagnostics,
        ..Document::default()
    }
}
