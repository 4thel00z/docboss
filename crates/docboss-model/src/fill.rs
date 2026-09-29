use crate::Color;

/// Where the colors of a path gradient spread from (ECMA-376 Part 1
/// §20.1.10.38 ST_PathShadeType).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum GradientPath {
    Circle,
    Rect,
    Shape,
}

/// A gradient fill (ECMA-376 Part 1 §20.1.8.33): color stops along a
/// line at `angle`, or spreading out from a focus rectangle when `path` is
/// set.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Gradient {
    stops: [(u32, Color); Gradient::MAX_STOPS],
    count: u8,
    /// The direction of a linear gradient, clockwise from left-to-right,
    /// in 60000ths of a degree.
    pub angle: i32,
    pub path: Option<GradientPath>,
    /// The focus of a path gradient as insets from the left, top, right
    /// and bottom edges, in thousandths of a percent of the box.
    pub focus: [i32; 4],
}

impl Gradient {
    /// The most stops a gradient holds.
    pub const MAX_STOPS: usize = 8;

    /// A gradient from stops at positions in thousandths of a percent,
    /// sorted by position and clamped to the box, keeping the first
    /// [`Gradient::MAX_STOPS`]. `None` without stops.
    pub fn new(stops: &[(i64, Color)]) -> Option<Gradient> {
        let first = stops.first()?;
        let mut gradient = Gradient {
            stops: [(0, first.1); Gradient::MAX_STOPS],
            count: 0,
            angle: 0,
            path: None,
            focus: [0; 4],
        };
        for (slot, (position, color)) in gradient.stops.iter_mut().zip(stops) {
            *slot = ((*position).clamp(0, 100_000) as u32, *color);
            gradient.count += 1;
        }
        gradient.stops[..usize::from(gradient.count)].sort_by_key(|(position, _)| *position);
        Some(gradient)
    }

    /// The gradient of a VML or binary shaded fill whose `focus`, from -1
    /// to 1, places its last stop ([MS-ODRAW] §2.3.7.15 fillFocus): 0 runs
    /// the stops backwards, 1 forwards, and a focus in between mirrors them
    /// about that point, a negative one mirroring the backward run.
    pub fn focused(stops: &[(i64, Color)], focus: f64) -> Option<Gradient> {
        let reversed: Vec<(i64, Color)> = stops.iter().map(|(p, c)| (100_000 - p, *c)).collect();
        let (run, at) = match focus < 0.0 {
            true => (stops, -focus),
            false => (&reversed[..], focus),
        };
        if at <= 0.001 {
            return Gradient::new(&reversed);
        }
        if at >= 0.999 {
            return Gradient::new(stops);
        }
        let split = (at * 100_000.0) as i64;
        let mirrored: Vec<(i64, Color)> = run
            .iter()
            .map(|(p, c)| ((100_000 - p) * split / 100_000, *c))
            .chain(
                run.iter()
                    .map(|(p, c)| (split + p * (100_000 - split) / 100_000, *c)),
            )
            .collect();
        Gradient::new(&mirrored)
    }

    /// The stops by position in thousandths of a percent.
    pub fn stops(&self) -> &[(u32, Color)] {
        &self.stops[..usize::from(self.count)]
    }

    /// The color at `t` from 0 to 1 along the gradient, blended between
    /// the stops on either side.
    pub fn color_at(&self, t: f32) -> Color {
        let stops = self.stops();
        let at = t.clamp(0.0, 1.0) * 100_000.0;
        let next = stops.partition_point(|(position, _)| (*position as f32) < at);
        let Some(&(p1, c1)) = stops.get(next) else {
            return stops.last().map_or(Color::WHITE, |s| s.1);
        };
        let Some(&(p0, c0)) = next.checked_sub(1).and_then(|i| stops.get(i)) else {
            return c1;
        };
        let span = (p1 - p0) as f32;
        let k = match span > 0.0 {
            true => (at - p0 as f32) / span,
            false => 1.0,
        };
        let mix = |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * k).round() as u8;
        Color(mix(c0.0, c1.0), mix(c0.1, c1.1), mix(c0.2, c1.2))
    }

    /// The color a text box over the gradient takes as its background: the
    /// middle of the gradient.
    pub fn average(&self) -> Color {
        self.color_at(0.5)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ECMA-376 Part 1 §20.1.8.36, §20.1.8.37: stops sort by position and
    /// colors blend between them; [MS-ODRAW] §2.3.7.15: the focus places
    /// the last color.
    #[test]
    fn gradients_blend_between_stops() {
        let red = Color(255, 0, 0);
        let blue = Color(0, 0, 255);
        let gradient = Gradient::new(&[(100_000, blue), (0, red)]).unwrap();
        assert_eq!(gradient.stops(), &[(0, red), (100_000, blue)]);
        assert_eq!(gradient.color_at(0.0), red);
        assert_eq!(gradient.color_at(1.0), blue);
        assert_eq!(gradient.color_at(0.5), Color(128, 0, 128));
        assert_eq!(gradient.color_at(-1.0), red);
        assert_eq!(Gradient::new(&[]), None);
        let pair = [(0, red), (100_000, blue)];
        let forward = Gradient::focused(&pair, 1.0).unwrap();
        assert_eq!(forward.stops(), &[(0, red), (100_000, blue)]);
        let backward = Gradient::focused(&pair, 0.0).unwrap();
        assert_eq!(backward.stops(), &[(0, blue), (100_000, red)]);
        let mirrored = Gradient::focused(&pair, 0.5).unwrap();
        assert_eq!(mirrored.color_at(0.0), red);
        assert_eq!(mirrored.color_at(0.5), blue);
        assert_eq!(mirrored.color_at(1.0), red);
        let inverse = Gradient::focused(&pair, -0.5).unwrap();
        assert_eq!(inverse.color_at(0.0), blue);
        assert_eq!(inverse.color_at(0.5), red);
        let many: Vec<(i64, Color)> = (0..20).map(|i| (i * 1000, red)).collect();
        assert_eq!(
            Gradient::new(&many).unwrap().stops().len(),
            Gradient::MAX_STOPS
        );
    }
}
