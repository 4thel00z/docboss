use docboss_model::{
    AbstractNumbering, Level, NumberFormat, Numbering, NumberingInstance, NumberingRef, Paragraph,
    ParagraphProperties, RunProperties, Style, StyleKind, Styles,
};

fn bold(value: bool) -> RunProperties {
    RunProperties {
        bold: Some(value),
        ..RunProperties::default()
    }
}

/// ECMA-376 Part 1 §17.7.3: a toggle property set by both the paragraph
/// style and the character style cancels out; direct formatting wins.
#[test]
fn toggle_properties_cancel_across_style_kinds() {
    let mut heading = Style::new("Heading1", StyleKind::Paragraph);
    heading.run = bold(true);
    let mut strong = Style::new("Strong", StyleKind::Character);
    strong.run = bold(true);
    let styles = Styles::new(
        ParagraphProperties::default(),
        RunProperties::default(),
        vec![heading, strong],
    );

    let in_heading = RunProperties {
        style_id: Some("Strong".into()),
        ..RunProperties::default()
    };
    assert_eq!(
        styles.resolve_run(Some("Heading1"), &in_heading).bold,
        Some(false)
    );

    let direct = RunProperties {
        style_id: Some("Strong".into()),
        bold: Some(true),
        ..RunProperties::default()
    };
    assert_eq!(
        styles.resolve_run(Some("Heading1"), &direct).bold,
        Some(true)
    );
    assert_eq!(styles.resolve_run(None, &in_heading).bold, Some(true));
}

/// ECMA-376 Part 1 §17.7.4.3: a style inherits the properties of the style
/// it is based on and overrides them.
#[test]
fn based_on_chain_resolves_root_first() {
    let mut normal = Style::new("Normal", StyleKind::Paragraph);
    normal.is_default = true;
    normal.run.size = Some(22);
    normal.run.italic = Some(true);
    let mut quote = Style::new("Quote", StyleKind::Paragraph);
    quote.based_on = Some("Normal".into());
    quote.run.size = Some(28);
    let styles = Styles::new(
        ParagraphProperties::default(),
        RunProperties::default(),
        vec![normal, quote],
    );

    let resolved = styles.resolve_run(Some("Quote"), &RunProperties::default());
    assert_eq!(resolved.size, Some(28));
    assert_eq!(resolved.italic, Some(true));
    assert_eq!(
        styles.resolve_run(None, &RunProperties::default()).size,
        Some(22)
    );
}

#[test]
fn heading_level_comes_from_style_names() {
    let mut heading = Style::new("Heading2", StyleKind::Paragraph);
    heading.name = Some("heading 2".into());
    let styles = Styles::new(
        ParagraphProperties::default(),
        RunProperties::default(),
        vec![heading],
    );
    let paragraph = Paragraph {
        style_id: Some("Heading2".into()),
        ..Paragraph::default()
    };
    assert_eq!(
        styles.heading_level(&paragraph, &Numbering::default()),
        Some(1)
    );
}

fn level(level: u8, format: NumberFormat, text: &str) -> Level {
    Level {
        level,
        format,
        text: text.into(),
        ..Level::default()
    }
}

/// ECMA-376 Part 1 §17.9.11: `%1.%2.` in a level's text is replaced by the
/// current numbers of levels 0 and 1, and a deeper level restarts when a
/// shallower one advances.
#[test]
fn multilevel_labels_count_and_restart() {
    let numbering = Numbering {
        abstracts: vec![AbstractNumbering {
            id: 0,
            levels: vec![
                level(0, NumberFormat::Decimal, "%1."),
                level(1, NumberFormat::LowerLetter, "%1.%2)"),
                level(2, NumberFormat::LowerRoman, "(%3)"),
            ],
        }],
        instances: vec![NumberingInstance {
            num_id: 1,
            abstract_id: 0,
            ..NumberingInstance::default()
        }],
    };
    let mut counter = numbering.counter();
    let at = |level: u8| NumberingRef { num_id: 1, level };
    let labels: Vec<String> = [0, 1, 1, 2, 2, 0, 1]
        .iter()
        .map(|&l| counter.next(at(l)).unwrap())
        .collect();
    assert_eq!(labels, ["1.", "1.a)", "1.b)", "(i)", "(ii)", "2.", "2.a)"]);
}

#[test]
fn number_formats() {
    use docboss_model::number_label;
    assert_eq!(number_label(&NumberFormat::UpperRoman, 1994), "MCMXCIV");
    assert_eq!(number_label(&NumberFormat::LowerLetter, 28), "bb");
    assert_eq!(number_label(&NumberFormat::Ordinal, 12), "12th");
    assert_eq!(number_label(&NumberFormat::Ordinal, 22), "22nd");
    assert_eq!(number_label(&NumberFormat::DecimalZero, 7), "07");
}

/// ECMA-376 Part 1 §17.7.2: table style properties sit above the document
/// defaults and below the paragraph style.
#[test]
fn table_style_sits_between_defaults_and_paragraph_style() {
    let mut normal = Style::new("Normal", StyleKind::Paragraph);
    normal.is_default = true;
    let mut quote = Style::new("Quote", StyleKind::Paragraph);
    quote.paragraph.spacing.before = Some(120);
    let mut grid = Style::new("Grid", StyleKind::Table);
    grid.paragraph.spacing.after = Some(0);
    grid.paragraph.spacing.before = Some(60);
    grid.run.size = Some(18);
    let mut defaults = ParagraphProperties::default();
    defaults.spacing.after = Some(200);
    let styles = Styles::new(
        defaults,
        RunProperties::default(),
        vec![normal, quote, grid],
    );

    let paragraph = Paragraph {
        style_id: Some("Quote".into()),
        ..Paragraph::default()
    };
    let resolved = styles.resolve_paragraph_in(&paragraph, &Numbering::default(), Some("Grid"));
    assert_eq!(resolved.spacing.after, Some(0));
    assert_eq!(resolved.spacing.before, Some(120));
    let outside = styles.resolve_paragraph(&paragraph, &Numbering::default());
    assert_eq!(outside.spacing.after, Some(200));
    assert_eq!(
        styles
            .resolve_run_in(Some("Quote"), &RunProperties::default(), Some("Grid"))
            .size,
        Some(18)
    );
}

/// Lookups find instances whether or not the reader sorted them, and two
/// instances of one abstract definition share its counters (ECMA-376 Part 1
/// §17.9.15).
#[test]
fn instances_resolve_sorted_or_not() {
    let definition = AbstractNumbering {
        id: 7,
        levels: vec![level(0, NumberFormat::Decimal, "%1.")],
    };
    let instance = |num_id: i64| NumberingInstance {
        num_id,
        abstract_id: 7,
        ..NumberingInstance::default()
    };
    let mut numbering = Numbering {
        abstracts: vec![definition],
        instances: vec![instance(9), instance(2), instance(5)],
    };
    for sorted in [false, true] {
        if sorted {
            numbering.sort_by_id();
        }
        assert!([2, 5, 9].iter().all(|&id| numbering.instance(id).is_some()));
        assert!(numbering.instance(4).is_none());
        let mut counter = numbering.counter();
        let labels: Vec<String> = [9, 2, 5]
            .iter()
            .map(|&num_id| counter.next(NumberingRef { num_id, level: 0 }).unwrap())
            .collect();
        assert_eq!(labels, ["1.", "2.", "3."]);
    }
}
