//! Anti-aliased coverage rasterizer: each polygon edge adds the signed
//! area it cuts out of the pixels it crosses to a per-row accumulation
//! buffer, and a running sum turns that into nonzero-rule coverage. The
//! same technique as pdfboss's rasterizer, reduced to the nonzero rule.

use docboss_font::Seg;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Point {
    pub x: f32,
    pub y: f32,
}

/// Closed polygons in device pixels.
pub(crate) type Polygons = Vec<Vec<Point>>;

const FLATNESS: f32 = 0.2;
const MAX_SUBDIVISIONS: u32 = 64;

/// Flattens outline segments under the map `(x, y) -> (a*x + c*y + e,
/// b*x + d*y + f)` into polygons.
pub(crate) fn flatten(segs: &[Seg], m: [f32; 6]) -> Polygons {
    polylines(segs, m)
        .into_iter()
        .filter_map(|(points, _)| (points.len() > 2).then_some(points))
        .collect()
}

/// Flattens outline segments under the map of [`flatten`] into one
/// polyline per subpath, each with whether a close ends it.
pub(crate) fn polylines(segs: &[Seg], m: [f32; 6]) -> Vec<(Vec<Point>, bool)> {
    let map = |x: f32, y: f32| Point {
        x: m[0] * x + m[2] * y + m[4],
        y: m[1] * x + m[3] * y + m[5],
    };
    let mut out: Vec<(Vec<Point>, bool)> = Vec::new();
    let mut current: Vec<Point> = Vec::new();
    let mut pen = Point { x: 0.0, y: 0.0 };
    let finish = |current: &mut Vec<Point>, out: &mut Vec<(Vec<Point>, bool)>, closed: bool| {
        if current.len() > 1 {
            out.push((std::mem::take(current), closed));
        }
        current.clear();
    };
    for seg in segs {
        match *seg {
            Seg::Move(x, y) => {
                finish(&mut current, &mut out, false);
                pen = map(x, y);
                current.push(pen);
            }
            Seg::Line(x, y) => {
                if current.is_empty() {
                    current.push(pen);
                }
                pen = map(x, y);
                current.push(pen);
            }
            Seg::Quad(cx, cy, x, y) => {
                if current.is_empty() {
                    current.push(pen);
                }
                let (c, e) = (map(cx, cy), map(x, y));
                let steps = steps_for(
                    ((pen.x - 2.0 * c.x + e.x).abs() + (pen.y - 2.0 * c.y + e.y).abs()) / 4.0,
                );
                for i in 1..=steps {
                    let t = i as f32 / steps as f32;
                    let u = 1.0 - t;
                    current.push(Point {
                        x: u * u * pen.x + 2.0 * u * t * c.x + t * t * e.x,
                        y: u * u * pen.y + 2.0 * u * t * c.y + t * t * e.y,
                    });
                }
                pen = e;
            }
            Seg::Cubic(x1, y1, x2, y2, x, y) => {
                if current.is_empty() {
                    current.push(pen);
                }
                let (c1, c2, e) = (map(x1, y1), map(x2, y2), map(x, y));
                let d1 = (pen.x - 2.0 * c1.x + c2.x).abs() + (pen.y - 2.0 * c1.y + c2.y).abs();
                let d2 = (c1.x - 2.0 * c2.x + e.x).abs() + (c1.y - 2.0 * c2.y + e.y).abs();
                let steps = steps_for(d1.max(d2) * 0.75);
                for i in 1..=steps {
                    let t = i as f32 / steps as f32;
                    let u = 1.0 - t;
                    let (a, b, cc, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
                    current.push(Point {
                        x: a * pen.x + b * c1.x + cc * c2.x + d * e.x,
                        y: a * pen.y + b * c1.y + cc * c2.y + d * e.y,
                    });
                }
                pen = e;
            }
            Seg::Close => {
                if let Some(first) = current.first() {
                    pen = *first;
                }
                finish(&mut current, &mut out, true);
            }
        }
    }
    finish(&mut current, &mut out, false);
    out
}

fn steps_for(deviation: f32) -> u32 {
    if !deviation.is_finite() {
        return 1;
    }
    ((deviation / FLATNESS).sqrt().ceil() as u32).clamp(1, MAX_SUBDIVISIONS)
}

#[derive(Debug)]
struct Edge {
    x0: f32,
    y0: f32,
    y1: f32,
    dir: f32,
    dxdy: f32,
}

/// A coverage mask over a pixel rectangle.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Mask {
    pub x: i32,
    pub y: i32,
    pub width: usize,
    pub height: usize,
    pub coverage: Vec<u8>,
}

impl Mask {
    /// The coverage at pixel `(x, y)`, zero outside the mask.
    pub(crate) fn at(&self, x: i32, y: i32) -> u8 {
        let (col, row) = (x - self.x, y - self.y);
        if col < 0 || row < 0 || col as usize >= self.width || row as usize >= self.height {
            return 0;
        }
        self.coverage
            .get(row as usize * self.width + col as usize)
            .copied()
            .unwrap_or(0)
    }
}

/// Rasterizes polygons into a coverage mask sized to their bounds and
/// clipped to the pixel rectangle `(left, top, right, bottom)` when given.
pub(crate) fn rasterize(polys: &Polygons, clip: Option<(i32, i32, i32, i32)>) -> Mask {
    coverage::<false>(polys, clip)
}

/// [`rasterize`] under the even-odd rule: a winding sum folds back to zero
/// at every second crossing.
pub(crate) fn rasterize_even_odd(polys: &Polygons, clip: Option<(i32, i32, i32, i32)>) -> Mask {
    coverage::<true>(polys, clip)
}

fn coverage<const EVEN_ODD: bool>(polys: &Polygons, clip: Option<(i32, i32, i32, i32)>) -> Mask {
    let mut edges: Vec<Edge> = Vec::new();
    let (mut xmin, mut xmax, mut ymin, mut ymax) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
    for poly in polys {
        for i in 0..poly.len() {
            let (p, q) = (poly[i], poly[(i + 1) % poly.len()]);
            if !(p.x.is_finite() && p.y.is_finite() && q.x.is_finite() && q.y.is_finite()) {
                continue;
            }
            xmin = xmin.min(p.x);
            xmax = xmax.max(p.x);
            ymin = ymin.min(p.y);
            ymax = ymax.max(p.y);
            if p.y == q.y {
                continue;
            }
            let (top, bottom, dir) = if p.y < q.y { (p, q, 1.0) } else { (q, p, -1.0) };
            edges.push(Edge {
                x0: top.x,
                y0: top.y,
                y1: bottom.y,
                dir,
                dxdy: (bottom.x - top.x) / (bottom.y - top.y),
            });
        }
    }
    if edges.is_empty() {
        return Mask::default();
    }
    let mut x0 = xmin.floor() as i32;
    let mut y0 = ymin.floor() as i32;
    let mut x1 = xmax.ceil() as i32 + 1;
    let mut y1 = ymax.ceil() as i32;
    if let Some((left, top, right, bottom)) = clip {
        x0 = x0.max(left);
        y0 = y0.max(top);
        x1 = x1.min(right);
        y1 = y1.min(bottom);
    }
    if x1 <= x0 || y1 <= y0 {
        return Mask::default();
    }
    let width = (x1 - x0) as usize;
    let height = (y1 - y0) as usize;
    if width.saturating_mul(height) > 64 << 20 {
        return Mask::default();
    }
    edges.sort_by(|a, b| a.y0.total_cmp(&b.y0));
    let mut coverage = vec![0u8; width * height];
    let mut acc = vec![0f32; width + 2];
    let mut active: Vec<usize> = Vec::new();
    let mut next = 0usize;
    for row in 0..height {
        let ytop = (y0 + row as i32) as f32;
        let ybot = ytop + 1.0;
        while next < edges.len() && edges[next].y0 < ybot {
            active.push(next);
            next += 1;
        }
        active.retain(|&i| edges[i].y1 > ytop);
        if active.is_empty() {
            continue;
        }
        for &i in &active {
            let e = &edges[i];
            let ya = e.y0.max(ytop);
            let yb = e.y1.min(ybot);
            if yb <= ya {
                continue;
            }
            let xa = e.x0 + (ya - e.y0) * e.dxdy - x0 as f32;
            let xb = e.x0 + (yb - e.y0) * e.dxdy - x0 as f32;
            accumulate(&mut acc, xa, xb, (yb - ya) * e.dir);
        }
        let out = &mut coverage[row * width..(row + 1) * width];
        let mut sum = 0.0f32;
        for (slot, value) in out.iter_mut().zip(acc.iter_mut()) {
            sum += *value;
            *value = 0.0;
            let winding = sum.abs();
            let covered = match EVEN_ODD {
                true => 1.0 - (winding % 2.0 - 1.0).abs(),
                false => winding.min(1.0),
            };
            *slot = (covered * 255.0 + 0.5) as u8;
        }
        acc[width..].fill(0.0);
    }
    Mask {
        x: x0,
        y: y0,
        width,
        height,
        coverage,
    }
}

/// Adds one edge piece within a pixel row: `dy` signed height entering at
/// `xa` and leaving at `xb`, split across the columns it crosses.
fn accumulate(acc: &mut [f32], xa: f32, xb: f32, dy: f32) {
    let right = (acc.len() - 2) as f32;
    let (mut xl, mut xr) = if xa <= xb { (xa, xb) } else { (xb, xa) };
    let mut dy = dy;
    if xr <= 0.0 {
        acc[0] += dy;
        return;
    }
    if xl >= right {
        return;
    }
    if xl < 0.0 {
        let outside = -xl / (xr - xl);
        acc[0] += dy * outside;
        dy -= dy * outside;
        xl = 0.0;
    }
    if xr > right {
        dy *= (right - xl) / (xr - xl);
        xr = right;
    }
    let first = xl as usize;
    let last = (xr as usize).min(acc.len() - 2);
    if first >= last {
        let frac = (xl + xr) * 0.5 - first as f32;
        acc[first] += dy * (1.0 - frac);
        acc[first + 1] += dy * frac;
        return;
    }
    let span = xr - xl;
    for c in first..=last {
        let l = xl.max(c as f32);
        let r = xr.min(c as f32 + 1.0);
        if r <= l {
            continue;
        }
        let part = dy * (r - l) / span;
        let frac = (l + r) * 0.5 - c as f32;
        acc[c] += part * (1.0 - frac);
        acc[c + 1] += part * frac;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(x0: f32, y0: f32, x1: f32, y1: f32) -> Polygons {
        vec![vec![
            Point { x: x0, y: y0 },
            Point { x: x1, y: y0 },
            Point { x: x1, y: y1 },
            Point { x: x0, y: y1 },
        ]]
    }

    #[test]
    fn integer_square_is_fully_covered() {
        let mask = rasterize(&square(2.0, 3.0, 6.0, 5.0), None);
        let covered: u32 = mask.coverage.iter().map(|&c| u32::from(c)).sum();
        assert_eq!(covered, 8 * 255);
        assert!(mask.coverage.iter().all(|&c| c == 0 || c == 255));
    }

    #[test]
    fn half_pixel_edges_are_half_covered() {
        let mask = rasterize(&square(0.5, 0.0, 2.5, 1.0), None);
        let row: Vec<u8> = mask.coverage[..mask.width].to_vec();
        assert_eq!(&row[..3], &[128, 255, 128]);
    }

    #[test]
    fn triangle_area_matches() {
        let tri = vec![vec![
            Point { x: 0.0, y: 0.0 },
            Point { x: 10.0, y: 0.0 },
            Point { x: 0.0, y: 10.0 },
        ]];
        let mask = rasterize(&tri, None);
        let area: f32 = mask.coverage.iter().map(|&c| c as f32 / 255.0).sum();
        assert!((area - 50.0).abs() < 0.5, "{area}");
    }

    #[test]
    fn even_odd_leaves_a_hole_where_same_windings_overlap() {
        let mut polys = square(0.0, 0.0, 10.0, 10.0);
        polys.extend(square(3.0, 3.0, 7.0, 7.0));
        let mask = rasterize_even_odd(&polys, None);
        assert_eq!(mask.coverage[5 * mask.width + 5], 0);
        assert_eq!(mask.coverage[mask.width + 1], 255);
        assert_eq!(rasterize(&polys, None).coverage[5 * mask.width + 5], 255);
    }

    #[test]
    fn opposite_windings_nest_as_holes_only_under_even_winding_sums() {
        let mut polys = square(0.0, 0.0, 10.0, 10.0);
        let mut hole = square(3.0, 3.0, 7.0, 7.0).remove(0);
        hole.reverse();
        polys.push(hole);
        let mask = rasterize(&polys, None);
        assert_eq!(mask.coverage[5 * mask.width + 5], 0);
        assert_eq!(mask.coverage[mask.width + 1], 255);
    }
}
