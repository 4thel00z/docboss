//! Glyph outlines in font units.

/// One outline command. The start of each curve is the current point.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Seg {
    Move(f32, f32),
    Line(f32, f32),
    /// Quadratic Bezier: control point then end point.
    Quad(f32, f32, f32, f32),
    /// Cubic Bezier: two control points then the end point.
    Cubic(f32, f32, f32, f32, f32, f32),
    Close,
}

impl Seg {
    /// Applies the linear map `[a b c d]` and translation `(tx, ty)`.
    pub fn transformed(self, m: [f32; 6]) -> Seg {
        let [a, b, c, d, tx, ty] = m;
        let f = |x: f32, y: f32| (a * x + c * y + tx, b * x + d * y + ty);
        match self {
            Seg::Move(x, y) => {
                let (x, y) = f(x, y);
                Seg::Move(x, y)
            }
            Seg::Line(x, y) => {
                let (x, y) = f(x, y);
                Seg::Line(x, y)
            }
            Seg::Quad(cx, cy, x, y) => {
                let (cx, cy) = f(cx, cy);
                let (x, y) = f(x, y);
                Seg::Quad(cx, cy, x, y)
            }
            Seg::Cubic(x1, y1, x2, y2, x, y) => {
                let (x1, y1) = f(x1, y1);
                let (x2, y2) = f(x2, y2);
                let (x, y) = f(x, y);
                Seg::Cubic(x1, y1, x2, y2, x, y)
            }
            Seg::Close => Seg::Close,
        }
    }
}

/// Bounding box of an outline in font units: `[x_min, y_min, x_max, y_max]`,
/// control points included; `None` for an empty outline.
pub fn bounds(segs: &[Seg]) -> Option<[f32; 4]> {
    let mut b = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
    let mut any = false;
    let mut add = |x: f32, y: f32| {
        any = true;
        b = [b[0].min(x), b[1].min(y), b[2].max(x), b[3].max(y)];
    };
    for seg in segs {
        match *seg {
            Seg::Move(x, y) | Seg::Line(x, y) => add(x, y),
            Seg::Quad(cx, cy, x, y) => {
                add(cx, cy);
                add(x, y);
            }
            Seg::Cubic(x1, y1, x2, y2, x, y) => {
                add(x1, y1);
                add(x2, y2);
                add(x, y);
            }
            Seg::Close => {}
        }
    }
    any.then_some(b)
}
