use std::collections::HashMap;

use crate::{Numbering, Paragraph, ParagraphProperties, RunProperties, TableProperties};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum StyleKind {
    Paragraph,
    Character,
    Table,
    Numbering,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Style {
    pub id: String,
    pub name: Option<String>,
    pub kind: StyleKind,
    pub based_on: Option<String>,
    pub next: Option<String>,
    /// Whether this is the default style of its kind.
    pub is_default: bool,
    pub paragraph: ParagraphProperties,
    pub run: RunProperties,
    pub table: Option<TableProperties>,
}

impl Style {
    pub fn new(id: impl Into<String>, kind: StyleKind) -> Self {
        Self {
            id: id.into(),
            name: None,
            kind,
            based_on: None,
            next: None,
            is_default: false,
            paragraph: ParagraphProperties::default(),
            run: RunProperties::default(),
            table: None,
        }
    }
}

/// The style sheet and document defaults, with property resolution.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Styles {
    pub default_paragraph: ParagraphProperties,
    pub default_run: RunProperties,
    pub styles: Vec<Style>,
    #[cfg_attr(feature = "serde", serde(skip))]
    index: HashMap<String, usize>,
}

const MAX_CHAIN: usize = 32;

impl Styles {
    pub fn new(
        default_paragraph: ParagraphProperties,
        default_run: RunProperties,
        styles: Vec<Style>,
    ) -> Self {
        let index = styles
            .iter()
            .enumerate()
            .map(|(i, style)| (style.id.clone(), i))
            .collect();
        Self {
            default_paragraph,
            default_run,
            styles,
            index,
        }
    }

    pub fn push(&mut self, style: Style) {
        self.index.insert(style.id.clone(), self.styles.len());
        self.styles.push(style);
    }

    pub fn get(&self, id: &str) -> Option<&Style> {
        self.index.get(id).map(|&i| &self.styles[i])
    }

    /// The default style of a kind, such as `Normal` for paragraphs.
    pub fn default_of(&self, kind: StyleKind) -> Option<&Style> {
        self.styles
            .iter()
            .find(|style| style.kind == kind && style.is_default)
    }

    /// A style and its `based_on` ancestors, root first. Cycles and chains
    /// longer than 32 styles are cut.
    pub fn chain(&self, id: &str) -> Vec<&Style> {
        let mut chain: Vec<&Style> = Vec::new();
        let mut next = self.get(id);
        while let Some(style) = next {
            if chain.len() == MAX_CHAIN || chain.iter().any(|seen| seen.id == style.id) {
                break;
            }
            chain.push(style);
            next = style
                .based_on
                .as_deref()
                .and_then(|parent| self.get(parent));
        }
        chain.reverse();
        chain
    }

    fn paragraph_style_id<'a>(&'a self, paragraph: &'a Paragraph) -> Option<&'a str> {
        paragraph.style_id.as_deref().or_else(|| {
            self.default_of(crate::StyleKind::Paragraph)
                .map(|style| style.id.as_str())
        })
    }

    /// The effective paragraph properties: document defaults, the paragraph
    /// style chain, the list level's properties, then direct formatting.
    pub fn resolve_paragraph(
        &self,
        paragraph: &Paragraph,
        numbering: &Numbering,
    ) -> ParagraphProperties {
        self.resolve_paragraph_in(paragraph, numbering, None)
    }

    /// [`Styles::resolve_paragraph`] for a paragraph inside a table with
    /// the given table style, whose paragraph properties sit between the
    /// document defaults and the paragraph style (ECMA-376 Part 1 §17.7.2).
    pub fn resolve_paragraph_in(
        &self,
        paragraph: &Paragraph,
        numbering: &Numbering,
        table_style: Option<&str>,
    ) -> ParagraphProperties {
        let mut resolved = self.default_paragraph.clone();
        if let Some(id) = table_style {
            self.chain(id)
                .iter()
                .for_each(|style| resolved.apply(&style.paragraph));
        }
        let chain = self
            .paragraph_style_id(paragraph)
            .map(|id| self.chain(id))
            .unwrap_or_default();
        chain
            .iter()
            .for_each(|style| resolved.apply(&style.paragraph));
        let list = paragraph.properties.numbering.or(resolved.numbering);
        if let Some(level) = list.and_then(|reference| numbering.level(reference)) {
            resolved.apply(&level.paragraph);
        }
        resolved.apply(&paragraph.properties);
        resolved
    }

    /// The effective properties of a run in a paragraph with the given
    /// style: document defaults, the paragraph style chain, the character
    /// style chain, then direct formatting. Toggle properties (bold,
    /// italic, caps, small caps, strike, double strike, vanish) set by both
    /// the paragraph and the character style cancel out, as ECMA-376 Part 1
    /// §17.7.3 specifies.
    pub fn resolve_run(
        &self,
        paragraph_style: Option<&str>,
        direct: &RunProperties,
    ) -> RunProperties {
        self.resolve_run_in(paragraph_style, direct, None)
    }

    /// [`Styles::resolve_run`] for a run inside a table with the given
    /// table style, whose run properties sit between the document defaults
    /// and the paragraph style (ECMA-376 Part 1 §17.7.2). Its toggle
    /// properties take part in the cancellation of §17.7.3.
    pub fn resolve_run_in(
        &self,
        paragraph_style: Option<&str>,
        direct: &RunProperties,
        table_style: Option<&str>,
    ) -> RunProperties {
        let mut resolved = self.default_run.clone();
        let paragraph_style = paragraph_style.or_else(|| {
            self.default_of(StyleKind::Paragraph)
                .map(|style| style.id.as_str())
        });
        let mut from_table = RunProperties::default();
        if let Some(id) = table_style {
            self.chain(id)
                .iter()
                .for_each(|style| from_table.apply(&style.run));
        }
        let mut from_paragraph = RunProperties::default();
        if let Some(id) = paragraph_style {
            self.chain(id)
                .iter()
                .for_each(|style| from_paragraph.apply(&style.run));
        }
        let mut from_character = RunProperties::default();
        if let Some(id) = direct.style_id.as_deref() {
            self.chain(id)
                .iter()
                .for_each(|style| from_character.apply(&style.run));
        }
        let mut styled = from_table.clone();
        styled.set_toggles(from_table.toggles_xor(&from_paragraph));
        let toggles = styled.toggles_xor(&from_character);
        resolved.apply(&from_table);
        resolved.apply(&from_paragraph);
        resolved.apply(&from_character);
        resolved.set_toggles(toggles);
        resolved.apply(direct);
        resolved
    }

    /// The heading level (0 for `Heading 1`) of a paragraph: its resolved
    /// outline level, else a style named `heading N` or `Title`.
    pub fn heading_level(&self, paragraph: &Paragraph, numbering: &Numbering) -> Option<u8> {
        let level = self.resolve_paragraph(paragraph, numbering).outline_level;
        if let Some(level) = level.filter(|&l| l < 9) {
            return Some(level);
        }
        let id = self.paragraph_style_id(paragraph)?;
        self.chain(id).iter().rev().find_map(|style| {
            let name = style
                .name
                .as_deref()
                .unwrap_or(&style.id)
                .to_ascii_lowercase();
            let name = name.replace(' ', "");
            if name == "title" {
                return Some(0);
            }
            let digits = name.strip_prefix("heading")?;
            let number: u8 = digits.parse().ok()?;
            (1..=9).contains(&number).then(|| number - 1)
        })
    }
}

impl ParagraphProperties {
    /// Overlays every property `over` states.
    pub fn apply(&mut self, over: &ParagraphProperties) {
        overlay(&mut self.justification, &over.justification);
        overlay(&mut self.indentation.left, &over.indentation.left);
        overlay(&mut self.indentation.right, &over.indentation.right);
        if over.indentation.first_line.is_some() || over.indentation.hanging.is_some() {
            self.indentation.first_line = over.indentation.first_line;
            self.indentation.hanging = over.indentation.hanging;
        }
        overlay(&mut self.spacing.before, &over.spacing.before);
        overlay(&mut self.spacing.after, &over.spacing.after);
        overlay(&mut self.spacing.line, &over.spacing.line);
        overlay(&mut self.spacing.line_rule, &over.spacing.line_rule);
        overlay(&mut self.spacing.before_auto, &over.spacing.before_auto);
        overlay(&mut self.spacing.after_auto, &over.spacing.after_auto);
        overlay(&mut self.keep_next, &over.keep_next);
        overlay(&mut self.keep_lines, &over.keep_lines);
        overlay(&mut self.page_break_before, &over.page_break_before);
        overlay(&mut self.widow_control, &over.widow_control);
        overlay(&mut self.contextual_spacing, &over.contextual_spacing);
        overlay(&mut self.numbering, &over.numbering);
        overlay(&mut self.outline_level, &over.outline_level);
        overlay(&mut self.borders, &over.borders);
        overlay(&mut self.shading, &over.shading);
        overlay(&mut self.bidi, &over.bidi);
        for stop in &over.tabs {
            self.tabs
                .retain(|existing| existing.position != stop.position);
            if stop.alignment != crate::TabAlignment::Clear {
                self.tabs.push(*stop);
            }
        }
        self.tabs.sort_by_key(|stop| stop.position);
    }

    /// The first-line offset from the left indent in twips: positive for a
    /// first-line indent, negative for a hanging one.
    pub fn first_line_offset(&self) -> i32 {
        if let Some(hanging) = self.indentation.hanging {
            return -hanging;
        }
        self.indentation.first_line.unwrap_or(0)
    }
}

impl RunProperties {
    /// Overlays every property `over` states.
    pub fn apply(&mut self, over: &RunProperties) {
        overlay(&mut self.style_id, &over.style_id);
        overlay(&mut self.bold, &over.bold);
        overlay(&mut self.italic, &over.italic);
        overlay(&mut self.underline, &over.underline);
        overlay(&mut self.strike, &over.strike);
        overlay(&mut self.double_strike, &over.double_strike);
        overlay(&mut self.caps, &over.caps);
        overlay(&mut self.small_caps, &over.small_caps);
        overlay(&mut self.vanish, &over.vanish);
        overlay(&mut self.fonts.ascii, &over.fonts.ascii);
        overlay(&mut self.fonts.high_ansi, &over.fonts.high_ansi);
        overlay(&mut self.fonts.east_asia, &over.fonts.east_asia);
        overlay(&mut self.fonts.complex, &over.fonts.complex);
        overlay(&mut self.size, &over.size);
        overlay(&mut self.color, &over.color);
        overlay(&mut self.highlight, &over.highlight);
        overlay(&mut self.shading, &over.shading);
        overlay(&mut self.vertical_align, &over.vertical_align);
        overlay(&mut self.spacing, &over.spacing);
        overlay(&mut self.position, &over.position);
        overlay(&mut self.language, &over.language);
        overlay(&mut self.right_to_left, &over.right_to_left);
    }

    fn toggles(&self) -> [Option<bool>; 7] {
        [
            self.bold,
            self.italic,
            self.caps,
            self.small_caps,
            self.strike,
            self.double_strike,
            self.vanish,
        ]
    }

    fn toggles_xor(&self, other: &RunProperties) -> [Option<bool>; 7] {
        let mut out = [None; 7];
        for (slot, (a, b)) in out
            .iter_mut()
            .zip(self.toggles().into_iter().zip(other.toggles()))
        {
            *slot = match (a, b) {
                (Some(a), Some(b)) => Some(a ^ b),
                (a, b) => a.or(b),
            };
        }
        out
    }

    fn set_toggles(&mut self, toggles: [Option<bool>; 7]) {
        let slots = [
            &mut self.bold,
            &mut self.italic,
            &mut self.caps,
            &mut self.small_caps,
            &mut self.strike,
            &mut self.double_strike,
            &mut self.vanish,
        ];
        for (slot, value) in slots.into_iter().zip(toggles) {
            if value.is_some() {
                *slot = value;
            }
        }
    }

    pub fn is_bold(&self) -> bool {
        self.bold.unwrap_or(false)
    }

    pub fn is_italic(&self) -> bool {
        self.italic.unwrap_or(false)
    }

    pub fn is_hidden(&self) -> bool {
        self.vanish.unwrap_or(false)
    }

    /// Font size in points; 10 when unstated, the WordprocessingML default.
    pub fn size_points(&self) -> f32 {
        self.size.map_or(10.0, |half| half as f32 / 2.0)
    }
}

fn overlay<T: Clone>(target: &mut Option<T>, over: &Option<T>) {
    if let Some(value) = over {
        *target = Some(value.clone());
    }
}
