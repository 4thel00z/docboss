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
    /// Orders instances and abstract definitions by id, keeping the first of
    /// equal ids first, so lookups binary-search instead of scanning.
    pub fn sort_by_id(&mut self) {
        self.instances.sort_by_key(|instance| instance.num_id);
        self.abstracts.sort_by_key(|definition| definition.id);
    }

    /// The first instance with `num_id`. A binary search finds it when the
    /// instances are sorted ([`Numbering::sort_by_id`]); otherwise the scan
    /// that follows does.
    pub fn instance(&self, num_id: i64) -> Option<&NumberingInstance> {
        let at = self
            .instances
            .partition_point(|instance| instance.num_id < num_id);
        if let Some(instance) = self
            .instances
            .get(at)
            .filter(|instance| instance.num_id == num_id)
        {
            return Some(instance);
        }
        self.instances
            .iter()
            .find(|instance| instance.num_id == num_id)
    }

    /// The first abstract definition with `id`, found like [`Numbering::instance`].
    pub fn definition(&self, id: i64) -> Option<&AbstractNumbering> {
        let at = self
            .abstracts
            .partition_point(|definition| definition.id < id);
        if let Some(definition) = self
            .abstracts
            .get(at)
            .filter(|definition| definition.id == id)
        {
            return Some(definition);
        }
        self.abstracts.iter().find(|definition| definition.id == id)
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
        self.definition(instance.abstract_id)?
            .levels
            .iter()
            .find(|level| level.level == reference.level)
    }

    /// A counter that produces list labels in document order.
    pub fn counter(&self) -> NumberingCounter<'_> {
        let instances = self
            .instances
            .iter()
            .enumerate()
            .rev()
            .map(|(index, instance)| (instance.num_id, index))
            .collect();
        let abstracts = self
            .abstracts
            .iter()
            .enumerate()
            .rev()
            .map(|(index, definition)| (definition.id, index))
            .collect();
        NumberingCounter {
            numbering: self,
            instances,
            abstracts,
            resolved: HashMap::new(),
            counts: HashMap::new(),
        }
    }
}

/// The nine levels of one numbering instance with overrides applied, and
/// each level's start value.
struct ResolvedInstance<'a> {
    abstract_id: i64,
    levels: [Option<&'a Level>; 9],
    starts: [u32; 9],
}

/// Walks list paragraphs in document order and produces their labels.
/// Counters are kept per abstract definition, so two instances of one
/// definition continue each other unless a start override restarts them.
/// Instances are resolved once, so each label costs a few map lookups
/// however many lists the document has.
pub struct NumberingCounter<'a> {
    numbering: &'a Numbering,
    instances: HashMap<i64, usize>,
    abstracts: HashMap<i64, usize>,
    resolved: HashMap<i64, ResolvedInstance<'a>>,
    counts: HashMap<i64, [Option<u32>; 9]>,
}

impl<'a> NumberingCounter<'a> {
    fn resolve(&self, num_id: i64) -> Option<ResolvedInstance<'a>> {
        let numbering: &'a Numbering = self.numbering;
        let instance = &numbering.instances[*self.instances.get(&num_id)?];
        let definition = self
            .abstracts
            .get(&instance.abstract_id)
            .map(|&index| &numbering.abstracts[index]);
        let levels: [Option<&'a Level>; 9] = std::array::from_fn(|index| {
            let wanted = index as u8;
            instance
                .level_overrides
                .iter()
                .find(|level| level.level == wanted)
                .or_else(|| {
                    definition?
                        .levels
                        .iter()
                        .find(|level| level.level == wanted)
                })
        });
        let starts = std::array::from_fn(|index| {
            instance
                .start_overrides
                .iter()
                .find(|(level, _)| usize::from(*level) == index)
                .map(|(_, start)| *start)
                .or_else(|| levels[index].map(|level| level.start))
                .unwrap_or(1)
        });
        Some(ResolvedInstance {
            abstract_id: instance.abstract_id,
            levels,
            starts,
        })
    }

    /// Advances the counter for a list paragraph and returns its label, such
    /// as `2.1.` or `•`; `None` when the reference names no list.
    pub fn next(&mut self, reference: NumberingRef) -> Option<String> {
        let level_index = usize::from(reference.level.min(8));
        if !self.resolved.contains_key(&reference.num_id) {
            let resolved = self.resolve(reference.num_id)?;
            self.resolved.insert(reference.num_id, resolved);
        }
        let resolved = self.resolved.get(&reference.num_id)?;
        let level = resolved.levels[level_index]?;
        let counts = self.counts.entry(resolved.abstract_id).or_insert([None; 9]);
        let current = counts[level_index].map_or(resolved.starts[level_index], |n| n + 1);
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
            let referenced = ((digit.unwrap_or(1).max(1) - 1) as usize).min(8);
            let value = snapshot[referenced].unwrap_or(resolved.starts[referenced]);
            let format = match level.legal {
                true => &NumberFormat::Decimal,
                false => resolved.levels[referenced].map_or(&NumberFormat::Decimal, |l| &l.format),
            };
            label.push_str(&crate::number_label(format, value));
        }
        Some(label)
    }
}
