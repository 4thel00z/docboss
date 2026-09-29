//! Gradient fills: the color of each pixel of a shaded outline.

use docboss_model::{Color, Gradient, GradientPath};

/// An affine map `[a, b, c, d, e, f]`: `(x, y)` goes to
/// `(a x + c y + e, b x + d y + f)`.
pub(crate) type Affine = [f32; 6];

/// Applies `m` to a point.
pub(crate) fn apply(m: Affine, x: f32, y: f32) -> (f32, f32) {
    (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5])
}

/// The inverse of `m`, `None` when it flattens the plane.
pub(crate) fn invert(m: Affine) -> Option<Affine> {
    let det = m[0] * m[3] - m[1] * m[2];
    if det.abs() < 1e-12 || !det.is_finite() {
        return None;
    }
    let (a, b, c, d) = (m[3] / det, -m[1] / det, -m[2] / det, m[0] / det);
    Some([a, b, c, d, -(a * m[4] + c * m[5]), -(b * m[4] + d * m[5])])
}

/// `outer` after `inner`: the map that applies `inner`, then `outer`.
pub(crate) fn compose(outer: Affine, inner: Affine) -> Affine {
    [
        outer[0] * inner[0] + outer[2] * inner[1],
        outer[1] * inner[0] + outer[3] * inner[1],
        outer[0] * inner[2] + outer[2] * inner[3],
        outer[1] * inner[2] + outer[3] * inner[3],
        outer[0] * inner[4] + outer[2] * inner[5] + outer[4],
        outer[1] * inner[4] + outer[3] * inner[5] + outer[5],
    ]
}

/// A gradient laid over a box of `width` by `height`, giving the color at
/// each point of the box.
pub(crate) struct Shader<'g> {
    gradient: &'g Gradient,
    width: f32,
    height: f32,
    direction: (f32, f32),
    half: f32,
    focus: [f32; 4],
}

impl<'g> Shader<'g> {
    pub(crate) fn new(gradient: &'g Gradient, (width, height): (f32, f32)) -> Shader<'g> {
        let angle = (f64::from(gradient.angle) / 60_000.0).to_radians();
        let (sin, cos) = (angle.sin() as f32, angle.cos() as f32);
        let half = (width * cos.abs() + height * sin.abs()) / 2.0;
        let inset = |i: usize| (gradient.focus[i] as f32 / 100_000.0).clamp(0.0, 1.0);
        let focus = [
            inset(0) * width,
            inset(1) * height,
            width - inset(2) * width,
            height - inset(3) * height,
        ];
        Shader {
            gradient,
            width,
            height,
            direction: (cos, sin),
            half,
            focus,
        }
    }

    /// The color at `(x, y)` in the box. A linear gradient
    /// (ECMA-376 Part 1 §20.1.8.41) runs across the box's extent along its
    /// direction; a path gradient (§20.1.8.46) runs from its focus
    /// rectangle (§20.1.8.31) out to the box's edge, a circle through the
    /// box's corners or a rectangle.
    pub(crate) fn color(&self, x: f32, y: f32) -> Color {
        let t = match self.gradient.path {
            None => {
                let (cx, cy) = (x - self.width / 2.0, y - self.height / 2.0);
                let along = cx * self.direction.0 + cy * self.direction.1;
                match self.half > 1e-6 {
                    true => (along + self.half) / (2.0 * self.half),
                    false => 0.0,
                }
            }
            Some(GradientPath::Circle) => {
                let [l, t, r, b] = self.focus;
                let (fx, fy) = ((l + r) / 2.0, (t + b) / 2.0);
                let rx = fx.max(self.width - fx) * std::f32::consts::SQRT_2;
                let ry = fy.max(self.height - fy) * std::f32::consts::SQRT_2;
                let dx = (x - fx) / rx.max(1e-6);
                let dy = (y - fy) / ry.max(1e-6);
                (dx * dx + dy * dy).sqrt()
            }
            Some(GradientPath::Rect | GradientPath::Shape) => {
                let [l, t, r, b] = self.focus;
                let spread = |v: f32, lo: f32, hi: f32, end: f32| {
                    if v < lo {
                        return (lo - v) / lo.max(1e-6);
                    }
                    if v > hi {
                        return (v - hi) / (end - hi).max(1e-6);
                    }
                    0.0
                };
                spread(x, l, r, self.width).max(spread(y, t, b, self.height))
            }
        };
        self.gradient.color_at(t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ECMA-376 Part 1 §20.1.8.41: a linear gradient at 90 degrees runs
    /// from the top of the box to its bottom; §20.1.8.46, §20.1.8.31: a
    /// rectangle path runs from its focus to the edge.
    #[test]
    fn shaders_follow_the_gradient_geometry() {
        let red = Color(255, 0, 0);
        let blue = Color(0, 0, 255);
        let mut gradient = Gradient::new(&[(0, red), (100_000, blue)]).unwrap();
        gradient.angle = 5_400_000;
        let shader = Shader::new(&gradient, (100.0, 50.0));
        assert_eq!(shader.color(30.0, 0.0), red);
        assert_eq!(shader.color(70.0, 50.0), blue);
        gradient.path = Some(GradientPath::Rect);
        gradient.focus = [50_000; 4];
        let shader = Shader::new(&gradient, (100.0, 50.0));
        assert_eq!(shader.color(50.0, 25.0), red);
        assert_eq!(shader.color(0.0, 25.0), blue);
        let m = [2.0, 0.0, 0.0, 3.0, 5.0, 7.0];
        let back = invert(m).unwrap();
        let (x, y) = apply(back, apply(m, 1.0, 1.0).0, apply(m, 1.0, 1.0).1);
        assert!((x - 1.0).abs() < 1e-5 && (y - 1.0).abs() < 1e-5);
        assert_eq!(compose(back, m), [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
    }
}
