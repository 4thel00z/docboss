//! Helpers over the XML reader for recursive-descent parsing of
//! WordprocessingML.

use std::borrow::Cow;

use docboss_model::Color;
use docboss_xml::{Element, Event, Ns, Reader};

/// Calls `f` for each child element of the element whose start was just
/// read, then consumes its end. `f` is called right after the child's start
/// and may consume as much of the child as it wants; whatever it leaves is
/// skipped.
pub fn children<'a>(reader: &mut Reader<'a>, mut f: impl FnMut(&mut Reader<'a>, Element<'a>)) {
    let depth = reader.depth();
    if depth == 0 {
        return;
    }
    loop {
        match reader.next_event() {
            Event::Start(e) => {
                f(reader, e);
                while reader.depth() > depth {
                    reader.skip();
                }
            }
            Event::End(_) if reader.depth() < depth => return,
            Event::Eof => return,
            _ => {}
        }
    }
}

/// Reads to the first start tag, the root element.
pub fn root<'a>(reader: &mut Reader<'a>) -> Option<Element<'a>> {
    loop {
        match reader.next_event() {
            Event::Start(e) => return Some(e),
            Event::Eof => return None,
            _ => {}
        }
    }
}

pub fn val<'a>(e: &Element<'a>) -> Option<Cow<'a, str>> {
    e.attr(Ns::W, "val")
}

pub fn attr<'a>(e: &Element<'a>, local: &str) -> Option<Cow<'a, str>> {
    e.attr(Ns::W, local)
}

/// An ST_OnOff value (ECMA-376 Part 1 §17.17.4): absent means on.
pub fn on_off_value(value: Option<&str>) -> bool {
    !matches!(value, Some("0" | "false" | "off" | "False" | "FALSE"))
}

pub fn on_off(e: &Element<'_>) -> bool {
    on_off_value(e.attr_raw(Ns::W, "val"))
}

pub fn int(value: &str) -> Option<i64> {
    let value = value.trim();
    if let Ok(n) = value.parse::<i64>() {
        return Some(n);
    }
    value
        .parse::<f64>()
        .ok()
        .filter(|f| f.is_finite())
        .map(|f| f.round() as i64)
}

/// A measure in twips: a bare number, or a universal measure with a unit
/// suffix as strict documents write them (ECMA-376 Part 1 §22.9.2.15).
pub fn twips(value: &str) -> Option<i32> {
    let value = value.trim();
    let units: [(&str, f64); 6] = [
        ("mm", 56.692_913),
        ("cm", 566.929_13),
        ("in", 1440.0),
        ("pt", 20.0),
        ("pc", 240.0),
        ("pi", 240.0),
    ];
    for (suffix, factor) in units {
        if let Some(number) = value.strip_suffix(suffix) {
            let n: f64 = number.trim().parse().ok()?;
            return n
                .is_finite()
                .then(|| (n * factor).round().clamp(i32::MIN as f64, i32::MAX as f64) as i32);
        }
    }
    int(value).map(|n| n.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32)
}

pub fn twips_attr(e: &Element<'_>, local: &str) -> Option<i32> {
    e.attr_raw(Ns::W, local).and_then(twips)
}

pub fn int_attr(e: &Element<'_>, local: &str) -> Option<i64> {
    e.attr_raw(Ns::W, local).and_then(int)
}

pub fn u32_attr(e: &Element<'_>, local: &str) -> Option<u32> {
    int_attr(e, local).map(|n| n.clamp(0, i64::from(u32::MAX)) as u32)
}

/// An ST_HexColor, where `auto` gives `Some(None)`.
pub fn color(value: &str) -> Option<Option<Color>> {
    if value.eq_ignore_ascii_case("auto") {
        return Some(None);
    }
    Color::from_hex(value).map(Some)
}

/// The fixed highlight palette of ST_HighlightColor (ECMA-376 Part 1
/// §17.18.40).
pub fn highlight(name: &str) -> Option<Color> {
    let rgb = match name {
        "black" => (0, 0, 0),
        "blue" => (0, 0, 255),
        "cyan" => (0, 255, 255),
        "green" => (0, 255, 0),
        "magenta" => (255, 0, 255),
        "red" => (255, 0, 0),
        "yellow" => (255, 255, 0),
        "white" => (255, 255, 255),
        "darkBlue" => (0, 0, 139),
        "darkCyan" => (0, 139, 139),
        "darkGreen" => (0, 100, 0),
        "darkMagenta" => (128, 0, 128),
        "darkRed" => (139, 0, 0),
        "darkYellow" => (128, 128, 0),
        "darkGray" => (169, 169, 169),
        "lightGray" => (211, 211, 211),
        _ => return None,
    };
    Some(Color(rgb.0, rgb.1, rgb.2))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ECMA-376 Part 1 §22.9.2.15: universal measures carry a unit.
    #[test]
    fn measures() {
        assert_eq!(twips("1440"), Some(1440));
        assert_eq!(twips("1in"), Some(1440));
        assert_eq!(twips("12pt"), Some(240));
        assert_eq!(twips("2.54cm"), Some(1440));
        assert_eq!(twips("-720"), Some(-720));
        assert_eq!(twips("abc"), None);
        assert_eq!(twips("99999999999999"), Some(i32::MAX));
    }

    /// ECMA-376 Part 1 §17.17.4: ST_OnOff.
    #[test]
    fn on_off_values() {
        assert!(on_off_value(None));
        assert!(on_off_value(Some("1")));
        assert!(on_off_value(Some("true")));
        assert!(!on_off_value(Some("0")));
        assert!(!on_off_value(Some("false")));
    }
}
