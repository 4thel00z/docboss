//! TrueType `glyf` outlines: simple glyphs with implied on-curve midpoints
//! and composite glyphs with their component transforms.

use crate::bytes::{f2dot14, i16_at, u16_at};
use crate::outline::Seg;

const MAX_COMPOSITE_DEPTH: u32 = 8;

pub(crate) struct Glyf<'a> {
    pub data: &'a [u8],
    /// Byte offsets into `data` for each glyph, `num_glyphs + 1` long.
    pub loca: &'a [u32],
}

impl Glyf<'_> {
    pub(crate) fn outline(&self, gid: u16, out: &mut Vec<Seg>) {
        self.append(gid, out, 0);
    }

    fn append(&self, gid: u16, out: &mut Vec<Seg>, depth: u32) {
        if depth > MAX_COMPOSITE_DEPTH {
            return;
        }
        let (Some(&start), Some(&end)) =
            (self.loca.get(gid as usize), self.loca.get(gid as usize + 1))
        else {
            return;
        };
        let Some(g) = self.data.get(start as usize..end as usize) else {
            return;
        };
        let Some(contours) = i16_at(g, 0) else {
            return;
        };
        if contours < 0 {
            self.composite(g, out, depth);
            return;
        }
        simple_glyph(g, contours as usize, out);
    }

    fn composite(&self, g: &[u8], out: &mut Vec<Seg>, depth: u32) {
        const ARGS_ARE_WORDS: u16 = 0x0001;
        const ARGS_ARE_XY: u16 = 0x0002;
        const HAVE_SCALE: u16 = 0x0008;
        const MORE_COMPONENTS: u16 = 0x0020;
        const HAVE_XY_SCALE: u16 = 0x0040;
        const HAVE_2X2: u16 = 0x0080;
        let mut p = 10;
        loop {
            let (Some(flags), Some(component)) = (u16_at(g, p), u16_at(g, p + 2)) else {
                return;
            };
            p += 4;
            let (dx, dy) = if flags & ARGS_ARE_WORDS != 0 {
                let (Some(a), Some(b)) = (i16_at(g, p), i16_at(g, p + 2)) else {
                    return;
                };
                p += 4;
                (a as f32, b as f32)
            } else {
                let (Some(&a), Some(&b)) = (g.get(p), g.get(p + 1)) else {
                    return;
                };
                p += 2;
                (a as i8 as f32, b as i8 as f32)
            };
            let mut m = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
            if flags & HAVE_SCALE != 0 {
                let s = f2dot14(g, p);
                m[0] = s;
                m[3] = s;
                p += 2;
            } else if flags & HAVE_XY_SCALE != 0 {
                m[0] = f2dot14(g, p);
                m[3] = f2dot14(g, p + 2);
                p += 4;
            } else if flags & HAVE_2X2 != 0 {
                m[0] = f2dot14(g, p);
                m[1] = f2dot14(g, p + 2);
                m[2] = f2dot14(g, p + 4);
                m[3] = f2dot14(g, p + 6);
                p += 8;
            }
            if flags & ARGS_ARE_XY != 0 {
                m[4] = dx;
                m[5] = dy;
            }
            let mut sub = Vec::new();
            self.append(component, &mut sub, depth + 1);
            out.extend(sub.into_iter().map(|seg| seg.transformed(m)));
            if flags & MORE_COMPONENTS == 0 {
                return;
            }
        }
    }
}

#[derive(Clone, Copy)]
struct Pt {
    x: f32,
    y: f32,
    on: bool,
}

fn simple_glyph(g: &[u8], num_contours: usize, out: &mut Vec<Seg>) {
    const ON_CURVE: u8 = 0x01;
    const X_SHORT: u8 = 0x02;
    const Y_SHORT: u8 = 0x04;
    const REPEAT: u8 = 0x08;
    const X_SAME: u8 = 0x10;
    const Y_SAME: u8 = 0x20;
    let mut p = 10;
    let mut end_pts = Vec::with_capacity(num_contours);
    for _ in 0..num_contours {
        let Some(e) = u16_at(g, p) else { return };
        end_pts.push(e as usize);
        p += 2;
    }
    let Some(&last) = end_pts.last() else { return };
    let num_points = last + 1;
    if num_points > g.len() * 2 {
        return;
    }
    let Some(instructions) = u16_at(g, p) else {
        return;
    };
    p += 2 + instructions as usize;
    let mut flags = Vec::with_capacity(num_points);
    while flags.len() < num_points {
        let Some(&f) = g.get(p) else { return };
        p += 1;
        flags.push(f);
        if f & REPEAT == 0 {
            continue;
        }
        let Some(&count) = g.get(p) else { return };
        p += 1;
        let room = num_points - flags.len();
        flags.extend(std::iter::repeat_n(f, (count as usize).min(room)));
    }
    let mut read_axis = |short: u8, same: u8| -> Option<Vec<i32>> {
        let mut values = Vec::with_capacity(num_points);
        let mut v = 0i32;
        for &f in &flags {
            if f & short != 0 {
                let d = *g.get(p)? as i32;
                p += 1;
                v += if f & same != 0 { d } else { -d };
            } else if f & same == 0 {
                v += i16_at(g, p)? as i32;
                p += 2;
            }
            values.push(v);
        }
        Some(values)
    };
    let Some(xs) = read_axis(X_SHORT, X_SAME) else {
        return;
    };
    let Some(ys) = read_axis(Y_SHORT, Y_SAME) else {
        return;
    };
    let mut begin = 0usize;
    for &end in &end_pts {
        if end >= num_points || end < begin {
            break;
        }
        let pts: Vec<Pt> = (begin..=end)
            .map(|i| Pt {
                x: xs[i] as f32,
                y: ys[i] as f32,
                on: flags[i] & ON_CURVE != 0,
            })
            .collect();
        contour(&pts, out);
        begin = end + 1;
    }
}

fn mid(a: Pt, b: Pt) -> Pt {
    Pt {
        x: (a.x + b.x) * 0.5,
        y: (a.y + b.y) * 0.5,
        on: true,
    }
}

fn contour(pts: &[Pt], out: &mut Vec<Seg>) {
    let n = pts.len();
    if n < 2 {
        return;
    }
    let seq: Vec<Pt> = match (0..n).find(|&i| pts[i].on) {
        Some(s) => (0..=n).map(|k| pts[(s + k) % n]).collect(),
        None => {
            let start = mid(pts[0], pts[n - 1]);
            std::iter::once(start)
                .chain(pts.iter().copied())
                .chain(std::iter::once(start))
                .collect()
        }
    };
    out.push(Seg::Move(seq[0].x, seq[0].y));
    let mut i = 1;
    while i < seq.len() {
        let p = seq[i];
        if p.on {
            out.push(Seg::Line(p.x, p.y));
            i += 1;
            continue;
        }
        let Some(&next) = seq.get(i + 1) else { break };
        if next.on {
            out.push(Seg::Quad(p.x, p.y, next.x, next.y));
            i += 2;
            continue;
        }
        let m = mid(p, next);
        out.push(Seg::Quad(p.x, p.y, m.x, m.y));
        i += 1;
    }
    out.push(Seg::Close);
}
