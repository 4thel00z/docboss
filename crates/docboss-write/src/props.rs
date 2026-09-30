//! Paragraph, run, table and section properties in schema order
//! (ECMA-376 Part 1 §17.3.1 paragraph properties, §17.3.2 run properties,
//! §17.4 table properties, §17.6 section properties).

use docboss_model::{
    Border, BorderStyle, Borders, Color, Justification, LineRule, NumberFormat,
    ParagraphProperties, RunProperties, SectionBreak, SectionProperties, Shading, TabAlignment,
    TabLeader, TableCellProperties, TableProperties, TableRowProperties, Underline, VerticalAlign,
    VerticalMerge,
};

use crate::xml::Xml;

fn toggle(xml: &mut Xml, name: &str, value: Option<bool>) {
    match value {
        Some(true) => xml.empty(name, &[]),
        Some(false) => xml.val(name, "0"),
        None => {}
    }
}

fn number(xml: &mut Xml, name: &str, value: Option<impl ToString>) {
    if let Some(value) = value {
        xml.val(name, &value.to_string());
    }
}

pub fn justification_name(value: Justification) -> &'static str {
    match value {
        Justification::Left => "left",
        Justification::Center => "center",
        Justification::Right => "right",
        Justification::Both => "both",
        Justification::Distribute => "distribute",
    }
}

fn underline_name(value: Underline) -> &'static str {
    match value {
        Underline::None => "none",
        Underline::Single | Underline::Other => "single",
        Underline::Double => "double",
        Underline::Thick => "thick",
        Underline::Dotted => "dotted",
        Underline::Dashed => "dash",
        Underline::Wave => "wave",
        Underline::Words => "words",
    }
}

const HIGHLIGHTS: [(&str, Color); 16] = [
    ("black", Color(0, 0, 0)),
    ("blue", Color(0, 0, 255)),
    ("cyan", Color(0, 255, 255)),
    ("green", Color(0, 255, 0)),
    ("magenta", Color(255, 0, 255)),
    ("red", Color(255, 0, 0)),
    ("yellow", Color(255, 255, 0)),
    ("white", Color(255, 255, 255)),
    ("darkBlue", Color(0, 0, 128)),
    ("darkCyan", Color(0, 128, 128)),
    ("darkGreen", Color(0, 128, 0)),
    ("darkMagenta", Color(128, 0, 128)),
    ("darkRed", Color(128, 0, 0)),
    ("darkYellow", Color(128, 128, 0)),
    ("darkGray", Color(128, 128, 128)),
    ("lightGray", Color(192, 192, 192)),
];

/// The name of the highlight palette entry nearest to `color`
/// (ECMA-376 Part 1 §17.18.40).
pub fn highlight_name(color: Color) -> &'static str {
    let distance = |c: Color| {
        let d = |a: u8, b: u8| (i32::from(a) - i32::from(b)).pow(2);
        d(c.0, color.0) + d(c.1, color.1) + d(c.2, color.2)
    };
    HIGHLIGHTS
        .iter()
        .min_by_key(|(_, c)| distance(*c))
        .map_or("yellow", |(name, _)| name)
}

fn color_value(color: Option<Color>) -> String {
    color.map_or_else(|| "auto".to_string(), Color::to_hex)
}

/// Writes `<w:rPr>`; `with_style` is false inside style definitions.
pub fn run_properties(xml: &mut Xml, props: &RunProperties, with_style: bool) {
    let mut inner = Xml { out: String::new() };
    run_properties_inner(&mut inner, props, with_style);
    if inner.out.is_empty() {
        return;
    }
    xml.open("w:rPr", &[]);
    xml.out.push_str(&inner.out);
    xml.close("w:rPr");
}

fn run_properties_inner(xml: &mut Xml, props: &RunProperties, with_style: bool) {
    if let Some(style) = props.style_id.as_deref().filter(|_| with_style) {
        xml.val("w:rStyle", style);
    }
    let fonts = &props.fonts;
    let slots = [
        ("w:ascii", fonts.ascii.as_deref()),
        ("w:hAnsi", fonts.high_ansi.as_deref()),
        ("w:eastAsia", fonts.east_asia.as_deref()),
        ("w:cs", fonts.complex.as_deref()),
    ];
    let slots: Vec<(&str, &str)> = slots
        .iter()
        .filter_map(|(k, v)| v.map(|v| (*k, v)))
        .collect();
    if !slots.is_empty() {
        xml.empty("w:rFonts", &slots);
    }
    toggle(xml, "w:b", props.bold);
    toggle(xml, "w:bCs", props.bold_complex.or(props.bold));
    toggle(xml, "w:i", props.italic);
    toggle(xml, "w:iCs", props.italic_complex.or(props.italic));
    toggle(xml, "w:caps", props.caps);
    toggle(xml, "w:smallCaps", props.small_caps);
    toggle(xml, "w:strike", props.strike);
    toggle(xml, "w:dstrike", props.double_strike);
    toggle(xml, "w:vanish", props.vanish);
    if let Some(color) = props.color {
        xml.val("w:color", &color_value(color));
    }
    number(xml, "w:spacing", props.spacing);
    number(xml, "w:position", props.position);
    number(xml, "w:sz", props.size);
    number(xml, "w:szCs", props.size_complex.or(props.size));
    if let Some(highlight) = props.highlight {
        xml.val("w:highlight", highlight_name(highlight));
    }
    if let Some(underline) = props.underline {
        xml.val("w:u", underline_name(underline));
    }
    if let Some(shading) = props.shading {
        shading_element(xml, "w:shd", shading);
    }
    if let Some(align) = props.vertical_align {
        let name = match align {
            VerticalAlign::Superscript => "superscript",
            VerticalAlign::Subscript => "subscript",
            _ => "baseline",
        };
        xml.val("w:vertAlign", name);
    }
    toggle(xml, "w:rtl", props.right_to_left);
    toggle(xml, "w:cs", props.complex_script);
    if let Some(language) = &props.language {
        xml.val("w:lang", language);
    }
}

pub fn shading_element(xml: &mut Xml, name: &str, shading: Shading) {
    xml.empty(
        name,
        &[
            ("w:val", "clear"),
            ("w:color", "auto"),
            ("w:fill", &color_value(shading.fill)),
        ],
    );
}

fn border_style_name(style: BorderStyle) -> &'static str {
    match style {
        BorderStyle::None => "none",
        BorderStyle::Single | BorderStyle::Other | BorderStyle::Art => "single",
        BorderStyle::Thick => "thick",
        BorderStyle::Double => "double",
        BorderStyle::Dotted => "dotted",
        BorderStyle::Dashed => "dashed",
        BorderStyle::DotDash => "dotDash",
        BorderStyle::DotDotDash => "dotDotDash",
        BorderStyle::DashSmallGap => "dashSmallGap",
        BorderStyle::DashDotStroked => "dashDotStroked",
    }
}

fn border(xml: &mut Xml, name: &str, border: Option<Border>) {
    let Some(border) = border else {
        return;
    };
    let (size, space, color) = (
        border.size.to_string(),
        border.space.to_string(),
        color_value(border.color),
    );
    let attrs = [
        ("w:val", border_style_name(border.style)),
        ("w:sz", size.as_str()),
        ("w:space", space.as_str()),
        ("w:color", color.as_str()),
        ("w:shadow", "1"),
    ];
    let count = if border.shadow { 5 } else { 4 };
    xml.empty(name, &attrs[..count]);
}

/// Writes a border group; `between` names the inner horizontal edge
/// (`w:between` for paragraphs, `w:insideH` for tables).
pub fn borders(xml: &mut Xml, name: &str, value: &Borders, between: &str) {
    xml.open(name, &[]);
    border(xml, "w:top", value.top);
    border(xml, "w:left", value.left);
    border(xml, "w:bottom", value.bottom);
    border(xml, "w:right", value.right);
    border(xml, between, value.inside_horizontal);
    if between == "w:insideH" {
        border(xml, "w:insideV", value.inside_vertical);
    }
    xml.close(name);
}

fn tab_alignment(value: TabAlignment) -> &'static str {
    match value {
        TabAlignment::Left => "left",
        TabAlignment::Center => "center",
        TabAlignment::Right => "right",
        TabAlignment::Decimal => "decimal",
        TabAlignment::Bar => "bar",
        TabAlignment::Clear => "clear",
    }
}

fn tab_leader(value: TabLeader) -> &'static str {
    match value {
        TabLeader::None => "none",
        TabLeader::Dot => "dot",
        TabLeader::Hyphen => "hyphen",
        TabLeader::Underscore => "underscore",
        TabLeader::MiddleDot => "middleDot",
        TabLeader::Heavy => "heavy",
    }
}

/// Writes `<w:pPr>` with the style reference, the paragraph mark's run
/// properties and a pre-rendered `<w:sectPr>` in their schema positions.
pub fn paragraph_properties(
    xml: &mut Xml,
    style_id: Option<&str>,
    props: &ParagraphProperties,
    mark: Option<&RunProperties>,
    section: Option<&str>,
) {
    let mut inner = Xml { out: String::new() };
    paragraph_properties_inner(&mut inner, style_id, props);
    if let Some(mark) = mark {
        run_properties(&mut inner, mark, true);
    }
    if let Some(section) = section {
        inner.out.push_str(section);
    }
    if inner.out.is_empty() {
        return;
    }
    xml.open("w:pPr", &[]);
    xml.out.push_str(&inner.out);
    xml.close("w:pPr");
}

fn paragraph_properties_inner(xml: &mut Xml, style_id: Option<&str>, props: &ParagraphProperties) {
    if let Some(style) = style_id {
        xml.val("w:pStyle", style);
    }
    toggle(xml, "w:keepNext", props.keep_next);
    toggle(xml, "w:keepLines", props.keep_lines);
    toggle(xml, "w:pageBreakBefore", props.page_break_before);
    toggle(xml, "w:widowControl", props.widow_control);
    if let Some(list) = props.numbering {
        xml.open("w:numPr", &[]);
        xml.val("w:ilvl", &list.level.to_string());
        xml.val("w:numId", &list.num_id.to_string());
        xml.close("w:numPr");
    }
    if let Some(value) = &props.borders {
        borders(xml, "w:pBdr", value, "w:between");
    }
    if let Some(shading) = props.shading {
        shading_element(xml, "w:shd", shading);
    }
    if !props.tabs.is_empty() {
        xml.open("w:tabs", &[]);
        for stop in &props.tabs {
            let position = stop.position.to_string();
            let mut attributes = vec![("w:val", tab_alignment(stop.alignment))];
            if stop.leader != TabLeader::None {
                attributes.push(("w:leader", tab_leader(stop.leader)));
            }
            attributes.push(("w:pos", &position));
            xml.empty("w:tab", &attributes);
        }
        xml.close("w:tabs");
    }
    toggle(xml, "w:bidi", props.bidi);
    spacing(xml, props);
    indentation(xml, props);
    toggle(xml, "w:contextualSpacing", props.contextual_spacing);
    if let Some(value) = props.justification {
        xml.val("w:jc", justification_name(value));
    }
    number(xml, "w:outlineLvl", props.outline_level);
}

fn spacing(xml: &mut Xml, props: &ParagraphProperties) {
    let s = &props.spacing;
    let mut attributes: Vec<(&str, String)> = Vec::new();
    let push = |attributes: &mut Vec<(&str, String)>, key, value: Option<String>| {
        if let Some(value) = value {
            attributes.push((key, value));
        }
    };
    push(&mut attributes, "w:before", s.before.map(|v| v.to_string()));
    push(
        &mut attributes,
        "w:beforeAutospacing",
        s.before_auto.map(|v| u8::from(v).to_string()),
    );
    push(&mut attributes, "w:after", s.after.map(|v| v.to_string()));
    push(
        &mut attributes,
        "w:afterAutospacing",
        s.after_auto.map(|v| u8::from(v).to_string()),
    );
    push(&mut attributes, "w:line", s.line.map(|v| v.to_string()));
    let rule = s.line_rule.map(|rule| match rule {
        LineRule::Auto => "auto".to_string(),
        LineRule::Exact => "exact".to_string(),
        LineRule::AtLeast => "atLeast".to_string(),
    });
    push(&mut attributes, "w:lineRule", rule);
    if attributes.is_empty() {
        return;
    }
    let borrowed: Vec<(&str, &str)> = attributes.iter().map(|(k, v)| (*k, v.as_str())).collect();
    xml.empty("w:spacing", &borrowed);
}

fn indentation(xml: &mut Xml, props: &ParagraphProperties) {
    let ind = &props.indentation;
    let values = [
        ("w:left", ind.left),
        ("w:right", ind.right),
        ("w:firstLine", ind.first_line),
        ("w:hanging", ind.hanging),
    ];
    let values: Vec<(&str, String)> = values
        .iter()
        .filter_map(|(k, v)| v.map(|v| (*k, v.to_string())))
        .collect();
    if values.is_empty() {
        return;
    }
    let borrowed: Vec<(&str, &str)> = values.iter().map(|(k, v)| (*k, v.as_str())).collect();
    xml.empty("w:ind", &borrowed);
}

fn margins(xml: &mut Xml, name: &str, values: [i32; 4]) {
    xml.open(name, &[]);
    for (side, value) in ["w:top", "w:left", "w:bottom", "w:right"]
        .into_iter()
        .zip(values)
    {
        xml.empty(side, &[("w:w", &value.to_string()), ("w:type", "dxa")]);
    }
    xml.close(name);
}

pub fn table_properties(xml: &mut Xml, props: &TableProperties) {
    xml.open("w:tblPr", &[]);
    if let Some(style) = &props.style_id {
        xml.val("w:tblStyle", style);
    }
    if props.bidi_visual {
        xml.empty("w:bidiVisual", &[]);
    }
    match (props.width, props.width_pct) {
        (Some(width), _) => xml.empty("w:tblW", &[("w:w", &width.to_string()), ("w:type", "dxa")]),
        (None, Some(pct)) => xml.empty("w:tblW", &[("w:w", &pct.to_string()), ("w:type", "pct")]),
        (None, None) => xml.empty("w:tblW", &[("w:w", "0"), ("w:type", "auto")]),
    }
    if let Some(value) = props.justification {
        xml.val("w:jc", justification_name(value));
    }
    if let Some(indent) = props.indent {
        xml.empty(
            "w:tblInd",
            &[("w:w", &indent.to_string()), ("w:type", "dxa")],
        );
    }
    if let Some(value) = &props.borders {
        borders(xml, "w:tblBorders", value, "w:insideH");
    }
    if let Some(shading) = props.shading {
        shading_element(xml, "w:shd", shading);
    }
    if props.fixed_layout {
        xml.empty("w:tblLayout", &[("w:type", "fixed")]);
    }

    if let Some(values) = props.cell_margins {
        margins(xml, "w:tblCellMar", values);
    }
    xml.close("w:tblPr");
}

pub fn row_properties(xml: &mut Xml, props: &TableRowProperties) {
    if props.height.is_none() && !props.header && !props.cant_split {
        return;
    }
    xml.open("w:trPr", &[]);
    if props.cant_split {
        xml.empty("w:cantSplit", &[]);
    }
    if let Some(height) = props.height {
        let rule = if props.height_exact {
            "exact"
        } else {
            "atLeast"
        };
        xml.empty(
            "w:trHeight",
            &[("w:val", &height.to_string()), ("w:hRule", rule)],
        );
    }
    if props.header {
        xml.empty("w:tblHeader", &[]);
    }
    xml.close("w:trPr");
}

pub fn cell_properties(xml: &mut Xml, props: &TableCellProperties) {
    xml.open("w:tcPr", &[]);
    match props.width {
        Some(width) => xml.empty("w:tcW", &[("w:w", &width.to_string()), ("w:type", "dxa")]),
        None => xml.empty("w:tcW", &[("w:w", "0"), ("w:type", "auto")]),
    }
    if props.grid_span > 1 {
        xml.val("w:gridSpan", &props.grid_span.to_string());
    }
    match props.vertical_merge {
        Some(VerticalMerge::Restart) => xml.val("w:vMerge", "restart"),
        Some(VerticalMerge::Continue) => xml.empty("w:vMerge", &[]),
        None => {}
    }
    if let Some(value) = &props.borders {
        borders(xml, "w:tcBorders", value, "w:insideH");
    }
    if let Some(shading) = props.shading {
        shading_element(xml, "w:shd", shading);
    }
    if let Some(values) = props.margins {
        margins(xml, "w:tcMar", values);
    }
    if let Some(align) = props.vertical_align {
        let name = match align {
            VerticalAlign::Center => "center",
            VerticalAlign::Bottom => "bottom",
            _ => "top",
        };
        xml.val("w:vAlign", name);
    }
    xml.close("w:tcPr");
}

/// Renders `<w:sectPr>`; `relationship` maps a header or footer id of the
/// model to the relationship id written for it.
pub fn section_properties(
    props: &SectionProperties,
    relationship: &dyn Fn(&str) -> Option<String>,
) -> String {
    let mut xml = Xml { out: String::new() };
    xml.open("w:sectPr", &[]);
    let references = [
        ("w:headerReference", &props.headers),
        ("w:footerReference", &props.footers),
    ];
    for (name, refs) in references {
        let kinds = [
            ("default", &refs.default),
            ("first", &refs.first),
            ("even", &refs.even),
        ];
        for (kind, id) in kinds {
            let Some(rid) = id.as_deref().and_then(relationship) else {
                continue;
            };
            xml.empty(name, &[("w:type", kind), ("r:id", &rid)]);
        }
    }
    let start = match props.start {
        SectionBreak::NextPage => "nextPage",
        SectionBreak::Continuous => "continuous",
        SectionBreak::EvenPage => "evenPage",
        SectionBreak::OddPage => "oddPage",
        SectionBreak::NextColumn => "nextColumn",
    };
    xml.val("w:type", start);
    let size = props.page_size;
    let (width, height) = (size.width.to_string(), size.height.to_string());
    let mut page = vec![("w:w", width.as_str()), ("w:h", height.as_str())];
    if size.orientation == docboss_model::Orientation::Landscape {
        page.push(("w:orient", "landscape"));
    }
    xml.empty("w:pgSz", &page);
    let m = props.margins;
    let values: Vec<String> = [
        m.top, m.right, m.bottom, m.left, m.header, m.footer, m.gutter,
    ]
    .iter()
    .map(i32::to_string)
    .collect();
    let keys = [
        "w:top", "w:right", "w:bottom", "w:left", "w:header", "w:footer", "w:gutter",
    ];
    let margins: Vec<(&str, &str)> = keys
        .into_iter()
        .zip(values.iter().map(String::as_str))
        .collect();
    xml.empty("w:pgMar", &margins);
    if let Some(value) = &props.page_borders {
        page_borders(&mut xml, value);
    }
    if let Some(start) = props.page_number_start {
        xml.empty("w:pgNumType", &[("w:start", &start.to_string())]);
    }
    columns(&mut xml, &props.columns);
    if props.title_page {
        xml.empty("w:titlePg", &[]);
    }
    xml.close("w:sectPr");
    xml.out
}

/// ECMA-376 Part 1 §17.6.10: `w:pgBorders` with its sides.
fn page_borders(xml: &mut Xml, value: &docboss_model::PageBorders) {
    let offset = match value.offset_from {
        docboss_model::PageBorderOffset::Page => "page",
        docboss_model::PageBorderOffset::Text => "text",
    };
    let display = match value.display {
        docboss_model::PageBorderDisplay::AllPages => "allPages",
        docboss_model::PageBorderDisplay::FirstPage => "firstPage",
        docboss_model::PageBorderDisplay::NotFirstPage => "notFirstPage",
    };
    let z_order = if value.behind_text { "back" } else { "front" };
    xml.open(
        "w:pgBorders",
        &[
            ("w:offsetFrom", offset),
            ("w:display", display),
            ("w:zOrder", z_order),
        ],
    );
    border(xml, "w:top", value.sides.top);
    border(xml, "w:left", value.sides.left);
    border(xml, "w:bottom", value.sides.bottom);
    border(xml, "w:right", value.sides.right);
    xml.close("w:pgBorders");
}

fn columns(xml: &mut Xml, columns: &docboss_model::Columns) {
    let count = columns.count.max(1).to_string();
    let space = columns.space.to_string();
    let mut attributes = vec![("w:space", space.as_str()), ("w:num", count.as_str())];
    if columns.separator {
        attributes.push(("w:sep", "1"));
    }
    if columns.widths.is_empty() {
        xml.empty("w:cols", &attributes);
        return;
    }
    attributes.push(("w:equalWidth", "0"));
    xml.open("w:cols", &attributes);
    for (width, space) in &columns.widths {
        xml.empty(
            "w:col",
            &[("w:w", &width.to_string()), ("w:space", &space.to_string())],
        );
    }
    xml.close("w:cols");
}

pub fn number_format_name(format: &NumberFormat) -> &str {
    match format {
        NumberFormat::Decimal => "decimal",
        NumberFormat::DecimalZero => "decimalZero",
        NumberFormat::UpperRoman => "upperRoman",
        NumberFormat::LowerRoman => "lowerRoman",
        NumberFormat::UpperLetter => "upperLetter",
        NumberFormat::LowerLetter => "lowerLetter",
        NumberFormat::Ordinal => "ordinal",
        NumberFormat::Bullet => "bullet",
        NumberFormat::None => "none",
        NumberFormat::Other(name) => name,
    }
}
