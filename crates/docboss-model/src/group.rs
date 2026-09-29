use crate::Drawing;

/// A drawing inside a group or drawing canvas, placed in the group's box:
/// `x` and `y` are EMU from the group's top-left corner, and the drawing's
/// extent, rotation and flips are already mapped through every group it
/// sits in.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct GroupMember {
    pub x: i64,
    pub y: i64,
    pub drawing: Drawing,
}

/// A box in some coordinate space with the turn and flips of the shape it
/// holds; the rotation is clockwise in 60000ths of a degree.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ChildBox {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub rotation: i32,
    pub flip_horizontal: bool,
    pub flip_vertical: bool,
}

/// How a group maps the coordinates of its children into its parent's
/// (ECMA-376 Part 1 §20.1.7.5, [MS-ODRAW] §2.2.38): the child rectangle
/// at `child_offset` of `child_extent` fills the group's box at `offset`
/// of `extent`, which is then flipped and turned about its center.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GroupFrame {
    pub offset: (f64, f64),
    pub extent: (f64, f64),
    pub child_offset: (f64, f64),
    pub child_extent: (f64, f64),
    pub rotation: i32,
    pub flip_horizontal: bool,
    pub flip_vertical: bool,
}

const FULL_TURN: i64 = 21_600_000;
const QUARTER_TURN: i64 = 5_400_000;

impl GroupFrame {
    /// A frame whose children are in the parent's units at the same origin,
    /// as a drawing canvas places them.
    pub fn identity(width: f64, height: f64) -> GroupFrame {
        GroupFrame {
            offset: (0.0, 0.0),
            extent: (width, height),
            child_offset: (0.0, 0.0),
            child_extent: (width, height),
            rotation: 0,
            flip_horizontal: false,
            flip_vertical: false,
        }
    }

    /// Maps a child's box into the parent's coordinates. Its center follows
    /// the group's map; its size scales with the group, along the axes the
    /// child's own quarter turns leave it on; its turn adds to the group's,
    /// negated under one flip, and its flips combine with the group's.
    pub fn place(&self, child: ChildBox) -> ChildBox {
        let scale = |extent: f64, child: f64| match child.abs() > 1e-9 && extent.is_finite() {
            true => extent / child,
            false => 1.0,
        };
        let sx = scale(self.extent.0, self.child_extent.0);
        let sy = scale(self.extent.1, self.child_extent.1);
        let quarter = ((i64::from(child.rotation) + QUARTER_TURN / 2) / QUARTER_TURN) % 2 == 1;
        let (width, height) = match quarter {
            true => (child.width * sy, child.height * sx),
            false => (child.width * sx, child.height * sy),
        };
        let center = (
            (child.x + child.width / 2.0 - self.child_offset.0) * sx + self.offset.0,
            (child.y + child.height / 2.0 - self.child_offset.1) * sy + self.offset.1,
        );
        let pivot = (
            self.offset.0 + self.extent.0 / 2.0,
            self.offset.1 + self.extent.1 / 2.0,
        );
        let mut d = (center.0 - pivot.0, center.1 - pivot.1);
        if self.flip_horizontal {
            d.0 = -d.0;
        }
        if self.flip_vertical {
            d.1 = -d.1;
        }
        let angle = (f64::from(self.rotation) / 60_000.0).to_radians();
        let (sin, cos) = angle.sin_cos();
        let center = (
            pivot.0 + d.0 * cos - d.1 * sin,
            pivot.1 + d.0 * sin + d.1 * cos,
        );
        let own = match self.flip_horizontal != self.flip_vertical {
            true => -i64::from(child.rotation),
            false => i64::from(child.rotation),
        };
        ChildBox {
            x: center.0 - width / 2.0,
            y: center.1 - height / 2.0,
            width,
            height,
            rotation: (i64::from(self.rotation) + own).rem_euclid(FULL_TURN) as i32,
            flip_horizontal: child.flip_horizontal != self.flip_horizontal,
            flip_vertical: child.flip_vertical != self.flip_vertical,
        }
    }

    /// Maps a child's box through `frames`, innermost last, into the
    /// outermost group's box, whose own offset is its origin.
    pub fn place_through(frames: &[GroupFrame], child: ChildBox) -> ChildBox {
        let Some((outer, inner)) = frames.split_first() else {
            return child;
        };
        let placed = inner
            .iter()
            .rev()
            .fold(child, |child, frame| frame.place(child));
        let origin = GroupFrame {
            offset: (0.0, 0.0),
            ..*outer
        };
        origin.place(placed)
    }
}

impl GroupMember {
    /// A member from its drawing and its box in the outermost group's EMU,
    /// taking the box's extent, rotation and flips into the drawing.
    pub fn new(mut drawing: Drawing, placed: ChildBox) -> GroupMember {
        let emu = |v: f64| match v.is_finite() {
            true => v.round().clamp(-1e15, 1e15) as i64,
            false => 0,
        };
        drawing.width = emu(placed.width).max(0);
        drawing.height = emu(placed.height).max(0);
        drawing.shape.rotation = placed.rotation;
        drawing.shape.flip_horizontal = placed.flip_horizontal;
        drawing.shape.flip_vertical = placed.flip_vertical;
        GroupMember {
            x: emu(placed.x),
            y: emu(placed.y),
            drawing,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    /// ECMA-376 Part 1 §20.1.7.5, §20.1.7.2, §20.1.7.1: a child's box
    /// scales from the child extent to the group's extent; a turned group
    /// carries its children's centers round and adds its turn to theirs;
    /// a flipped group mirrors their centers, negates their turn and
    /// toggles their flips.
    #[test]
    fn group_frames_place_children() {
        let frame = GroupFrame {
            offset: (1000.0, 2000.0),
            extent: (200.0, 100.0),
            child_offset: (10.0, 10.0),
            child_extent: (100.0, 100.0),
            rotation: 0,
            flip_horizontal: false,
            flip_vertical: false,
        };
        let child = ChildBox {
            x: 10.0,
            y: 60.0,
            width: 50.0,
            height: 50.0,
            ..Default::default()
        };
        let placed = frame.place(child);
        assert!(close(placed.x, 1000.0) && close(placed.y, 2050.0));
        assert!(close(placed.width, 100.0) && close(placed.height, 50.0));
        let through = GroupFrame::place_through(&[frame], child);
        assert!(close(through.x, 0.0) && close(through.y, 50.0));

        let turned = GroupFrame {
            rotation: 5_400_000,
            ..GroupFrame::identity(100.0, 100.0)
        };
        let corner = ChildBox {
            width: 20.0,
            height: 10.0,
            rotation: 600_000,
            ..Default::default()
        };
        let placed = turned.place(corner);
        assert!(close(placed.x + placed.width / 2.0, 95.0));
        assert!(close(placed.y + placed.height / 2.0, 10.0));
        assert_eq!(placed.rotation, 6_000_000);

        let flipped = GroupFrame {
            flip_horizontal: true,
            ..GroupFrame::identity(100.0, 100.0)
        };
        let placed = flipped.place(corner);
        assert!(close(placed.x, 80.0) && close(placed.y, 0.0));
        assert_eq!(placed.rotation, 21_000_000);
        assert!(placed.flip_horizontal && !placed.flip_vertical);

        let quarter = ChildBox {
            width: 20.0,
            height: 10.0,
            rotation: 5_400_000,
            ..Default::default()
        };
        let placed = frame.place(quarter);
        assert!(close(placed.width, 20.0) && close(placed.height, 20.0));
    }
}
