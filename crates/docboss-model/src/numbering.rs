use std::collections::HashMap;

use crate::{Justification, NumberingRef, ParagraphProperties, RunProperties};

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum NumberFormat {
    Decimal,
    DecimalZero,
    UpperRoman,
    LowerRoman,
    UpperLetter,
    LowerLetter,
    Ordinal,
    Bullet,
    None,
    /// A format docboss does not number with; labels fall back to decimal.
    Other(String),
}

/// One level of a list definition.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Level {
    pub level: u8,
    pub start: u32,
    pub format: NumberFormat,
    /// The label template: `%1.` is replaced by level 0's number, and so on.
    /// For bullets it is the bullet character itself.
    pub text: String,
    pub justification: Option<Justification>,
    /// The level at which this counter restarts; `None` restarts on any
    /// higher level.
    pub restart_after: Option<u8>,
    /// Legal numbering: every level shows as decimal.
    pub legal: bool,
    pub paragraph: ParagraphProperties,
    pub run: RunProperties,
}

impl Default for Level {
    fn default() -> Self {
        Self {
            level: 0,
            start: 1,
            format: NumberFormat::Decimal,
            text: "%1.".to_string(),
            justification: None,
            restart_after: None,
            legal: false,
            paragraph: ParagraphProperties::default(),
            run: RunProperties::default(),
        }
    }
}

/// An abstract list definition: the levels a numbering instance uses.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct AbstractNumbering {
    pub id: i64,
    pub levels: Vec<Level>,
}

/// A numbering instance a paragraph refers to by `num_id`, with optional
/// per-level overrides of its abstract definition.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct NumberingInstance {
    pub num_id: i64,
    pub abstract_id: i64,
    pub start_overrides: Vec<(u8, u32)>,
    pub level_overrides: Vec<Level>,
}

#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Numbering {
    pub abstracts: Vec<AbstractNumbering>,
    pub instances: Vec<NumberingInstance>,
}

impl Numbering {
    pub fn instance(&self, num_id: i64) -> Option<&NumberingInstance> {
        self.instances
            .iter()
            .find(|instance| instance.num_id == num_id)
    }

    /// The effective definition of one level of a numbering instance.
    pub fn level(&self, reference: NumberingRef) -> Option<&Level> {
        let instance = self.instance(reference.num_id)?;
        if let Some(level) = instance
            .level_overrides
            .iter()
            .find(|l| l.level == reference.level)
        {
            return Some(level);
        }
        self.abstracts
            .iter()
            .find(|definition| definition.id == instance.abstract_id)?
            .levels
            .iter()
            .find(|level| level.level == reference.level)
    }

    fn start(&self, reference: NumberingRef) -> u32 {
        let overridden = self.instance(reference.num_id).and_then(|instance| {
            instance
                .start_overrides
                .iter()
                .find(|(level, _)| *level == reference.level)
        });
        match overridden {
            Some((_, start)) => *start,
            None => self.level(reference).map_or(1, |level| level.start),
        }
    }

    /// A counter that produces list labels in document order.
    pub fn counter(&self) -> NumberingCounter<'_> {
        NumberingCounter {
            numbering: self,
            counts: HashMap::new(),
        }
    }
}

/// Walks list paragraphs in document order and produces their labels.
/// Counters are kept per abstract definition, so two instances of one
/// definition continue each other unless a start override restarts them.
pub struct NumberingCounter<'a> {
    numbering: &'a Numbering,
    counts: HashMap<i64, [Option<u32>; 9]>,
}

impl NumberingCounter<'_> {
    /// Advances the counter for a list paragraph and returns its label, such
    /// as `2.1.` or `•`; `None` when the reference names no list.
    pub fn next(&mut self, reference: NumberingRef) -> Option<String> {
        let level_index = usize::from(reference.level.min(8));
        let instance = self.numbering.instance(reference.num_id)?;
        let level = self.numbering.level(reference)?.clone();
        let starts: Vec<u32> = (0..9u8)
            .map(|l| {
                self.numbering.start(NumberingRef {
                    num_id: reference.num_id,
                    level: l,
                })
            })
            .collect();
        let counts = self.counts.entry(instance.abstract_id).or_insert([None; 9]);
        let current = counts[level_index].map_or(starts[level_index], |n| n + 1);
        counts[level_index] = Some(current);
        counts[level_index + 1..].fill(None);
        if level.format == NumberFormat::None {
            return Some(String::new());
        }
        if level.format == NumberFormat::Bullet {
            return Some(level.text.clone());
        }
        let snapshot = *counts;
        let mut label = String::new();
        let mut chars = level.text.chars().peekable();
        while let Some(c) = chars.next() {
            let digit = chars.peek().and_then(|d| d.to_digit(10));
            if c != '%' || digit.is_none() {
                label.push(c);
                continue;
            }
            chars.next();
            let referenced = (digit.unwrap_or(1).max(1) - 1) as usize;
            let value = snapshot
                .get(referenced)
                .copied()
                .flatten()
                .unwrap_or(starts[referenced.min(8)]);
            let format = self
                .numbering
                .level(NumberingRef {
                    num_id: reference.num_id,
                    level: referenced as u8,
                })
                .map_or(NumberFormat::Decimal, |l| l.format.clone());
            let format = if level.legal {
                NumberFormat::Decimal
            } else {
                format
            };
            label.push_str(&crate::number_label(&format, value));
        }
        Some(label)
    }
}
