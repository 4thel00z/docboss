//! DrawingML text bodies (`a:p` paragraphs inside `dsp:txBody`,
//! `dgm:t`, `c:rich` and similar), read into model paragraphs.

use docboss_model::{
    Block, Break, Color, FontSlots, Indentation, Inline, Justification, LineRule, Paragraph,
    ParagraphProperties, Run, RunContent, RunProperties, Spacing, Underline, VerticalAlign,
};
use docboss_xml::{Element, Ns, Reader};

use super::color_choice;
use crate::props::Theme;
use crate::xml::{children, int};

/// The most paragraphs one text body keeps.
const MAX_PARAGRAPHS: usize = 10_000;

/// What a text body's runs take when their own properties are silent: the
/// size in hundredths of a point, the face and the color.
#[derive(Debug, Clone, Default)]
pub(crate) struct TextDefaults {
    pub size: u32,
    pub font: Option<String>,
    pub color: Option<Color>,
    pub bold: bool,
}

impl TextDefaults {
    /// The DrawingML defaults: 18 points in the theme's minor face.
    pub(crate) fn minor(theme: &Theme) -> TextDefaults {
        TextDefaults {
            size: 1800,
            font: theme.minor[0].clone(),
            color: None,
            bold: false,
        }
    }
}

/// A DrawingML run's properties before they become model run properties.
#[derive(Debug, Clone, Default)]
pub(crate) struct TextRun {
    pub size: Option<u32>,
    pub bold: Option<bool>,
    italic: Option<bool>,
    underline: Option<bool>,
    strike: Option<bool>,
    caps: Option<bool>,
    small_caps: Option<bool>,
    baseline: Option<i64>,
    pub font: Option<String>,
    pub color: Option<Color>,
}

impl TextRun {
    pub(crate) fn apply(&mut self, over: &TextRun) {
        macro_rules! take {
            ($($field:ident),*) => {$(if over.$field.is_some() { self.$field = over.$field.clone(); })*};
        }
        take!(size, bold, italic, underline, strike, caps, small_caps, baseline, font, color);
    }

    fn properties(&self, defaults: &TextDefaults) -> RunProperties {
        let font = self.font.clone().or_else(|| defaults.font.clone());
        let size = self.size.unwrap_or(defaults.size).clamp(100, 400_000);
        let baseline = self.baseline.unwrap_or(0);
        RunProperties {
            bold: Some(self.bold.unwrap_or(defaults.bold)),
            italic: Some(self.italic.unwrap_or(false)),
            underline: Some(match self.underline.unwrap_or(false) {
                true => Underline::Single,
                false => Underline::None,
            }),
            strike: Some(self.strike.unwrap_or(false)),
            caps: Some(self.caps.unwrap_or(false)),
            small_caps: Some(self.small_caps.unwrap_or(false)),
            fonts: FontSlots {
                ascii: font.clone(),
                high_ansi: font.clone(),
                east_asia: font.clone(),
                complex: font,
            },
            size: Some(size.div_ceil(50)),
            size_complex: Some(size.div_ceil(50)),
            color: Some(Some(self.color.or(defaults.color).unwrap_or(Color::BLACK))),
            vertical_align: match baseline {
                b if b > 0 => Some(VerticalAlign::Superscript),
                b if b < 0 => Some(VerticalAlign::Subscript),
                _ => Some(VerticalAlign::Baseline),
            },
            ..RunProperties::default()
        }
    }
}

/// A theme font reference such as `+mn-lt` resolved to its face, or the
/// face as written.
fn typeface(value: &str, theme: &Theme) -> Option<String> {
    let slot = |fonts: &[Option<String>; 3], script: &str| match script {
        "ea" => fonts[1].clone(),
        "cs" => fonts[2].clone(),
        _ => fonts[0].clone(),
    };
    match value.strip_prefix('+') {
        Some(rest) => match rest.split_once('-') {
            Some(("mn", script)) => slot(&theme.minor, script),
            Some(("mj", script)) => slot(&theme.major, script),
            _ => None,
        },
        None => (!value.is_empty()).then(|| value.to_string()),
    }
}

/// `a:rPr`, `a:defRPr` or `a:endParaRPr` (ECMA-376 Part 1 §21.1.2.3.9,
/// §21.1.2.3.2, §21.1.2.2.3): size, bold, italic, underline, strike,
/// caps, baseline shift, the fill's color and the `a:latin` face
/// (§21.1.2.3.7).
fn run_properties(reader: &mut Reader<'_>, e: &Element<'_>, theme: &Theme) -> TextRun {
    let flag = |name: &str| {
        e.attr_raw(Ns::NONE, name)
            .map(|v| matches!(v, "1" | "true"))
    };
    let mut run = TextRun {
        size: e
            .attr_raw(Ns::NONE, "sz")
            .and_then(int)
            .map(|v| v.clamp(0, 400_000) as u32),
        bold: flag("b"),
        italic: flag("i"),
        underline: e.attr_raw(Ns::NONE, "u").map(|u| u != "none"),
        strike: e.attr_raw(Ns::NONE, "strike").map(|s| s != "noStrike"),
        caps: e.attr_raw(Ns::NONE, "cap").map(|c| c == "all"),
        small_caps: e.attr_raw(Ns::NONE, "cap").map(|c| c == "small"),
        baseline: e.attr_raw(Ns::NONE, "baseline").and_then(int),
        ..TextRun::default()
    };
    children(reader, |reader, child| {
        if child.ns != Ns::A {
            return;
        }
        match child.local {
            "solidFill" => run.color = color_choice(reader, theme),
            "latin" => {
                run.font = child
                    .attr(Ns::NONE, "typeface")
                    .and_then(|face| typeface(&face, theme))
            }
            _ => {}
        }
    });
    run
}

/// A spacing value: `a:spcPct` in thousandths of a percent of the text
/// size, or `a:spcPts` in hundredths of a point (ECMA-376 Part 1
/// §21.1.2.2.11, §21.1.2.2.12).
#[derive(Debug, Clone, Copy)]
enum Space {
    Percent(i64),
    Points(i64),
}

fn space(reader: &mut Reader<'_>) -> Option<Space> {
    let mut found = None;
    children(reader, |_, e| {
        let value = e.attr_raw(Ns::NONE, "val").and_then(int);
        found = match (e.local, value) {
            ("spcPct", Some(v)) => Some(Space::Percent(v.clamp(0, 10_000_000))),
            ("spcPts", Some(v)) => Some(Space::Points(v.clamp(0, 1_000_000))),
            _ => found,
        };
    });
    found
}

/// A paragraph's `a:pPr` (ECMA-376 Part 1 §21.1.2.2.7).
#[derive(Debug, Clone, Default)]
pub(crate) struct TextParagraph {
    justification: Option<Justification>,
    margin: i64,
    indent: i64,
    line: Option<Space>,
    before: Option<Space>,
    after: Option<Space>,
    bullet: Option<char>,
    pub defaults: TextRun,
}

impl TextParagraph {
    fn apply(&mut self, over: &TextParagraph) {
        self.justification = over.justification.or(self.justification);
        self.margin = match over.margin {
            0 => self.margin,
            m => m,
        };
        self.indent = match over.indent {
            0 => self.indent,
            i => i,
        };
        self.line = over.line.or(self.line);
        self.before = over.before.or(self.before);
        self.after = over.after.or(self.after);
        self.bullet = over.bullet.or(self.bullet);
        self.defaults.apply(&over.defaults);
    }
}

/// `a:pPr` or a list style level: alignment, margins, line spacing
/// (§21.1.2.2.5), space before and after (§21.1.2.2.10, §21.1.2.2.9), a
/// character bullet (§21.1.2.4.3) and the default run properties.
fn paragraph_properties(reader: &mut Reader<'_>, e: &Element<'_>, theme: &Theme) -> TextParagraph {
    let emu = |name: &str| {
        e.attr_raw(Ns::NONE, name)
            .and_then(int)
            .map_or(0, |v| v.clamp(-51_206_400, 51_206_400))
    };
    let mut paragraph = TextParagraph {
        justification: match e.attr_raw(Ns::NONE, "algn") {
            Some("ctr") => Some(Justification::Center),
            Some("r") => Some(Justification::Right),
            Some("just" | "justLow") => Some(Justification::Both),
            Some("dist" | "thaiDist") => Some(Justification::Distribute),
            Some("l") => Some(Justification::Left),
            _ => None,
        },
        margin: emu("marL"),
        indent: emu("indent"),
        ..TextParagraph::default()
    };
    children(reader, |reader, child| {
        if child.ns != Ns::A {
            return;
        }
        match child.local {
            "lnSpc" => paragraph.line = space(reader),
            "spcBef" => paragraph.before = space(reader),
            "spcAft" => paragraph.after = space(reader),
            "buChar" => {
                paragraph.bullet = child.attr(Ns::NONE, "char").and_then(|c| c.chars().next())
            }
            "buNone" => paragraph.bullet = None,
            "defRPr" => paragraph.defaults = run_properties(reader, &child, theme),
            _ => {}
        }
    });
    paragraph
}

/// `a:lstStyle` (§21.1.2.4.12): the first level's paragraph properties,
/// which every paragraph of the body starts from.
fn list_style(reader: &mut Reader<'_>, theme: &Theme) -> TextParagraph {
    let mut first = TextParagraph::default();
    children(reader, |reader, e| {
        if e.ns == Ns::A && e.local == "lvl1pPr" {
            first = paragraph_properties(reader, &e, theme);
        }
    });
    first
}

fn twips_of(space: Space, size: u32) -> i32 {
    let twips = match space {
        Space::Percent(pct) => pct as f64 / 100_000.0 * f64::from(size) / 100.0 * 20.0,
        Space::Points(pts) => pts as f64 / 100.0 * 20.0,
    };
    twips.round().clamp(0.0, 31_680.0) as i32
}

/// A text body's paragraphs (ECMA-376 Part 1 §21.1.2.2.6): runs
/// (§21.1.2.3.8) with their text (§21.1.2.3.11), line breaks
/// (§21.1.2.2.1) and fields (§21.1.2.2.4) as their cached text. Every
/// paragraph states its spacing, indents and run formatting, so the
/// document's styles do not reach it. `base` holds the properties the
/// body's container gives its text before its own list style.
/// Elements other than those are handed to `other`, such as `a:bodyPr`.
pub(crate) fn text_body<'a>(
    reader: &mut Reader<'a>,
    theme: &Theme,
    defaults: &TextDefaults,
    base: Option<&TextParagraph>,
    mut other: impl FnMut(&mut Reader<'a>, &Element<'a>),
) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut inherited = base.cloned().unwrap_or_default();
    children(reader, |reader, e| match (e.ns, e.local) {
        (Ns::A, "lstStyle") => inherited.apply(&list_style(reader, theme)),
        (Ns::A, "p") => {
            if blocks.len() < MAX_PARAGRAPHS {
                blocks.push(Block::Paragraph(paragraph(
                    reader, theme, defaults, &inherited,
                )));
            }
        }
        _ => other(reader, &e),
    });
    blocks
}

/// The properties a container such as a chart's `c:txPr` (ECMA-376 Part 1
/// §21.2.2.216) gives the text inside it: its list style's first level
/// and its first paragraph's `a:pPr`.
pub(crate) fn body_defaults(reader: &mut Reader<'_>, theme: &Theme) -> TextParagraph {
    let mut found = TextParagraph::default();
    let mut first = true;
    children(reader, |reader, e| {
        if e.ns != Ns::A {
            return;
        }
        match e.local {
            "lstStyle" => found.apply(&list_style(reader, theme)),
            "p" if first => {
                first = false;
                children(reader, |reader, p| {
                    if p.ns == Ns::A && p.local == "pPr" {
                        found.apply(&paragraph_properties(reader, &p, theme));
                    }
                })
            }
            _ => {}
        }
    });
    found
}

/// The text of a `c:rich` body (§21.2.2.156), its paragraphs joined by
/// line breaks, and the properties of its first paragraph's first run
/// over the paragraph's defaults.
pub(crate) fn rich_text(reader: &mut Reader<'_>, theme: &Theme) -> (String, TextRun) {
    let mut text = String::new();
    let mut style: Option<TextRun> = None;
    let mut paragraphs = 0usize;
    children(reader, |reader, e| {
        if e.ns != Ns::A || e.local != "p" || paragraphs >= MAX_PARAGRAPHS {
            return;
        }
        paragraphs += 1;
        if paragraphs > 1 {
            text.push('\n');
        }
        let mut defaults = TextRun::default();
        children(reader, |reader, part| {
            if part.ns != Ns::A {
                return;
            }
            match part.local {
                "pPr" => defaults.apply(&paragraph_properties(reader, &part, theme).defaults),
                "r" | "fld" => children(reader, |reader, r| {
                    if r.ns != Ns::A {
                        return;
                    }
                    match r.local {
                        "rPr" if style.is_none() => {
                            let mut run = defaults.clone();
                            run.apply(&run_properties(reader, &r, theme));
                            style = Some(run);
                        }
                        "t" => text.push_str(&reader.read_text()),
                        _ => {}
                    }
                }),
                "br" => text.push('\n'),
                _ => {}
            }
        });
        if style.is_none() {
            style = Some(defaults);
        }
    });
    (text, style.unwrap_or_default())
}

fn paragraph(
    reader: &mut Reader<'_>,
    theme: &Theme,
    defaults: &TextDefaults,
    inherited: &TextParagraph,
) -> Paragraph {
    let mut format = inherited.clone();
    let mut runs: Vec<(TextRun, RunContent)> = Vec::new();
    let mut end = TextRun::default();
    children(reader, |reader, e| {
        if e.ns != Ns::A {
            return;
        }
        match e.local {
            "pPr" => format.apply(&paragraph_properties(reader, &e, theme)),
            "r" | "fld" => {
                let mut run = format.defaults.clone();
                let mut text = String::new();
                children(reader, |reader, part| {
                    if part.ns != Ns::A {
                        return;
                    }
                    match part.local {
                        "rPr" => run.apply(&run_properties(reader, &part, theme)),
                        "t" => text.push_str(&reader.read_text()),
                        _ => {}
                    }
                });
                if !text.is_empty() {
                    runs.push((run, RunContent::Text(text)));
                }
            }
            "br" => {
                let mut run = format.defaults.clone();
                children(reader, |reader, part| {
                    if part.ns == Ns::A && part.local == "rPr" {
                        run.apply(&run_properties(reader, &part, theme));
                    }
                });
                runs.push((run, RunContent::Break(Break::Line)));
            }
            "endParaRPr" => {
                end = format.defaults.clone();
                end.apply(&run_properties(reader, &e, theme));
            }
            _ => {}
        }
    });
    let mark_run = match runs.first() {
        Some((first, _)) => first.clone(),
        None => end,
    };
    let size = mark_run.size.unwrap_or(defaults.size);
    let line = format
        .line
        .map_or((240, LineRule::Auto), |line| match line {
            Space::Percent(pct) => ((pct * 240 / 100_000).clamp(24, 2400) as i32, LineRule::Auto),
            Space::Points(_) => (twips_of(line, size).max(20), LineRule::Exact),
        });
    let twips = |emu: i64| (emu / 635).clamp(-31_680, 31_680) as i32;
    let hanging = (format.indent < 0).then(|| twips(-format.indent));
    let first_line = (format.indent > 0).then(|| twips(format.indent));
    let properties = ParagraphProperties {
        justification: Some(format.justification.unwrap_or(Justification::Left)),
        indentation: Indentation {
            left: Some(twips(format.margin)),
            right: Some(0),
            first_line: Some(first_line.unwrap_or(0)),
            hanging,
        },
        spacing: Spacing {
            before: Some(format.before.map_or(0, |s| twips_of(s, size))),
            after: Some(format.after.map_or(0, |s| twips_of(s, size))),
            line: Some(line.0),
            line_rule: Some(line.1),
            before_auto: Some(false),
            after_auto: Some(false),
        },
        keep_next: Some(false),
        keep_lines: Some(false),
        widow_control: Some(false),
        contextual_spacing: Some(false),
        outline_level: Some(9),
        ..ParagraphProperties::default()
    };
    let mark = mark_run.properties(defaults);
    let mut inlines: Vec<Inline> = Vec::with_capacity(runs.len() + 1);
    if let Some(bullet) = format.bullet.filter(|_| !runs.is_empty()) {
        inlines.push(Inline::Run(Run {
            properties: mark.clone(),
            content: vec![RunContent::Text(bullet.to_string()), RunContent::Tab],
        }));
    }
    inlines.extend(runs.into_iter().map(|(run, content)| {
        Inline::Run(Run {
            properties: run.properties(defaults),
            content: vec![content],
        })
    }));
    Paragraph {
        style_id: None,
        properties,
        mark,
        inlines,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(xml: &str) -> Vec<Block> {
        let xml = format!(
            r#"<t xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">{xml}</t>"#
        );
        let mut reader = Reader::new(&xml);
        reader.next_event();
        text_body(
            &mut reader,
            &Theme::default(),
            &TextDefaults::minor(&Theme::default()),
            None,
            |_, _| {},
        )
    }

    /// ECMA-376 Part 1 §21.1.2.2.6, §21.1.2.2.7, §21.1.2.3.9, §21.1.2.2.5,
    /// §21.1.2.2.9: DrawingML paragraphs keep their alignment, run sizes
    /// and spacing, stated so document styles stay out.
    #[test]
    fn drawingml_paragraphs_become_model_paragraphs() {
        let blocks = body(
            r#"<a:bodyPr/><a:lstStyle/><a:p><a:pPr algn="ctr"><a:lnSpc><a:spcPct val="90000"/></a:lnSpc><a:spcAft><a:spcPct val="35000"/></a:spcAft></a:pPr><a:r><a:rPr sz="2000" b="1"/><a:t>Sample</a:t></a:r><a:br/><a:r><a:t>two</a:t></a:r></a:p><a:p><a:endParaRPr sz="1000"/></a:p>"#,
        );
        assert_eq!(blocks.len(), 2);
        let Block::Paragraph(first) = &blocks[0] else {
            panic!()
        };
        assert_eq!(first.text(), "Sample\ntwo");
        let p = &first.properties;
        assert_eq!(p.justification, Some(Justification::Center));
        assert_eq!(p.spacing.line, Some(216));
        assert_eq!(p.spacing.after, Some(140));
        let Inline::Run(run) = &first.inlines[0] else {
            panic!()
        };
        assert_eq!(run.properties.size, Some(40));
        assert_eq!(run.properties.bold, Some(true));
        let Block::Paragraph(second) = &blocks[1] else {
            panic!()
        };
        assert_eq!(second.mark.size, Some(20));
    }
}
