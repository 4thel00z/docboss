//! Text wrapping around floating objects: the areas objects keep text out
//! of on a page, and the free spans a row of text has beside them.

use docboss_model::{TextWrap, WrapKind, WrapSide};

use crate::paragraph::Band;
use crate::units::emu_to_pt;
use crate::Rect;

/// The units of a wrap polygon across the object's width and height.
const POLYGON_UNITS: f32 = 21_600.0;

/// An area a floating object keeps text out of, in page coordinates.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Exclusion {
    rect: Rect,
    /// The distances kept from the text above, below, left and right.
    distance: [f32; 4],
    kind: WrapKind,
    side: WrapSide,
    /// The outline text wraps to under tight and through wrapping.
    outline: Vec<(f32, f32)>,
}

impl Exclusion {
    /// The area of an object at `rect` wrapped as `wrap` says, the wrap
    /// polygon scaled onto the rectangle.
    /// ECMA-376 Part 1 §20.4.2.17, §20.4.2.19, §20.4.2.18, §20.4.2.20, §20.4.2.16.
    pub(crate) fn new(rect: Rect, wrap: &TextWrap) -> Exclusion {
        let outline = match wrap.kind {
            WrapKind::Tight | WrapKind::Through if wrap.polygon.len() >= 3 => wrap
                .polygon
                .iter()
                .map(|&(x, y)| {
                    (
                        rect.x + x as f32 / POLYGON_UNITS * rect.width,
                        rect.y + y as f32 / POLYGON_UNITS * rect.height,
                    )
                })
                .collect(),
            _ => Vec::new(),
        };
        Exclusion {
            rect,
            distance: wrap.distance.map(|d| emu_to_pt(d).max(0.0)),
            kind: wrap.kind,
            side: wrap.side,
            outline,
        }
    }

    pub(crate) fn top(&self) -> f32 {
        self.rect.y - self.distance[0]
    }

    pub(crate) fn bottom(&self) -> f32 {
        self.rect.bottom() + self.distance[1]
    }

    /// How far across the object reaches within the band from `top` to
    /// `bottom`, its side distances added; `None` when the band misses it.
    fn extent(&self, top: f32, bottom: f32) -> Option<(f32, f32)> {
        if bottom <= self.top() || top >= self.bottom() {
            return None;
        }
        let (left, right) = match self.outline.is_empty() {
            true => (self.rect.x, self.rect.right()),
            false => outline_extent(&self.outline, top, bottom)?,
        };
        Some((left - self.distance[2], right + self.distance[3]))
    }
}

/// The horizontal extent of a closed outline between `top` and `bottom`.
fn outline_extent(points: &[(f32, f32)], top: f32, bottom: f32) -> Option<(f32, f32)> {
    let mut extent: Option<(f32, f32)> = None;
    let mut widen = |x: f32| {
        extent = Some(extent.map_or((x, x), |(a, b)| (a.min(x), b.max(x))));
    };
    let closing = points.last().copied().zip(points.first().copied());
    let edges = points.windows(2).map(|w| (w[0], w[1])).chain(closing);
    for ((x0, y0), (x1, y1)) in edges {
        let (low, high) = (y0.min(y1), y0.max(y1));
        if high < top || low > bottom {
            continue;
        }
        if (y1 - y0).abs() < f32::EPSILON {
            widen(x0);
            widen(x1);
            continue;
        }
        let at = |y: f32| x0 + (x1 - x0) * (y - y0) / (y1 - y0);
        widen(at(low.max(top)));
        widen(at(high.min(bottom)));
    }
    extent
}

/// Takes `cut` out of each span.
fn subtract(spans: Vec<(f32, f32)>, (a, b): (f32, f32)) -> Vec<(f32, f32)> {
    spans
        .into_iter()
        .flat_map(|(left, right)| {
            let before = (left, right.min(a));
            let after = (left.max(b), right);
            [before, after].into_iter().filter(|(l, r)| r > l)
        })
        .collect()
}

/// The free spans of the band from `top` to `bottom` across `left` to
/// `right`, in page coordinates, and the lowest bottom of the objects in
/// the way; `None` when no object reaches into the band. Text flows
/// beside an object on the sides its wrapping allows (ECMA-376 Part 1
/// §20.4.3.7): both, only its left or right, or the side with more room;
/// top and bottom wrapping leaves no span.
pub(crate) fn band(
    exclusions: &[Exclusion],
    top: f32,
    bottom: f32,
    left: f32,
    right: f32,
) -> Option<Band> {
    let mut spans = vec![(left, right)];
    let mut below = f32::INFINITY;
    let mut hit = false;
    for exclusion in exclusions {
        let Some((a, b)) = exclusion.extent(top, bottom) else {
            continue;
        };
        if b <= left || a >= right {
            continue;
        }
        hit = true;
        below = below.min(exclusion.bottom());
        let cut = match (exclusion.kind, exclusion.side) {
            (WrapKind::TopAndBottom, _) => (f32::NEG_INFINITY, f32::INFINITY),
            (_, WrapSide::Both) => (a, b),
            (_, WrapSide::Left) => (a, f32::INFINITY),
            (_, WrapSide::Right) => (f32::NEG_INFINITY, b),
            (_, WrapSide::Largest) if a - left >= right - b => (a, f32::INFINITY),
            (_, WrapSide::Largest) => (f32::NEG_INFINITY, b),
        };
        spans = subtract(spans, cut);
    }
    hit.then_some(Band { spans, below })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(side: WrapSide) -> TextWrap {
        TextWrap {
            kind: WrapKind::Square,
            side,
            distance: [0, 0, 127_000, 127_000],
            polygon: Vec::new(),
        }
    }

    /// ECMA-376 Part 1 §20.4.2.17, §20.4.3.7: both sides leave two spans,
    /// one side one, and the distances widen the object.
    #[test]
    fn square_wrapping_leaves_the_allowed_sides() {
        let rect = Rect::new(200.0, 100.0, 100.0, 50.0);
        let both = [Exclusion::new(rect, &square(WrapSide::Both))];
        let found = band(&both, 110.0, 122.0, 72.0, 540.0).expect("the band meets the object");
        assert_eq!(found.spans, vec![(72.0, 190.0), (310.0, 540.0)]);
        assert_eq!(found.below, 150.0);
        let left = [Exclusion::new(rect, &square(WrapSide::Left))];
        let found = band(&left, 110.0, 122.0, 72.0, 540.0).expect("the band meets the object");
        assert_eq!(found.spans, vec![(72.0, 190.0)]);
        let largest = [Exclusion::new(rect, &square(WrapSide::Largest))];
        let found = band(&largest, 110.0, 122.0, 72.0, 540.0).expect("the band meets the object");
        assert_eq!(found.spans, vec![(310.0, 540.0)]);
        assert!(band(&both, 150.0, 162.0, 72.0, 540.0).is_none());
    }

    /// ECMA-376 Part 1 §20.4.2.20: top and bottom wrapping leaves no span.
    #[test]
    fn top_and_bottom_wrapping_blocks_the_row() {
        let wrap = TextWrap {
            kind: WrapKind::TopAndBottom,
            ..TextWrap::default()
        };
        let rect = Rect::new(400.0, 100.0, 50.0, 50.0);
        let found = band(&[Exclusion::new(rect, &wrap)], 90.0, 102.0, 72.0, 540.0)
            .expect("the band meets the object");
        assert!(found.spans.is_empty());
    }

    /// Tight and through wrapping follow the polygon, here a triangle wide
    /// at the bottom.
    /// ECMA-376 Part 1 §20.4.2.19, §20.4.2.18, §20.4.2.16.
    #[test]
    fn tight_wrapping_follows_the_polygon() {
        let wrap = TextWrap {
            kind: WrapKind::Tight,
            side: WrapSide::Both,
            distance: [0; 4],
            polygon: vec![(10_800, 0), (21_600, 21_600), (0, 21_600), (10_800, 0)],
        };
        let rect = Rect::new(100.0, 100.0, 100.0, 100.0);
        let exclusion = [Exclusion::new(rect, &wrap)];
        let top = band(&exclusion, 100.0, 110.0, 0.0, 600.0).expect("meets the apex");
        assert_eq!(top.spans, vec![(0.0, 145.0), (155.0, 600.0)]);
        let low = band(&exclusion, 190.0, 200.0, 0.0, 600.0).expect("meets the base");
        assert_eq!(low.spans, vec![(0.0, 100.0), (200.0, 600.0)]);
        let through = TextWrap {
            kind: WrapKind::Through,
            ..wrap
        };
        let exclusion = [Exclusion::new(rect, &through)];
        let top = band(&exclusion, 100.0, 110.0, 0.0, 600.0).expect("meets the apex");
        assert_eq!(top.spans, vec![(0.0, 145.0), (155.0, 600.0)]);
    }
}
